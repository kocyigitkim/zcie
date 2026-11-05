//! Real servers on small temporary projects. Each test prints a skip line when its server is not installed.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use zcie_lsp::{Config, Location, Pool};

fn pool() -> Pool {
    Pool::new(Config { enabled: true, idle: Duration::from_secs(600), timeout: Duration::from_secs(30) })
}

/// Asks until the server answers with references, for up to 60 seconds.
fn wait_for_references(pool: &Pool, root: &Path, file: &Path, line: u32, column: u32) -> Vec<Location> {
    let text = std::fs::read_to_string(file).expect("read file");
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        if let Some(found) = pool.references(root, file, &text, line, column).filter(|f| !f.is_empty()) {
            return found;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Vec::new()
}

/// 0-based line and column of the first `needle` of a file.
fn position(file: &Path, needle: &str) -> (u32, u32) {
    let text = std::fs::read_to_string(file).expect("read file");
    let (line, text_line) = text.lines().enumerate().find(|(_, l)| l.contains(needle)).expect("needle");
    (line as u32, text_line.find(needle).expect("column") as u32)
}

#[test]
fn rust_analyzer_finds_method_call_sites() {
    let lib = Path::new("/nonexistent/lib.rs");
    if !zcie_lsp::server_available(lib) {
        eprintln!("skipped: rust-analyzer not found");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let root: PathBuf = std::fs::canonicalize(dir.path()).expect("canonical");
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"meter\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").expect("manifest");
    std::fs::write(root.join("src/lib.rs"), "pub mod a;\npub mod b;\n\npub struct Meter;\n\nimpl Meter {\n    pub fn tick(&self) {}\n}\n").expect("lib");
    std::fs::write(root.join("src/a.rs"), "use crate::Meter;\n\npub fn run() {\n    let m = Meter;\n    m.tick();\n}\n").expect("a");
    std::fs::write(root.join("src/b.rs"), "use crate::Meter;\n\npub fn run() {\n    let m = Meter;\n    m.tick();\n}\n").expect("b");
    let file = root.join("src/lib.rs");
    let (line, column) = position(&file, "fn tick");
    let started = Instant::now();
    let found = wait_for_references(&pool(), &root, &file, line, column + 3);
    eprintln!("rust-analyzer answered after {:?}", started.elapsed());
    let mut paths: Vec<String> = found.iter().map(|l| l.path.strip_prefix(&root).expect("inside").to_string_lossy().into_owned()).collect();
    paths.sort();
    assert_eq!(paths, ["src/a.rs", "src/b.rs"]);
    assert_eq!(found.iter().map(|l| l.line).collect::<Vec<u32>>(), vec![5, 5]);
}

/// A TypeScript package folder with `lib/tsserver.js` (TypeScript 7 has none).
fn typescript_with_tsserver() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let cache = PathBuf::from(home).join(".npm/_npx");
    std::fs::read_dir(cache).ok()?.filter_map(Result::ok).map(|e| e.path().join("node_modules/typescript")).find(|p| p.join("lib/tsserver.js").is_file())
}

#[test]
fn typescript_server_finds_import_call_sites() {
    let file = Path::new("/nonexistent/a.ts");
    if !zcie_lsp::server_available(file) {
        eprintln!("skipped: typescript-language-server not found");
        return;
    }
    let Some(typescript) = typescript_with_tsserver() else {
        eprintln!("skipped: no TypeScript 5 or 6 (lib/tsserver.js) found in ~/.npm/_npx");
        return;
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let root: PathBuf = std::fs::canonicalize(dir.path()).expect("canonical");
    // The server finds TypeScript in the project's node_modules, as in a real project.
    std::fs::create_dir(root.join("node_modules")).expect("node_modules");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&typescript, root.join("node_modules/typescript")).expect("link");
    #[cfg(not(unix))]
    {
        let _ = typescript;
        eprintln!("skipped: the symlink of node_modules needs unix");
        return;
    }
    std::fs::write(root.join("tsconfig.json"), "{\"compilerOptions\": {\"strict\": true, \"module\": \"esnext\", \"target\": \"es2020\"}, \"include\": [\"*.ts\"]}\n").expect("tsconfig");
    std::fs::write(root.join("a.ts"), "export function computeVat(x: number): number {\n  return x * 0.2;\n}\n").expect("a");
    std::fs::write(root.join("b.ts"), "import { computeVat } from \"./a\";\n\nexport const b = computeVat(1);\n").expect("b");
    std::fs::write(root.join("c.ts"), "import { computeVat } from \"./a\";\n\nexport const c = computeVat(2);\n").expect("c");
    let file = root.join("a.ts");
    let (line, column) = position(&file, "computeVat");
    let found = wait_for_references(&pool(), &root, &file, line, column + 2);
    // The import specifier and the call are both references (LSP semantics): line 1 and line 3.
    let mut sites: Vec<(String, u32)> = found.iter().map(|l| (l.path.strip_prefix(&root).expect("inside").to_string_lossy().into_owned(), l.line)).collect();
    sites.sort();
    let expected = [("b.ts", 1), ("b.ts", 3), ("c.ts", 1), ("c.ts", 3)].map(|(p, l)| (p.to_string(), l));
    assert_eq!(sites, expected);
}
