//! The fixture repository (`docs/13-test-plan.md` section 4.1).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs `git` in `dir` and panics on failure. Returns stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Commits all changes with a message. Returns the SHA.
pub fn commit_all(dir: &Path, message: &str) -> String {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Copies the fixture repository into `parent/sample-repo`, runs `git init` and commits.
///
/// With `with_git = false` the folder has no `.git`.
pub fn fixture_repo(parent: &Path, with_git: bool) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join("sample-repo");
    let target = parent.join("sample-repo");
    copy_dir(&source, &target);
    std::fs::write(target.join(".env"), "API_KEY=abc\n").unwrap();
    std::fs::create_dir_all(target.join("assets")).unwrap();
    std::fs::write(target.join("assets").join("icon.bin"), b"BIN\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b").unwrap();
    if with_git {
        git(&target, &["init", "-q", "-b", "main"]);
        commit_all(&target, "initial");
    }
    target
}

/// Makes a small ledger repository for the context-efficiency tests (TS-20) and commits it:
/// late fees, invoices that use them, currency codes, a database module, a 250-line report file
/// and a test file that calls `apply_late_fee`.
pub fn ledger_repo(parent: &Path) -> PathBuf {
    let root = parent.join("ledger");
    let report: String = (1..=125).map(|i| format!("pub fn report_line_{i}() -> u32 {{\n    {i}\n}}\n")).collect::<Vec<_>>().join("");
    let files = [
        ("src/fees.rs", "// Late payment charges for overdue invoices.\npub const LATE_FEE_PERCENT: u32 = 2;\n\npub fn apply_late_fee(amount: i64, days: u32) -> i64 {\n    if days <= grace_days() {\n        return amount;\n    }\n    amount + amount * LATE_FEE_PERCENT as i64 / 100\n}\n\npub fn grace_days() -> u32 {\n    7\n}\n".to_string()),
        ("src/invoice.rs", "use crate::fees::apply_late_fee;\n\npub struct Invoice {\n    pub amount: i64,\n    pub days: u32,\n}\n\npub fn invoice_total(invoice: &Invoice) -> i64 {\n    apply_late_fee(invoice.amount, invoice.days)\n}\n".to_string()),
        ("src/currency.rs", "// Currency symbols and ISO codes.\npub fn normalize_currency(symbol: &str) -> &'static str {\n    match symbol {\n        \"€\" => \"EUR\",\n        \"$\" => \"USD\",\n        _ => \"XXX\",\n    }\n}\n".to_string()),
        ("src/db.rs", "// Database connection settings.\npub const MAX_DB_CONNECTIONS: u32 = 20;\n\npub fn connection_url(host: &str) -> String {\n    format!(\"postgres://{host}/ledger\")\n}\n".to_string()),
        ("src/report.rs", report),
        ("tests/fees_test.rs", "use ledger::fees::apply_late_fee;\n\n#[test]\nfn late_fee_after_grace_days() {\n    assert_eq!(apply_late_fee(100, 10), 102);\n}\n".to_string()),
        ("README.md", "# ledger\n\nInvoices, late fees and currency codes.\n".to_string()),
    ];
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    git(&root, &["init", "-q", "-b", "main"]);
    commit_all(&root, "initial");
    root
}
