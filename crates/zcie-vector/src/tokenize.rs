//! BERT uncased tokenization: lowercase, accents removed, split at white space and punctuation,
//! then WordPiece (longest known prefix, `##` for the rest of a word).

use std::collections::HashMap;

/// Longest word that WordPiece splits; a longer word is unknown.
const MAX_WORD_CHARS: usize = 100;

/// Returns the accent-free lowercase form of a letter (common Latin letters, including Turkish).
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ı' => 'i',
        'ğ' => 'g',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ş' => 's',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

/// Splits a text into words and single punctuation characters.
pub fn pre_tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in text.chars().flat_map(char::to_lowercase).map(fold) {
        if c.is_alphanumeric() {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
        if !c.is_whitespace() && !c.is_control() {
            out.push(c.to_string());
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// Splits one word into WordPiece token IDs. Returns an empty list for an unknown word.
pub fn word_pieces(word: &str, vocab: &HashMap<String, u32>) -> Vec<u32> {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() > MAX_WORD_CHARS {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = chars.len();
        let mut found = None;
        while end > start {
            let piece: String = chars[start..end].iter().collect();
            let key = if start == 0 { piece } else { format!("##{piece}") };
            if let Some(id) = vocab.get(&key) {
                found = Some(*id);
                break;
            }
            end -= 1;
        }
        let Some(id) = found else { return Vec::new() };
        out.push(id);
        start = end;
    }
    out
}

/// A tokenizer of `tokenizer.json`: WordPiece (BERT models) or Unigram (SentencePiece models).
pub enum Tokenizer {
    /// WordPiece with a lowercase, accent-free pre-tokenizer.
    WordPiece(HashMap<String, u32>),
    /// Unigram with a Metaspace pre-tokenizer.
    Unigram(crate::unigram::Unigram),
}

impl Tokenizer {
    /// Reads the `model` part of a parsed `tokenizer.json`.
    pub fn from_json(json: &serde_json::Value) -> crate::Result<Tokenizer> {
        let model = &json["model"];
        match model["type"].as_str() {
            Some("WordPiece") => {
                let vocab = model["vocab"].as_object().ok_or_else(|| crate::Error::Format("WordPiece vocab missing".into()))?;
                let vocab = vocab.iter().filter_map(|(k, v)| Some((k.clone(), u32::try_from(v.as_u64()?).ok()?))).collect();
                Ok(Tokenizer::WordPiece(vocab))
            }
            Some("Unigram") => Ok(Tokenizer::Unigram(crate::unigram::Unigram::from_json(model)?)),
            other => Err(crate::Error::Format(format!("tokenizer model {other:?} is not supported"))),
        }
    }

    /// Returns the number of tokens (the largest ID plus one).
    pub fn len(&self) -> usize {
        match self {
            Tokenizer::WordPiece(vocab) => vocab.values().max().map_or(0, |m| *m as usize + 1),
            Tokenizer::Unigram(unigram) => unigram.len(),
        }
    }

    /// True if the tokenizer has no tokens.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the token IDs of a text. Unknown words and characters are dropped (as in model2vec).
    pub fn encode(&self, text: &str) -> Vec<u32> {
        match self {
            Tokenizer::WordPiece(vocab) => pre_tokenize(text).iter().flat_map(|w| word_pieces(w, vocab)).collect(),
            Tokenizer::Unigram(unigram) => unigram.encode(text),
        }
    }
}
