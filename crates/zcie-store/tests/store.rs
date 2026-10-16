//! TS-03: store and object store (docs/13-test-plan.md section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use zcie_store::codec::NumberValue;
use zcie_store::events::BlobValue;
use zcie_store::watermarks::{GRAPH, raise_watermark, read_watermark};
use zcie_store::{Batch, Cf, Error, ObjectStore, Store, key, layout, prefix};

fn open() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("db")).unwrap();
    (dir, store)
}

#[test]
fn new_database_has_all_nine_column_families() {
    let (_dir, store) = open();
    assert_eq!(Cf::ALL.len(), 9);
    for cf in Cf::ALL {
        store.put_sync(cf, b"k", &NumberValue::new(1)).unwrap();
        assert_eq!(store.get::<NumberValue>(cf, b"k").unwrap().unwrap().value, 1);
    }
}

#[test]
fn batch_writes_keys_in_two_column_families() {
    let (_dir, store) = open();
    let mut batch = Batch::new();
    batch.put(Cf::Events, b"a".to_vec(), &NumberValue::new(1)).unwrap();
    batch.put(Cf::Lookup, b"b".to_vec(), &NumberValue::new(2)).unwrap();
    store.write_batch(batch, true).unwrap();
    assert!(store.exists(Cf::Events, b"a").unwrap());
    assert!(store.exists(Cf::Lookup, b"b").unwrap());
}

#[test]
fn prefix_scan_returns_only_prefix_keys_up_to_limit() {
    let (_dir, store) = open();
    for i in 0..5 {
        store.put_sync(Cf::Lookup, &key(&["t", "p", &i.to_string()]), &NumberValue::new(i)).unwrap();
    }
    store.put_sync(Cf::Lookup, &key(&["t", "q", "0"]), &NumberValue::new(9)).unwrap();
    let all = store.prefix_scan::<NumberValue>(Cf::Lookup, &prefix(&["t", "p"]), 100).unwrap();
    assert_eq!(all.len(), 5);
    let limited = store.prefix_scan::<NumberValue>(Cf::Lookup, &prefix(&["t", "p"]), 2).unwrap();
    assert_eq!(limited.len(), 2);
}

fn blob_files(root: &std::path::Path) -> usize {
    walk(root).into_iter().filter(|p| p.extension().is_some_and(|e| e == "zst")).count()
}

fn walk(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() { out.extend(walk(&path)) } else { out.push(path) }
    }
    out
}

#[test]
fn putting_same_bytes_twice_gives_one_file() {
    let dir = tempfile::tempdir().unwrap();
    let objects = ObjectStore::new(dir.path().to_path_buf()).unwrap();
    let a = objects.put(b"hello").unwrap();
    let b = objects.put(b"hello").unwrap();
    assert_eq!(a, b);
    assert_eq!(blob_files(dir.path()), 1);
    assert_eq!(objects.get(&a).unwrap(), b"hello");
}

#[test]
fn corrupted_blob_gives_hash_error() {
    let dir = tempfile::tempdir().unwrap();
    let objects = ObjectStore::new(dir.path().to_path_buf()).unwrap();
    let hash = objects.put(b"original content").unwrap();
    let file = walk(dir.path()).into_iter().next().unwrap();
    std::fs::write(&file, zstd::encode_all(&b"changed content!"[..], 3).unwrap()).unwrap();
    assert!(matches!(objects.get(&hash), Err(Error::HashMismatch(_))));
}

#[test]
fn sweep_deletes_only_unreferenced_blobs() {
    let (dir, store) = open();
    let objects = ObjectStore::new(dir.path().join("objects")).unwrap();
    let kept = objects.put(b"referenced").unwrap();
    let orphan = objects.put(b"orphan").unwrap();
    store.put_sync(Cf::Lookup, &layout::blob("t", &kept), &BlobValue { v: 1, size: 10, ref_count: 1 }).unwrap();
    assert_eq!(objects.sweep_orphans(&store, "t", Duration::ZERO).unwrap(), 1);
    assert!(objects.exists(&kept));
    assert!(!objects.exists(&orphan));
}

#[test]
fn watermark_never_decreases() {
    let (_dir, store) = open();
    assert_eq!(raise_watermark(&store, "t", GRAPH, 10).unwrap(), 10);
    assert_eq!(raise_watermark(&store, "t", GRAPH, 4).unwrap(), 10);
    assert_eq!(read_watermark(&store, "t", GRAPH).unwrap(), 10);
}
