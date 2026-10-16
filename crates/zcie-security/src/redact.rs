//! Secret redaction (`docs/11-security.md` section 5).

use std::sync::OnceLock;

use regex::Regex;

/// Redaction rules: rule ID and regex.
const RULES: [(&str, &str); 8] = [
    ("R-01", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
    ("R-02", r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----"),
    ("R-03", r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{16,}"),
    ("R-04", r#"(?i)\b(?:api[_-]?key|secret|token|password|passwd)\b\s*[:=]\s*["']?[^\s"']{8,}"#),
    ("R-05", r"\bgh[pousr]_[A-Za-z0-9]{36,}\b"),
    ("R-06", r"\bsk-(?:ant-)?[A-Za-z0-9_-]{20,}\b"),
    ("R-07", r"\bzcie_[0-9a-f]{64}\b"),
    ("R-08", r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b"),
];

fn compiled_rules() -> &'static Vec<(&'static str, Regex)> {
    static COMPILED: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        RULES
            .iter()
            .filter_map(|(id, pattern)| Regex::new(pattern).ok().map(|re| (*id, re)))
            .collect()
    })
}

/// Replaces each secret with `[REDACTED:<rule id>]`. Returns the new text and the count.
pub fn redact(text: &str) -> (String, u32) {
    let mut output = text.to_string();
    let mut count = 0u32;
    for (id, regex) in compiled_rules() {
        let matches = regex.find_iter(&output).count() as u32;
        if matches == 0 {
            continue;
        }
        count += matches;
        let marker = format!("[REDACTED:{id}]");
        output = regex.replace_all(&output, marker.as_str()).into_owned();
    }
    (output, count)
}
