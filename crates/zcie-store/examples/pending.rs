//! Diagnostic: counts pending jobs by workspace, kind and target folder, read-only (the daemon
//! can run). Usage: `cargo run -p zcie-store --example pending -- <instance>/rocksdb`

use std::collections::BTreeMap;

use rocksdb::{DB, IteratorMode, Options};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: pending <rocksdb folder>")?;
    let names = DB::list_cf(&Options::default(), &path)?;
    let db = DB::open_cf_for_read_only(&Options::default(), &path, &names, false)?;
    let jobs = db.cf_handle("jobs").ok_or("no jobs column family")?;
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_folder: BTreeMap<String, usize> = BTreeMap::new();
    let mut targets: BTreeMap<String, usize> = BTreeMap::new();
    for item in db.iterator_cf(jobs, IteratorMode::Start) {
        let (key, value) = item?;
        if !String::from_utf8_lossy(&key).contains("pending") {
            continue;
        }
        let job: serde_json::Value = serde_json::from_slice(&value)?;
        let ws = job["scope"]["workspace_id"].as_str().unwrap_or_default().to_string();
        *by_kind.entry(format!("{ws} {}", job["kind"])).or_default() += 1;
        let target = job["target"].as_str().unwrap_or_default().to_string();
        let folder: String = target.split('/').take(4).collect::<Vec<_>>().join("/");
        *by_folder.entry(folder).or_default() += 1;
        *targets.entry(target).or_default() += 1;
    }
    let top = |map: &BTreeMap<String, usize>, n: usize| {
        let mut items: Vec<_> = map.iter().collect();
        items.sort_by(|a, b| b.1.cmp(a.1));
        items.into_iter().take(n).map(|(k, v)| format!("{v:>7}  {k}")).collect::<Vec<_>>().join("\n")
    };
    println!("== by workspace and kind\n{}\n== by target folder\n{}\n== most repeated targets\n{}", top(&by_kind, 15), top(&by_folder, 15), top(&targets, 10));
    println!("total targets {} jobs {}", targets.len(), targets.values().sum::<usize>());
    Ok(())
}
