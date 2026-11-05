//! JSON-RPC framing of the Language Server Protocol and `file://` URIs.

use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Encodes one message with its `Content-Length` header.
pub fn encode(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

/// Reads one message. `Ok(None)` at the end of the stream.
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let Some(length) = length else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"));
    };
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// `file://` URI of an absolute path. Bytes other than unreserved ASCII and `/` are percent-encoded.
pub fn path_to_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'/' | b'-' | b'_' | b'.' | b'~' | b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' => uri.push(byte as char),
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

/// Path of a `file://` URI. `None` for other schemes or bad escapes.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let mut bytes = Vec::new();
    let mut rest = uri.strip_prefix("file://")?.bytes();
    while let Some(byte) = rest.next() {
        if byte == b'%' {
            let hex = [rest.next()?, rest.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    Some(PathBuf::from(String::from_utf8(bytes).ok()?))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn framing_round_trips_and_skips_extra_headers() {
        let first = serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": null});
        let second = serde_json::json!({"jsonrpc": "2.0", "method": "$/progress", "params": {"text": "ü"}});
        let mut wire = encode(&first);
        wire.extend_from_slice(b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n");
        wire.extend_from_slice(&encode(&second));
        let mut reader = io::BufReader::new(wire.as_slice());
        assert_eq!(read_message(&mut reader).expect("first").expect("some"), first);
        assert_eq!(read_message(&mut reader).expect("second").expect("some"), second);
        assert!(read_message(&mut reader).expect("end").is_none());
    }

    #[test]
    fn uri_round_trips_spaces_and_non_ascii() {
        let path = std::env::temp_dir().join("a b").join("ü.rs");
        let uri = path_to_uri(&path);
        assert!(uri.starts_with("file://") && uri.contains("%20") && !uri.contains(' '));
        assert_eq!(uri_to_path(&uri), Some(path));
        assert_eq!(uri_to_path("https://example.com/a"), None);
    }
}
