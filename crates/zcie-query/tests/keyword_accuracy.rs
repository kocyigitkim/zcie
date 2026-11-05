//! Accuracy of the rule-based classifier and intent selector on labeled questions (K0).
//!
//! Run with `cargo test -p zcie-query --test keyword_accuracy -- --nocapture` to see the numbers.
//! `ZCIE_KEYWORD_MIN` (a fraction from 0 to 1, default 0) is the minimum holdout joint accuracy.

use std::path::Path;

use zcie_query::{classify::classify, intent::select_intent};
use zcie_types::{QueryClass, QueryIntent, QueryMode};

#[derive(Default)]
struct Score {
    total: usize,
    class_ok: usize,
    intent_ok: usize,
    joint_ok: usize,
}

fn pct(part: usize, total: usize) -> f64 {
    if total == 0 { 0.0 } else { 100.0 * part as f64 / total as f64 }
}

#[test]
fn keyword_accuracy() {
    let min: f64 = std::env::var("ZCIE_KEYWORD_MIN").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let no_symbols = |_: &str| false;
    let mut scores: Vec<(&str, Score)> = vec![("dev", Score::default()), ("holdout", Score::default()), ("unseen", Score::default()), ("unseen2", Score::default()), ("unseen3", Score::default()), ("unseen4", Score::default())];
    let mut mismatches = Vec::new();

    for file in ["keyword_cases.json", "keyword_cases_unseen.json", "keyword_cases_unseen2.json", "keyword_cases_unseen3.json", "keyword_cases_unseen4.json"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(file);
        // unseen2 and unseen3 are optional: skip them when the file is absent.
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) if file.ends_with("unseen2.json") || file.ends_with("unseen3.json") || file.ends_with("unseen4.json") => continue,
            Err(_) => panic!("read {file}"),
        };
        let Ok(rows) = serde_json::from_str::<Vec<serde_json::Value>>(&text) else { panic!("parse {file}") };
        // The first element of the dev file is the rules comment, not a case.
        for row in rows.iter().filter(|r| r.get("q").is_some()) {
            let Some(q) = row["q"].as_str() else { panic!("q is a string") };
            let Ok(want_class) = serde_json::from_value::<QueryClass>(row["class"].clone()) else { panic!("known class") };
            let Ok(want_intent) = serde_json::from_value::<QueryIntent>(row["intent"].clone()) else { panic!("known intent") };
            let got_class = classify(q, QueryMode::Auto, &no_symbols);
            let got_intent = select_intent(q, QueryMode::Auto, got_class);

            let set = row["set"].as_str().unwrap_or("dev");
            let Some((_, score)) = scores.iter_mut().find(|(name, _)| *name == set) else { panic!("unknown set {set}") };
            score.total += 1;
            let class_ok = got_class == want_class;
            let intent_ok = got_intent == want_intent;
            score.class_ok += usize::from(class_ok);
            score.intent_ok += usize::from(intent_ok);
            score.joint_ok += usize::from(class_ok && intent_ok);

            if !(class_ok && intent_ok) {
                mismatches.push(format!(
                    "[{set}] {q} | expected {want_class:?}/{want_intent:?} | got {got_class:?}/{got_intent:?}"
                ));
            }
        }
    }

    for (name, s) in &scores {
        println!(
            "{name:8} n={:3}  class {:5.1}%  intent {:5.1}%  joint {:5.1}%",
            s.total,
            pct(s.class_ok, s.total),
            pct(s.intent_ok, s.total),
            pct(s.joint_ok, s.total),
        );
    }
    println!("mismatches ({}):", mismatches.len());
    for line in &mismatches {
        println!("{line}");
    }

    let holdout = &scores[1].1;
    let holdout_joint = holdout.joint_ok as f64 / holdout.total.max(1) as f64;
    assert!(
        holdout_joint >= min,
        "holdout joint accuracy {holdout_joint:.3} is below ZCIE_KEYWORD_MIN {min:.3}"
    );
}
