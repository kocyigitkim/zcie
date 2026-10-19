//! Unigram (SentencePiece) tokenization as in the Hugging Face `tokenizers` library: the text is
//! normalized, each word gets the "▁" prefix (Metaspace), and Viterbi finds the pieces with the
//! highest sum of log probabilities.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::{Error, Result};

/// The word prefix of the Metaspace pre-tokenizer.
const SPACE: char = '▁';
/// Score below the lowest piece score for one unknown character.
const UNKNOWN_PENALTY: f64 = 10.0;
/// Longest piece (in characters) that the search tries.
const MAX_PIECE_CHARS: usize = 64;
/// Characters that the normalizer surrounds with spaces (bge-m3 / model2vec tokenizers).
const SPLIT_CHARS: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

/// A Unigram model.
pub struct Unigram {
    pieces: HashMap<String, (u32, f64)>,
    count: usize,
    unknown: Option<u32>,
    unknown_score: f64,
    max_chars: usize,
}

impl Unigram {
    /// Reads the `model` object of a Unigram `tokenizer.json`.
    pub fn from_json(model: &Value) -> Result<Unigram> {
        let vocab = model["vocab"].as_array().ok_or_else(|| Error::Format("Unigram vocab missing".into()))?;
        let mut pieces = HashMap::with_capacity(vocab.len());
        let (mut lowest, mut max_chars) = (0.0_f64, 1);
        for (id, entry) in vocab.iter().enumerate() {
            let (Some(piece), Some(score)) = (entry[0].as_str(), entry[1].as_f64()) else { return Err(Error::Format("bad Unigram entry".into())) };
            let id = u32::try_from(id).map_err(|_| Error::Format("too many pieces".into()))?;
            lowest = lowest.min(score);
            max_chars = max_chars.max(piece.chars().count().min(MAX_PIECE_CHARS));
            pieces.insert(piece.to_string(), (id, score));
        }
        let unknown = model["unk_id"].as_u64().and_then(|v| u32::try_from(v).ok());
        Ok(Unigram { pieces, count: vocab.len(), unknown, unknown_score: lowest - UNKNOWN_PENALTY, max_chars })
    }

    /// Returns the number of pieces.
    pub fn len(&self) -> usize {
        self.count
    }

    /// True if the model has no pieces.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Returns the piece IDs of a text; unknown characters are dropped.
    pub fn encode(&self, text: &str) -> Vec<u32> {
        normalize(text).split(' ').filter(|w| !w.is_empty()).flat_map(|w| self.word(&format!("{SPACE}{w}"))).filter(|id| Some(*id) != self.unknown).collect()
    }

    /// Viterbi over one word: `best[i]` is the best score of the first `i` characters.
    fn word(&self, word: &str) -> Vec<u32> {
        let bounds: Vec<usize> = word.char_indices().map(|(i, _)| i).chain(std::iter::once(word.len())).collect();
        let n = bounds.len() - 1;
        let mut best: Vec<(f64, usize, Option<u32>)> = vec![(f64::NEG_INFINITY, 0, None); n + 1];
        best[0].0 = 0.0;
        for start in 0..n {
            let base = best[start].0;
            if base == f64::NEG_INFINITY {
                continue;
            }
            for end in start + 1..=n.min(start + self.max_chars) {
                let Some((id, score)) = self.pieces.get(&word[bounds[start]..bounds[end]]) else { continue };
                if base + score > best[end].0 {
                    best[end] = (base + score, start, Some(*id));
                }
            }
            if base + self.unknown_score > best[start + 1].0 && best[start + 1].2.is_none() {
                best[start + 1] = (base + self.unknown_score, start, self.unknown);
            }
        }
        let mut ids = Vec::new();
        let mut at = n;
        while at > 0 {
            let (_, prev, id) = best[at];
            ids.extend(id);
            at = prev;
        }
        ids.reverse();
        ids
    }
}

/// Returns the NFKC form of the common compatibility characters (the precompiled map of the
/// tokenizer does this): "…" is "...", full-width ASCII is ASCII, special spaces are spaces.
fn compatibility(c: char) -> Option<&'static str> {
    match c {
        '…' => Some("..."),
        SPACE => Some(" "),
        '\u{a0}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}' => Some(" "),
        'ﬁ' => Some("fi"),
        'ﬂ' => Some("fl"),
        '™' => Some("TM"),
        _ => None,
    }
}

/// Maps a full-width ASCII character (U+FF01..U+FF5E) to ASCII.
fn narrow(c: char) -> char {
    match u32::from(c) {
        code @ 0xFF01..=0xFF5E => char::from_u32(code - 0xFEE0).unwrap_or(c),
        _ => c,
    }
}

/// The normalizer of the model2vec Unigram tokenizers: compatibility characters are mapped,
/// punctuation becomes a separate word and white space is collapsed.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut expanded = String::with_capacity(text.len());
    for c in text.chars().map(narrow) {
        match compatibility(c) {
            Some(text) => expanded.push_str(text),
            None => expanded.push(c),
        }
    }
    for c in expanded.chars() {
        if SPLIT_CHARS.contains(c) {
            out.push(' ');
            out.push(c);
            out.push(' ');
        } else if c.is_whitespace() {
            out.push(' ');
        } else if !c.is_control() {
            out.push(c);
        }
    }
    out
}
