//! `zcie capture`: send one event to the daemon.

use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Args;
use zcie_sdk::Client;
use zcie_types::EventSource;
use zcie_types::api::CaptureRequest;
use zcie_types::event::EVENT_SCHEMA;

use super::print_json;

/// Arguments of `zcie capture`.
#[derive(Debug, Args)]
pub struct CaptureArgs {
    /// Event type, for example user.message.
    #[arg(long = "type")]
    pub event_type: String,
    /// Client session ID.
    #[arg(long)]
    pub session: String,
    /// Client turn ID.
    #[arg(long)]
    pub turn: Option<String>,
    /// Client application name.
    #[arg(long, default_value = "cli")]
    pub app: String,
    /// Idempotency key. Default `cli:<session>:<turn or time>`.
    #[arg(long = "key-id")]
    pub key_id: Option<String>,
    /// Optional repository ID.
    #[arg(long)]
    pub repository: Option<String>,
    /// Payload text, or `-` to read stdin.
    pub text: String,
}

/// Runs `zcie capture`.
pub fn run(args: CaptureArgs, client: &Client) -> anyhow::Result<()> {
    let payload = read_payload(&args.text)?;
    let key = args.key_id.clone().unwrap_or_else(|| default_key(&args));
    let request = CaptureRequest {
        schema: EVENT_SCHEMA.to_string(),
        idempotency_key: key,
        timestamp_utc: None,
        workspace: client.workspace().to_string(),
        repository_id: args.repository,
        source: EventSource { app: args.app, session_id: args.session, turn_id: args.turn },
        event_type: args.event_type,
        payload: Some(payload),
        git: None,
        classification: None,
    };
    print_json(&client.capture(request)?)
}

fn read_payload(text: &str) -> anyhow::Result<String> {
    if text != "-" {
        return Ok(text.to_string());
    }
    let mut buffer = String::new();
    std::io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn default_key(args: &CaptureArgs) -> String {
    let suffix = match &args.turn {
        Some(turn) => turn.clone(),
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos().to_string())
            .unwrap_or_default(),
    };
    format!("cli:{}:{}:{suffix}", args.session, args.event_type)
}
