//! `zcie telemetry export`: the activity log as OpenTelemetry traces in OTLP/JSON (ADR-032).

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Subcommand;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

/// Spans per POST to the collector.
const BATCH: usize = 1000;

/// Telemetry actions.
#[derive(Debug, Subcommand)]
pub enum TelemetryAction {
    /// Export the activity log as OpenTelemetry traces (OTLP/JSON, ADR-032).
    Export {
        /// Days of the window: the lines whose `ts` is in the last N days.
        #[arg(long, default_value_t = 1)]
        days: u32,
        /// Collector address; `/v1/traces` is added. Also `OTEL_EXPORTER_OTLP_ENDPOINT`.
        #[arg(long)]
        endpoint: Option<String>,
        /// File to write the JSON to (default: stdout). Not used when posting to an endpoint.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

/// Runs one telemetry action.
pub fn run(action: TelemetryAction, home: &Path) -> anyhow::Result<()> {
    match action {
        TelemetryAction::Export { days, endpoint, out } => {
            let endpoint = endpoint.or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok()).filter(|e| !e.is_empty());
            export(home, days, endpoint, out.as_deref())
        }
    }
}

fn export(home: &Path, days: u32, endpoint: Option<String>, out: Option<&Path>) -> anyhow::Result<()> {
    // The daemon rotates the log to `activity.1.jsonl` (ADR-023): read the older file first.
    let mut text = String::new();
    for name in ["activity.1.jsonl", "activity.jsonl"] {
        let path = home.join("stats").join(name);
        match std::fs::read_to_string(&path) {
            Ok(part) => text.push_str(&part),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }
    let since = time::OffsetDateTime::now_utc() - time::Duration::days(i64::from(days));
    let spans: Vec<Value> = text.lines().filter_map(|line| span(line, since)).collect();
    if let Some(endpoint) = endpoint {
        let url = format!("{}/v1/traces", endpoint.trim_end_matches('/'));
        let client = reqwest::blocking::Client::new();
        for batch in spans.chunks(BATCH) {
            let response = client.post(&url).json(&document(batch)).send()?;
            if !response.status().is_success() {
                bail!("{url} answered {}", response.status());
            }
        }
        eprintln!("{} spans exported", spans.len());
        return Ok(());
    }
    let body = serde_json::to_string_pretty(&document(&spans))?;
    match out {
        Some(file) => std::fs::write(file, body).with_context(|| format!("writing {}", file.display()))?,
        None => println!("{body}"),
    }
    eprintln!("{} spans written", spans.len());
    Ok(())
}

/// Converts one activity line into a span; None if it does not parse, has no `ts`, or is older than `since`.
fn span(line: &str, since: time::OffsetDateTime) -> Option<Value> {
    let Value::Object(fields) = serde_json::from_str::<Value>(line).ok()? else { return None };
    let end = time::OffsetDateTime::parse(fields.get("ts")?.as_str()?, &Rfc3339).ok()?;
    if end < since {
        return None;
    }
    let op = fields.get("op").and_then(Value::as_str).unwrap_or("unknown");
    let tool = match op {
        "query" => Some("zcie_query"),
        "expand" => Some("zcie_expand"),
        _ => None,
    };
    let end_ns = end.unix_timestamp_nanos();
    let ms = fields.get("ms").and_then(Value::as_f64).unwrap_or(0.0);
    let start_ns = end_ns - (ms * 1e6) as i128;
    let mut attributes = vec![kv("gen_ai.operation.name", json!({"stringValue": if tool.is_some() { "execute_tool".to_string() } else { format!("zcie.{op}") }}))];
    if let Some(tool) = tool {
        attributes.push(kv("gen_ai.tool.name", json!({"stringValue": tool})));
    }
    for (key, value) in &fields {
        if key == "ts" || key == "op" {
            continue;
        }
        let key = if key == "ws" { "zcie.workspace".to_string() } else { format!("zcie.{key}") };
        if let Some(value) = any_value(value) {
            attributes.push(kv(&key, value));
        }
    }
    let code = if fields.get("ok") == Some(&Value::Bool(false)) { 2 } else { 1 };
    let hash = blake3::hash(line.as_bytes()).to_hex();
    let hex = hash.as_str();
    Some(json!({
        "traceId": &hex[..32],
        "spanId": &hex[32..48],
        "name": format!("zcie.{op}"),
        "kind": 1,
        "startTimeUnixNano": start_ns.to_string(),
        "endTimeUnixNano": end_ns.to_string(),
        "attributes": attributes,
        "status": {"code": code},
    }))
}

/// One OTLP attribute.
fn kv(key: &str, value: Value) -> Value {
    json!({"key": key, "value": value})
}

/// The OTLP value of a scalar field; None for null, arrays and objects.
fn any_value(value: &Value) -> Option<Value> {
    Some(match value {
        Value::String(s) => json!({"stringValue": s}),
        Value::Bool(b) => json!({"boolValue": b}),
        Value::Number(n) if n.is_f64() => json!({"doubleValue": n.as_f64()}),
        Value::Number(n) => json!({"intValue": n.to_string()}),
        _ => return None,
    })
}

/// One OTLP/JSON document with the spans of ZCIE.
fn document(spans: &[Value]) -> Value {
    json!({"resourceSpans": [{
        "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "zcied"}}]},
        "scopeSpans": [{"scope": {"name": "zcie"}, "spans": spans}]
    }]})
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::span;

    #[test]
    fn an_activity_line_becomes_a_span() {
        // ADR-032, D-109 (TS-32-02).
        let since = time::OffsetDateTime::UNIX_EPOCH;
        let line = r#"{"op":"query","ms":2.5,"ok":false,"keys":3,"ts":"2026-10-23T21:31:16.120Z","ws":"proj_1"}"#;
        let s = span(line, since).unwrap();
        assert_eq!(s["name"], "zcie.query");
        assert_eq!(s["endTimeUnixNano"], "1792791076120000000");
        assert_eq!(s["startTimeUnixNano"], "1792791076117500000");
        assert_eq!(s["status"]["code"], 2);
        assert_eq!(s["traceId"].as_str().unwrap().len(), 32);
        let attr = |k: &str| s["attributes"].as_array().unwrap().iter().find(|a| a["key"] == k).map(|a| a["value"].clone());
        assert_eq!(attr("gen_ai.operation.name").unwrap()["stringValue"], "execute_tool");
        assert_eq!(attr("gen_ai.tool.name").unwrap()["stringValue"], "zcie_query");
        assert_eq!(attr("zcie.workspace").unwrap()["stringValue"], "proj_1");
        assert_eq!(attr("zcie.keys").unwrap()["intValue"], "3");
        assert!(span("not json", since).is_none());
        assert!(span(line, time::OffsetDateTime::now_utc() + time::Duration::days(1)).is_none());
    }
}
