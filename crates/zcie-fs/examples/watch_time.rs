//! Diagnostic: time to start a file watcher on a folder. Usage: `cargo run -p zcie-fs --example watch_time -- <folder>`
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(std::env::args().nth(1).ok_or("usage: watch_time <folder>")?);
    let started = std::time::Instant::now();
    let watcher = zcie_fs::FileWatcher::start(root, std::time::Duration::from_millis(200), zcie_security::ExcludeSet::new(&[])?, Box::new(|_| {}))?;
    println!("started in {} ms", started.elapsed().as_millis());
    drop(watcher);
    Ok(())
}
