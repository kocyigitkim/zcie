//! Prints the token IDs of each line of a file (to compare with the reference tokenizer).
//! Usage: `cargo run -p zcie-vector --example encode -- <tokenizer.json> <lines.txt>`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(tokenizer), Some(lines)) = (args.get(1), args.get(2)) else { return Err("usage: encode <tokenizer.json> <lines.txt>".into()) };
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(tokenizer)?)?;
    let tokenizer = zcie_vector::tokenize::Tokenizer::from_json(&json)?;
    for line in std::fs::read_to_string(lines)?.lines() {
        println!("{}", tokenizer.encode(line).iter().map(u32::to_string).collect::<Vec<_>>().join(" "));
    }
    Ok(())
}
