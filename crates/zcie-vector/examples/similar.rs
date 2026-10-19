//! Prints the cosine similarity of text pairs (`a<TAB>b` lines) with a model folder.
//! Usage: `cargo run -p zcie-vector --example similar -- <model folder> <pairs.tsv>`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(folder), Some(pairs)) = (args.get(1), args.get(2)) else { return Err("usage: similar <model folder> <pairs.tsv>".into()) };
    let started = std::time::Instant::now();
    let model = zcie_vector::StaticModel::load(std::path::Path::new(folder))?;
    eprintln!("loaded in {} ms", started.elapsed().as_millis());
    for line in std::fs::read_to_string(pairs)?.lines() {
        let Some((a, b)) = line.split_once('\t') else { continue };
        let embed = |t: &str| model.embed(&zcie_vector::split_identifiers(t)).unwrap_or_default();
        let similarity: f32 = embed(a).iter().zip(embed(b)).map(|(x, y)| x * y).sum();
        println!("{similarity:.3}\t{a}\t{b}");
    }
    Ok(())
}
