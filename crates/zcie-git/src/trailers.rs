//! Commit message trailers, for example `ZCIE-Task: #89`.

use std::sync::OnceLock;

use regex::Regex;

/// Name of the task trailer.
pub const TASK_TRAILER: &str = "ZCIE-Task";

fn trailer_regex() -> Option<&'static Regex> {
    static REGEX: OnceLock<Option<Regex>> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"^([A-Za-z][A-Za-z0-9-]*):\s+(.+)$").ok())
        .as_ref()
}

/// Returns all `Key: value` lines of a message.
pub fn parse_trailers(message: &str) -> Vec<(String, String)> {
    let Some(regex) = trailer_regex() else { return Vec::new() };
    message
        .lines()
        .filter_map(|line| regex.captures(line.trim()))
        .filter_map(|c| Some((c.get(1)?.as_str().to_string(), c.get(2)?.as_str().trim().to_string())))
        .collect()
}

/// Returns the values of `ZCIE-Task` trailers. Comma-separated values are split.
pub fn task_trailers(message: &str) -> Vec<String> {
    parse_trailers(message)
        .into_iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(TASK_TRAILER))
        .flat_map(|(_, value)| {
            value.split(',').map(|v| v.trim().to_string()).collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .collect()
}
