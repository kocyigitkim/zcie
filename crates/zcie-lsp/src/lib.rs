//! Optional language-server enrichment (ADR-033): "who calls X" for Rust (rust-analyzer) and
//! TypeScript/JavaScript (typescript-language-server). Servers start on demand, stop when idle,
//! and every answer is optional: `None` means "not ready or not available", and the caller keeps
//! its own answer.

mod rpc;
mod server;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::rpc::uri_to_path;
use crate::server::Server;

/// Lowest wait between two start attempts of a server that failed or is missing.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// An empty answer in this time after the start means "still indexing".
const EMPTY_IS_NOT_READY_FOR: Duration = Duration::from_secs(60);
/// Longest interval of the idle check.
const REAP_EVERY: Duration = Duration::from_secs(30);

/// Settings of the pool.
#[derive(Debug, Clone)]
pub struct Config {
    /// False: `references` always returns `None` and no server starts.
    pub enabled: bool,
    /// A server without a message for this long is stopped.
    pub idle: Duration,
    /// Longest wait for one answer.
    pub timeout: Duration,
}

/// One reference: the absolute file and its 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// File path, as the server reports it.
    pub path: PathBuf,
    /// 1-based line, like the spans of ZCIE.
    pub line: u32,
}

/// Pool of language servers, one per (language, server root). Safe to share between threads.
pub struct Pool {
    inner: Arc<Inner>,
}

enum Entry {
    Starting,
    Failed(Instant),
    Ready(Arc<Server>),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Language {
    Rust,
    Typescript,
}

struct Inner {
    config: Config,
    servers: Mutex<HashMap<(Language, PathBuf), Entry>>,
}

impl Pool {
    /// Makes the pool. With `enabled` set, a thread stops the idle servers; it ends with the pool.
    pub fn new(config: Config) -> Self {
        let inner = Arc::new(Inner { config: config.clone(), servers: Mutex::new(HashMap::new()) });
        if config.enabled {
            let weak = Arc::downgrade(&inner);
            let every = config.idle.min(REAP_EVERY).max(Duration::from_millis(100));
            let _ = std::thread::Builder::new().name("zcie-lsp-reap".into()).spawn(move || {
                loop {
                    std::thread::sleep(every);
                    let Some(inner) = weak.upgrade() else { break };
                    inner.reap();
                }
            });
        }
        Self { inner }
    }

    /// Returns the references of the symbol at `line` (0-based) and `column` (0-based, in
    /// characters) of `file`, whose current text is `text`; the declaration is not included.
    /// `None`: disabled, unsupported file, server not ready, missing, failed or too slow.
    /// An answer of no references comes back as `Some` only after the server has been up for a minute.
    pub fn references(&self, root: &Path, file: &Path, text: &str, line: u32, column: u32) -> Option<Vec<Location>> {
        if !self.inner.config.enabled {
            return None;
        }
        let language = Language::of(file)?;
        let server = self.inner.server(language, language.root(root, file))?;
        if server.indexing() {
            return None;
        }
        let character = utf16_column(text, line, column)?;
        server.open(file, text).ok()?;
        let params = json!({
            "textDocument": {"uri": rpc::path_to_uri(file)},
            "position": {"line": line, "character": character},
            "context": {"includeDeclaration": false},
        });
        let result = server.request("textDocument/references", params, self.inner.config.timeout)?;
        let items = result.as_array().cloned().unwrap_or_default();
        if items.is_empty() && server.age() < EMPTY_IS_NOT_READY_FOR {
            return None;
        }
        Some(items.iter().filter_map(location).collect())
    }
}

impl Inner {
    /// Returns a running server, or starts one in the background and returns `None`.
    fn server(self: &Arc<Self>, language: Language, root: PathBuf) -> Option<Arc<Server>> {
        let key = (language, root.clone());
        let mut servers = self.servers.lock().ok()?;
        match servers.get(&key) {
            Some(Entry::Ready(server)) => return Some(Arc::clone(server)),
            Some(Entry::Starting) => return None,
            Some(Entry::Failed(at)) if at.elapsed() < RETRY_AFTER => return None,
            _ => {}
        }
        servers.insert(key.clone(), Entry::Starting);
        let (weak, failed_key) = (Arc::downgrade(self), key.clone());
        let spawned = std::thread::Builder::new().name("zcie-lsp-start".into()).spawn(move || {
            let server = language.binary().and_then(|binary| Server::start(&binary, language.args(), language.options(&binary), &root).ok());
            let Some(inner) = weak.upgrade() else { return };
            if let Ok(mut servers) = inner.servers.lock() {
                servers.insert(key, server.map_or(Entry::Failed(Instant::now()), |s| Entry::Ready(Arc::new(s))));
            }
        });
        if spawned.is_err() {
            servers.insert(failed_key, Entry::Failed(Instant::now()));
        }
        None
    }

    /// Stops the servers that were idle longer than the configured time.
    fn reap(&self) {
        if let Ok(mut servers) = self.servers.lock() {
            servers.retain(|_, entry| !matches!(entry, Entry::Ready(server) if server.idle() > self.config.idle));
        }
    }
}

impl Language {
    /// The language of a file by its extension.
    fn of(file: &Path) -> Option<Self> {
        match file.extension()?.to_str()? {
            "rs" => Some(Self::Rust),
            "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs" => Some(Self::Typescript),
            _ => None,
        }
    }

    /// The folder the server starts in, between `file` and `root`. Rust: the topmost Cargo
    /// workspace, else the nearest `Cargo.toml`. TypeScript: the nearest project file.
    fn root(self, root: &Path, file: &Path) -> PathBuf {
        let markers: &[&str] = match self {
            Self::Rust => &["Cargo.toml"],
            Self::Typescript => &["tsconfig.json", "jsconfig.json", "package.json"],
        };
        let found: Vec<PathBuf> = file
            .ancestors()
            .skip(1)
            .take_while(|dir| dir.starts_with(root))
            .filter(|dir| markers.iter().any(|m| dir.join(m).is_file()))
            .map(Path::to_path_buf)
            .collect();
        let workspace = found.iter().rev().find(|dir| self == Self::Rust && std::fs::read_to_string(dir.join("Cargo.toml")).is_ok_and(|t| t.contains("[workspace]")));
        workspace.or(found.first()).cloned().unwrap_or_else(|| root.to_path_buf())
    }

    fn binary(self) -> Option<PathBuf> {
        let name = match self {
            Self::Rust => "rust-analyzer",
            Self::Typescript => "typescript-language-server",
        };
        search_dirs().iter().map(|dir| dir.join(name)).find(|path| path.is_file())
    }

    /// `initializationOptions`. typescript-language-server finds no TypeScript by itself, so the
    /// `tsserver.js` of the npm prefix of its binary is given (`<prefix>/bin/<server>`).
    fn options(self, binary: &Path) -> Value {
        match self {
            Self::Rust => json!({}),
            Self::Typescript => {
                let tsserver = binary.parent().and_then(Path::parent).map(|prefix| prefix.join("lib/node_modules/typescript/lib/tsserver.js"));
                tsserver.filter(|p| p.is_file()).map_or(json!({}), |p| json!({"tsserver": {"path": p}}))
            }
        }
    }

    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[],
            Self::Typescript => &["--stdio"],
        }
    }
}

/// True when the server of the file's language is installed (tests skip when it is not).
pub fn server_available(file: &Path) -> bool {
    Language::of(file).and_then(Language::binary).is_some()
}

/// The folders searched for a server binary: the PATH, then the usual install folders, then the
/// newest Node.js of nvm (a launchd daemon has a small PATH).
pub(crate) fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".cargo").join("bin"));
        dirs.extend(newest_node_bin(&home));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    dirs
}

/// The `bin` folder of the newest Node.js under `~/.nvm/versions/node/<version>/bin`.
fn newest_node_bin(home: &Path) -> Option<PathBuf> {
    let mut versions: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(home.join(".nvm/versions/node"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|dir| {
            let version = dir.file_name()?.to_str()?.trim_start_matches('v').split('.').map(|n| n.parse().ok()).collect::<Option<Vec<u32>>>()?;
            Some((version, dir.join("bin")))
        })
        .collect();
    versions.sort();
    versions.pop().map(|(_, bin)| bin)
}

/// Column of a character index in UTF-16 units, which LSP positions use.
fn utf16_column(text: &str, line: u32, column: u32) -> Option<u32> {
    let text_line = text.lines().nth(line as usize)?;
    let units: usize = text_line.chars().take(column as usize).map(char::len_utf16).sum();
    u32::try_from(units).ok()
}

/// A location of a `Location` item of an LSP answer.
fn location(item: &Value) -> Option<Location> {
    let path = uri_to_path(item["uri"].as_str()?)?;
    let line = u32::try_from(item["range"]["start"]["line"].as_u64()?).ok()?;
    Some(Location { path, line: line + 1 })
}
