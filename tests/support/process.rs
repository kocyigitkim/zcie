//! The real `zcied` binary as a child process (crash and end-to-end tests).

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Once;
use std::time::{Duration, Instant};

use serde_json::Value;

/// Maximum wait for the daemon health check.
const START_TIMEOUT: Duration = Duration::from_secs(30);

/// Returns the workspace root folder.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root").to_path_buf()
}

/// Builds the binaries once per test process: `zcied`, `zcie`, `zcie-mcp`.
pub fn build_binaries() {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .current_dir(workspace_root())
            .args(["build", "-q", "-p", "zcied", "-p", "zcie", "-p", "zcie-mcp"])
            .status()
            .expect("cargo build runs");
        assert!(status.success(), "binaries did not build");
    });
}

/// Returns the path of a debug binary.
pub fn binary(name: &str) -> PathBuf {
    build_binaries();
    workspace_root().join("target").join("debug").join(name)
}

/// Returns a free TCP port on 127.0.0.1.
pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").expect("bind").local_addr().expect("addr").port()
}

/// A `zcied` child process.
pub struct DaemonProcess {
    child: Option<Child>,
    /// The home folder.
    pub home: PathBuf,
    /// The HTTP port.
    pub port: u16,
}

impl DaemonProcess {
    /// Starts `zcied --home <home>` on `port` and waits for the health check.
    pub fn spawn(home: &Path, port: u16) -> DaemonProcess {
        std::fs::create_dir_all(home).expect("home");
        let log = File::options().create(true).append(true).open(home.join("zcied.test.log")).expect("log");
        let child = Command::new(binary("zcied"))
            .arg("--home")
            .arg(home)
            .env("ZCIE_SERVER_HTTP_ADDR", format!("\"127.0.0.1:{port}\""))
            .env("ZCIE_INGEST_DEBOUNCE_MS", "50")
            .env_remove("ZCIE_API_KEY")
            .stdin(Stdio::null())
            .stdout(log.try_clone().expect("log"))
            .stderr(log)
            .spawn()
            .expect("zcied starts");
        let daemon = DaemonProcess { child: Some(child), home: home.to_path_buf(), port };
        daemon.wait_healthy();
        daemon
    }

    /// Returns the base URL.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Returns the admin key that the daemon wrote at first start.
    pub fn admin_key(&self) -> String {
        std::fs::read_to_string(self.home.join("config").join("admin.key")).expect("admin.key").trim().to_string()
    }

    /// Returns an SDK client for a workspace with the admin key.
    pub fn client(&self, workspace: &str) -> zcie_sdk::Client {
        zcie_sdk::Client::new(&self.url(), &self.admin_key(), workspace)
    }

    fn wait_healthy(&self) {
        let client = zcie_sdk::Client::new(&self.url(), "", "x");
        let started = Instant::now();
        while started.elapsed() < START_TIMEOUT {
            if client.health().is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("zcied did not start; see {}", self.home.join("zcied.test.log").display());
    }

    /// Waits until the queue is empty and all watermarks reach the canonical watermark.
    pub fn wait_idle(&self) -> zcie_types::api::HealthReport {
        let client = self.client("x");
        let started = Instant::now();
        while started.elapsed() < START_TIMEOUT {
            if let Ok(health) = client.health()
                && let (Some(wm), Some(queue)) = (health.watermarks, health.queue)
                && queue.interactive == 0
                && queue.background == 0
                && wm.graph_seq >= wm.canonical_seq
                && wm.search_seq >= wm.canonical_seq
            {
                std::thread::sleep(Duration::from_millis(300));
                return client.health().expect("health");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("zcied did not become idle");
    }

    /// Waits until the fixture repository is indexed: a symbol query finds `attach_to_bone`.
    pub fn wait_fixture_indexed(&self, workspace: &str) {
        let client = self.client(workspace);
        let started = Instant::now();
        while started.elapsed() < START_TIMEOUT {
            let found = client.query("where is attach_to_bone defined", 250).ok().and_then(|r| r.answer).is_some();
            if found {
                self.wait_idle();
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("fixture repository was not indexed");
    }

    /// Stops the daemon with SIGKILL (a crash).
    pub fn kill9(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Stops the daemon with SIGTERM and waits for the exit.
    pub fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = Command::new("kill").arg("-TERM").arg(child.id().to_string()).status();
            let _ = child.wait();
        }
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        self.kill9();
    }
}

/// Sends JSON-RPC lines to `zcie-mcp` and returns the responses with an `id`.
pub fn mcp_session(daemon: &DaemonProcess, workspace: &str, calls: &[Value]) -> Vec<Value> {
    let mut child = Command::new(binary("zcie-mcp"))
        .env("ZCIE_URL", daemon.url())
        .env("ZCIE_API_KEY", daemon.admin_key())
        .env("ZCIE_WORKSPACE", workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let init = serde_json::json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}});
    writeln!(stdin, "{init}").unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut responses = vec![read_response(&mut reader)];
    writeln!(stdin, "{}", serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"})).unwrap();
    for call in calls {
        writeln!(stdin, "{call}").unwrap();
        responses.push(read_response(&mut reader));
    }
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    responses
}

fn read_response(reader: &mut impl BufRead) -> Value {
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0, "zcie-mcp closed stdout");
        let value: Value = serde_json::from_str(line.trim()).unwrap();
        if value.get("id").is_some() {
            return value;
        }
    }
}

