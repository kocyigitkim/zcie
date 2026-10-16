//! TS-02: security (docs/13-test-plan.md section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use zcie_security::*;

fn clients_with(key: &str) -> ClientsFile {
    let mut clients = ClientsFile::default();
    clients.add(ClientEntry {
        name: "claude-code".into(),
        key_hash: hash_key(key),
        principal_id: "user-001".into(),
        workspaces: vec!["atheron".into()],
        permissions: vec![Permission::Read, Permission::Write],
    });
    clients
}

#[test]
fn authenticate_with_correct_key_gives_principal() {
    let key = generate_key().unwrap();
    let auth = authenticate(Some(&format!("Bearer {key}")), &clients_with(&key), None).unwrap();
    assert_eq!(auth.principal_id, "user-001");
    assert!(auth.can(Permission::Write));
    assert!(!auth.is_admin());
}

#[test]
fn authenticate_with_wrong_or_missing_key_is_unauthorized() {
    let clients = clients_with(&generate_key().unwrap());
    assert!(matches!(authenticate(Some("Bearer zcie_wrong"), &clients, None), Err(Error::Unauthorized)));
    assert!(matches!(authenticate(None, &clients, None), Err(Error::Unauthorized)));
    assert!(matches!(authenticate(Some("Basic abc"), &clients, None), Err(Error::Unauthorized)));
}

#[test]
fn each_redaction_rule_replaces_its_secret() {
    let zcie_key = format!("zcie_{}", "a".repeat(64));
    let cases = [
        ("R-01", "key AKIAABCDEFGHIJKLMNOP here".to_string()),
        ("R-02", "-----BEGIN RSA PRIVATE KEY-----\nMIIabc\n-----END RSA PRIVATE KEY-----".to_string()),
        ("R-03", "Authorization: Bearer abcdefghijklmnopqrstuv".to_string()),
        ("R-04", "password = hunter2hunter2".to_string()),
        ("R-05", format!("token ghp_{}", "A".repeat(36))),
        ("R-06", "sk-ant-abcdefghijklmnopqrstuvwxyz".to_string()),
        ("R-07", format!("use {zcie_key}")),
        ("R-08", "jwt eyJhbGciOiJIUzI1.eyJzdWIiOiIxMjM0.SflKxwRJSMeKKF2QT4".to_string()),
    ];
    for (id, text) in cases {
        let (out, count) = redact(&text);
        assert!(out.contains(&format!("[REDACTED:{id}]")), "{id}: {out}");
        assert!(count >= 1, "{id}");
    }
}

#[test]
fn redaction_counts_multiple_secrets() {
    let (out, count) = redact("a AKIAABCDEFGHIJKLMNOP b ASIAABCDEFGHIJKLMNOP");
    assert_eq!(count, 2);
    assert!(!out.contains("AKIA"));
}

#[test]
fn text_without_secrets_is_unchanged() {
    let text = "Fix weapon attachment during animation #89";
    assert_eq!(redact(text), (text.to_string(), 0));
}

#[test]
fn exclude_set_matches_env_but_not_source() {
    let set = ExcludeSet::new(&[".env".to_string()]).unwrap();
    assert!(set.is_excluded(".env"));
    assert!(set.is_excluded("sub/.env"));
    assert!(!set.is_excluded("src/main.rs"));
    assert!(set.is_excluded(".git/config"));
}

#[test]
fn default_exclude_set_covers_keys_and_build_output() {
    let set = ExcludeSet::with_defaults(&[]).unwrap();
    assert!(set.is_excluded(".env"));
    assert!(set.is_excluded("certs/server.pem"));
    assert!(set.is_excluded("node_modules/x/index.js"));
    assert!(!set.is_excluded("src/lib.rs"));
}

#[test]
fn generated_keys_are_distinct_hex() {
    let a = generate_key().unwrap();
    let b = generate_key().unwrap();
    assert_ne!(a, b);
    for key in [a, b] {
        let hex = key.strip_prefix("zcie_").unwrap();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
