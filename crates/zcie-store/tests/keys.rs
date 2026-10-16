//! TS-01: keys (docs/13-test-plan.md section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use zcie_store::{key, prefix, seq_part};

#[test]
fn key_joins_parts_with_zero_byte() {
    assert_eq!(key(&["local", "idem", "k1"]), b"local\x00idem\x00k1".to_vec());
    assert_eq!(prefix(&["local", "idem"]), b"local\x00idem\x00".to_vec());
}

#[test]
fn seq_part_sorts_numerically_as_bytes() {
    assert_eq!(seq_part(42), "00000000000000000042");
    assert!(seq_part(9).as_bytes() < seq_part(10).as_bytes());
    assert!(seq_part(999).as_bytes() < seq_part(1000).as_bytes());
}
