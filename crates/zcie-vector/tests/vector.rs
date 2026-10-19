//! TS-16 — Vector search (ADR-011, ADR-013). Model tests need a model folder in `ZCIE_TEST_MODEL`
//! or `~/.zcie/models/<name>` (`zcie vector install`); without it they print a reason and return.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::json;
use zcie_vector::tokenize::{Tokenizer, pre_tokenize, word_pieces};
use zcie_vector::{Entity, FlatIndex, StaticModel, VectorIndex, entity_text, model_folder, split_identifiers};

fn model() -> Option<StaticModel> {
    model_named("potion-base-8M")
}

fn model_named(name: &str) -> Option<StaticModel> {
    let folder = std::env::var(format!("ZCIE_TEST_MODEL_{}", name.replace('-', "_").to_uppercase())).map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".zcie/models").join(name)
    });
    if !folder.join("model.safetensors").exists() {
        println!("skipped: no model in {}", folder.display());
        return None;
    }
    Some(StaticModel::load(&folder).unwrap())
}

#[test]
fn tokenizer_splits_words_punctuation_and_pieces() {
    assert_eq!(pre_tokenize("Ücret, LATE_fee!"), ["ucret", ",", "late", "_", "fee", "!"]);
    let vocab: HashMap<String, u32> = [("play", 1), ("##ing", 2), ("un", 3), ("##play", 4)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    assert_eq!(word_pieces("playing", &vocab), [1, 2]);
    assert_eq!(word_pieces("unplaying", &vocab), [3, 4, 2]);
    assert!(word_pieces("xyz", &vocab).is_empty());
}

#[test]
fn unigram_finds_the_best_pieces_and_drops_unknown_characters() {
    let vocab = json!([["[PAD]", -10.0], ["[UNK]", -10.0], ["▁", -3.0], ["▁ge", -4.0], ["▁gec", -6.0], ["cik", -5.0], ["ikme", -2.0], ["▁gecik", -9.0], ["me", -3.0], [",", -2.0], ["▁,", -2.5]]);
    let tokenizer = Tokenizer::from_json(&json!({"model": {"type": "Unigram", "unk_id": 1, "vocab": vocab}})).unwrap();
    // "▁gecikme": ▁ge + cik + me = -12, ▁gec + ikme = -8, ▁gecik + me = -12: the best is ▁gec + ikme.
    assert_eq!(tokenizer.encode("gecikme"), [4, 6]);
    // Punctuation is a separate word; the unknown "x" is dropped.
    assert_eq!(tokenizer.encode("gecikme,x"), [4, 6, 10, 2]);
    assert_eq!(tokenizer.len(), 11);
    assert!(Tokenizer::from_json(&json!({"model": {"type": "BPE"}})).is_err());
}

#[test]
fn unigram_normalizer_maps_compatibility_characters() {
    assert_eq!(zcie_vector::unigram::normalize("wait…ＡＢ\u{a0}c"), "wait .  .  . AB c");
}

#[test]
fn default_model_falls_back_to_an_installed_model() {
    let home = std::env::temp_dir().join(format!("zcie-models-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    assert!(model_folder(&home, zcie_vector::DEFAULT_MODEL).is_none());
    std::fs::create_dir_all(home.join("potion-base-8M")).unwrap();
    std::fs::write(home.join("potion-base-8M/model.safetensors"), b"x").unwrap();
    assert_eq!(model_folder(&home, zcie_vector::DEFAULT_MODEL), Some(home.join("potion-base-8M")));
    assert!(model_folder(&home, "other").is_none(), "a configured model has no fallback");
    std::fs::create_dir_all(home.join(zcie_vector::DEFAULT_MODEL)).unwrap();
    std::fs::write(home.join(zcie_vector::DEFAULT_MODEL).join("model.safetensors"), b"x").unwrap();
    assert_eq!(model_folder(&home, zcie_vector::DEFAULT_MODEL), Some(home.join(zcie_vector::DEFAULT_MODEL)));
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn multilingual_model_matches_turkish_and_english() {
    let Some(model) = model_named(zcie_vector::DEFAULT_MODEL) else { return };
    let embed = |t: &str| model.embed(&split_identifiers(t)).unwrap();
    let cos = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let docs = ["fn send_password_reset_email(user: &User)", "Kullanıcıya her zaman Türkçe cevap ver.", "fn verify_jwt_token(token: &str) -> Result<Claims>", "The user does not want automatic commits; commit only when asked."];
    let vectors: Vec<Vec<f32>> = docs.iter().map(|d| embed(d)).collect();
    let questions = [("şifre sıfırlama e-postası", 0), ("which language should the replies be in", 1), ("kimlik doğrulama nasıl yapılıyor", 2), ("Kullanıcı otomatik commit istiyor mu?", 3)];
    for (question, gold) in questions {
        let q = embed(question);
        let scores: Vec<f32> = vectors.iter().map(|v| cos(&q, v)).collect();
        let best = scores.iter().enumerate().fold((0, f32::MIN), |b, (i, s)| if *s > b.1 { (i, *s) } else { b }).0;
        assert_eq!(best, gold, "{question}: {scores:?}");
    }
    assert_eq!(model.dim(), 256);
}

#[test]
fn identifiers_are_split_into_words() {
    assert_eq!(split_identifiers("applyLateFee"), "apply Late Fee");
    assert_eq!(split_identifiers("src/billing/late_fee.rs"), "src billing late fee rs");
    let symbol = Entity { kind: "symbol", name: "apply_late_fee", path: "src/billing/late_fee.rs", body: "function pub fn apply_late_fee(x: i64)\n/// Adds the fee.\nbody" };
    let text = entity_text(&symbol).unwrap();
    assert!(text.contains("apply late fee") && text.contains("Adds the fee"), "{text}");
    assert!(entity_text(&Entity { kind: "file", name: "a.rs", path: "a.rs", body: "text" }).is_none());
    let memory = |body| entity_text(&Entity { kind: "memory", name: "n", path: "", body });
    assert!(memory("user_requirement We use PostgreSQL 16; we rejected MySQL.").is_some(), "a stated requirement is a fact");
    assert!(memory("user_requirement Which database do we use? Tell me.").is_none(), "a stored question is not embedded");
    assert!(memory("observation the build was slow").is_none());
    assert!(memory("design_decision Fastify 4 replaces Express.").is_some());
}

#[test]
fn flat_index_upserts_removes_and_ranks() {
    let index = FlatIndex::default();
    index.upsert("w", "a", &[1.0, 0.0]);
    index.upsert("w", "b", &[0.0, 1.0]);
    index.insert_if_absent("w", "a", &[0.0, 1.0]);
    index.upsert("other", "c", &[1.0, 0.0]);
    let hits = index.search("w", &[0.8, 0.6], 5, 0.0);
    assert_eq!(hits.iter().map(|h| h.entity_ref.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert!((hits[0].similarity - 0.8).abs() < 0.02);
    index.remove("a");
    assert_eq!(index.search("w", &[1.0, 0.0], 5, 0.0).iter().map(|h| h.entity_ref.as_str()).collect::<Vec<_>>(), ["b"]);
    assert_eq!(index.len(), 2);
    index.clear();
    assert!(index.is_empty());
}

#[test]
fn model_puts_paraphrases_near_the_code() {
    let Some(model) = model() else { return };
    let embed = |t: &str| model.embed(&split_identifiers(t)).unwrap();
    let cos = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let fee = embed("apply_late_fee balance cents days late");
    let route = embed("resolveRoute path table handler prefix");
    let burn = embed("schedule_orbit_burn satellite delta v queue");
    for (question, best) in [("penalty for an overdue invoice", &fee), ("which handler serves a url", &route), ("plan a thruster firing for a spacecraft", &burn)] {
        let q = embed(question);
        let scores = [cos(&q, &fee), cos(&q, &route), cos(&q, &burn)];
        let top = scores.iter().cloned().fold(f32::MIN, f32::max);
        println!("{question}: {scores:?}");
        assert_eq!(cos(&q, best), top, "{question}: {scores:?}");
    }
    assert_eq!(model.dim(), 256);
    assert!(model.embed("").is_none());
}
