//! One language server process: JSON-RPC over its stdin and stdout.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::rpc::{encode, path_to_uri, read_message};

/// Longest wait for the `initialize` answer of a server.
const INIT_TIMEOUT: Duration = Duration::from_secs(60);

type Pending = Arc<Mutex<HashMap<i64, Sender<Value>>>>;
/// Progress tokens that began and did not end yet (indexing, loading).
type Busy = Arc<Mutex<HashSet<String>>>;

/// A running server. Dropping it kills the process.
pub(crate) struct Server {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    busy: Busy,
    next_id: AtomicI64,
    /// Open documents: the version and the text last sent.
    docs: Mutex<HashMap<PathBuf, (i64, String)>>,
    started: Instant,
    used: Mutex<Instant>,
}

impl Server {
    /// Starts `binary`, runs the `initialize` handshake for `root`, and returns the server.
    pub(crate) fn start(binary: &Path, args: &[&str], options: Value, root: &Path) -> io::Result<Self> {
        let mut child = Command::new(binary).args(args).env("PATH", child_path(binary)).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(io::Error::other("no stdio pipes"));
        };
        let stdin = Arc::new(Mutex::new(stdin));
        let (pending, busy) = (Pending::default(), Busy::default());
        let server = Self {
            child,
            stdin: Arc::clone(&stdin),
            pending: Arc::clone(&pending),
            busy: Arc::clone(&busy),
            next_id: AtomicI64::new(1),
            docs: Mutex::new(HashMap::new()),
            started: Instant::now(),
            used: Mutex::new(Instant::now()),
        };
        spawn_reader(stdout, stdin, pending, busy)?;
        let uri = path_to_uri(root);
        let init = json!({
            "processId": std::process::id(),
            "rootUri": uri,
            "workspaceFolders": [{"uri": uri, "name": "root"}],
            "capabilities": {"window": {"workDoneProgress": true}},
            "initializationOptions": options,
        });
        server.request("initialize", init, INIT_TIMEOUT).ok_or_else(|| io::Error::other("initialize failed"))?;
        server.notify("initialized", json!({}))?;
        Ok(server)
    }

    /// True while the server reports work in progress (indexing, loading the workspace).
    pub(crate) fn indexing(&self) -> bool {
        self.busy.lock().is_ok_and(|busy| !busy.is_empty())
    }

    /// Time since the start.
    pub(crate) fn age(&self) -> Duration {
        self.started.elapsed()
    }

    /// Time since the last message sent to the server.
    pub(crate) fn idle(&self) -> Duration {
        self.used.lock().map_or(Duration::ZERO, |used| used.elapsed())
    }

    /// Makes the server's copy of `path` equal to `text` (opens it, or sends the change).
    pub(crate) fn open(&self, path: &Path, text: &str) -> io::Result<()> {
        let mut docs = self.docs.lock().map_err(|_| io::Error::other("docs lock"))?;
        let uri = path_to_uri(path);
        if let Some((version, last)) = docs.get_mut(path) {
            if last != text {
                *version += 1;
                *last = text.to_string();
                let change = json!({"textDocument": {"uri": uri, "version": *version}, "contentChanges": [{"text": text}]});
                return self.notify("textDocument/didChange", change);
            }
            return Ok(());
        }
        docs.insert(path.to_path_buf(), (1, text.to_string()));
        let open = json!({"textDocument": {"uri": uri, "languageId": language_id(path), "version": 1, "text": text}});
        self.notify("textDocument/didOpen", open)
    }

    /// Sends a request and waits for its answer. `None` on an error, a timeout or a closed server.
    pub(crate) fn request(&self, method: &str, params: Value, timeout: Duration) -> Option<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.pending.lock().ok()?.insert(id, tx);
        let sent = self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let reply = if sent.is_ok() { rx.recv_timeout(timeout).ok() } else { None };
        self.pending.lock().ok()?.remove(&id);
        let reply = reply?;
        if reply.get("error").is_some() {
            return None;
        }
        Some(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    fn notify(&self, method: &str, params: Value) -> io::Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn send(&self, message: &Value) -> io::Result<()> {
        if let Ok(mut used) = self.used.lock() {
            *used = Instant::now();
        }
        let mut stdin = self.stdin.lock().map_err(|_| io::Error::other("stdin lock"))?;
        stdin.write_all(&encode(message))?;
        stdin.flush()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Reads the server's messages: answers go to the waiting request, server requests get a null
/// answer (`workspace/configuration` gets one null per item), progress updates the busy set.
fn spawn_reader(stdout: ChildStdout, stdin: Arc<Mutex<ChildStdin>>, pending: Pending, busy: Busy) -> io::Result<()> {
    std::thread::Builder::new().name("zcie-lsp-read".into()).spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Ok(Some(message)) = read_message(&mut reader) {
            let method = message.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
            match (method.as_str(), message.get("id").cloned()) {
                ("", Some(id)) => {
                    let waiting = id.as_i64().and_then(|id| pending.lock().ok()?.remove(&id));
                    if let Some(tx) = waiting {
                        let _ = tx.send(message);
                    }
                }
                ("$/progress", _) => track(&busy, &message["params"]),
                (_, Some(id)) => {
                    let result = if method == "workspace/configuration" {
                        json!(vec![Value::Null; message["params"]["items"].as_array().map_or(0, Vec::len)])
                    } else {
                        Value::Null
                    };
                    if let Ok(mut out) = stdin.lock() {
                        let _ = out.write_all(&encode(&json!({"jsonrpc": "2.0", "id": id, "result": result})));
                        let _ = out.flush();
                    }
                }
                _ => {}
            }
        }
    })?;
    Ok(())
}

fn track(busy: &Mutex<HashSet<String>>, params: &Value) {
    let token = params["token"].to_string();
    if let Ok(mut set) = busy.lock() {
        match params["value"]["kind"].as_str() {
            Some("begin") => {
                set.insert(token);
            }
            Some("end") => {
                set.remove(&token);
            }
            _ => {}
        }
    }
}

/// The folder of the server binary first (a node script finds its `node` there), then the search folders.
fn child_path(binary: &Path) -> OsString {
    let dirs = binary.parent().map(Path::to_path_buf).into_iter().chain(crate::search_dirs());
    std::env::join_paths(dirs).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

/// The LSP `languageId` of a file.
fn language_id(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("tsx") => "typescriptreact",
        Some("jsx") => "javascriptreact",
        Some("js" | "mjs" | "cjs") => "javascript",
        _ => "typescript",
    }
}
