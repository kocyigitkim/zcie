//! Keyword matching for the classifier and the intent selector. Text and keywords are folded
//! first (Turkish letters to ASCII, lowercase), so "çağırıyor", "cagiriyor" and "ÇAĞIRIYOR" match
//! the same keyword. A match starts at a word start.

/// Folds Turkish letters to ASCII and lowercases.
pub fn fold(text: &str) -> String {
    text.chars().map(zcie_search::fold_char).collect()
}

/// True if `keyword` occurs at a word start. Letters may follow it, so a stem matches its
/// suffixes ("çağır" matches "çağırıyor"). A phrase matches from its first word.
pub fn matches(text: &str, keyword: &str) -> bool {
    find(text, keyword, false)
}

/// True if `word` occurs as a whole word: no letter may follow it ("karar" does not match "kararlı").
pub fn exact_word(text: &str, word: &str) -> bool {
    find(text, word, true)
}

/// True if a word of `text` is `word` or one typo away from it ("wher", "refrences"). Only for
/// words of 5 or more letters, since short words are often other words.
pub fn near_word(text: &str, word: &str) -> bool {
    let key: Vec<char> = fold(word).chars().collect();
    key.len() >= 5 && fold(text).split(|c: char| !c.is_alphanumeric()).any(|t| within_one(&t.chars().collect::<Vec<_>>(), &key))
}

/// True if `word` is a whole token of `text` (tokens split at every non-letter, so "webhook'tan" has "tan").
pub fn has_token(text: &str, word: &str) -> bool {
    let key = fold(word);
    fold(text).split(|c: char| !c.is_alphanumeric()).any(|t| t == key)
}

/// True if a token is a code name ("apply_late_fee", "UserService", "PaymentLedger.flush"), not an acronym.
pub fn has_code_name(text: &str) -> bool {
    // "UserService", "PostgreSQL", "Bm25Weight" (digit, capital, lowercase); not "TTL'i" or an id's "1M5".
    let camel = |t: &str| {
        let c: Vec<char> = t.chars().collect();
        c.windows(2).any(|w| w[0].is_lowercase() && w[1].is_uppercase()) || c.windows(3).any(|w| w[0].is_ascii_digit() && w[1].is_uppercase() && w[2].is_lowercase())
    };
    // An id ("proj_01M53625R2YGNASE8V8MCBMR6T": 16 or more characters after the underscore) is not a name.
    let underscore = |t: &str| t.contains('_') && t.rsplit('_').next().is_some_and(|s| s.len() < 16);
    text.split_whitespace().any(|t| underscore(t) || t.contains("::") || camel(t))
}

/// True if the text has the word "where" or a typo of it. "there" is not a typo of "where".
pub fn where_word(text: &str) -> bool {
    let key: Vec<char> = "where".chars().collect();
    fold(text).split(|c: char| !c.is_alphanumeric()).any(|t| t == "where" || (t.chars().count() >= 4 && !["there", "were", "here"].contains(&t) && within_one(&t.chars().collect::<Vec<_>>(), &key)))
}

/// Number of keywords that match (see `matches`).
pub fn hits(text: &str, keywords: &[&str]) -> u32 {
    keywords.iter().filter(|k| matches(text, k)).count() as u32
}

/// Number of words that occur as whole words (see `exact_word`).
pub fn exact_hits(text: &str, words: &[&str]) -> u32 {
    words.iter().filter(|w| exact_word(text, w)).count() as u32
}

/// Number of words that are typo-tolerant matches (see `near_word`).
pub fn near_hits(text: &str, words: &[&str]) -> u32 {
    words.iter().filter(|w| near_word(text, w)).count() as u32
}

fn find(text: &str, keyword: &str, whole: bool) -> bool {
    let (text, key) = (fold(text), fold(keyword));
    text.match_indices(key.as_str()).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(char::is_alphanumeric)
            && (!whole || !text[at + key.len()..].chars().next().is_some_and(char::is_alphanumeric))
    })
}

/// True if `a` equals `b` or becomes `b` by one substitution, insertion, deletion or swap of two
/// neighbours.
fn within_one(a: &[char], b: &[char]) -> bool {
    let (la, lb) = (a.len(), b.len());
    if la.abs_diff(lb) > 1 {
        return false;
    }
    if la == lb {
        let diff: Vec<usize> = (0..la).filter(|&i| a[i] != b[i]).collect();
        return match diff.as_slice() {
            [] | [_] => true,
            [i, j] => *j == i + 1 && a[*i] == b[*j] && a[*j] == b[*i],
            _ => false,
        };
    }
    // Walk the shorter word against the longer one, allowing one skipped letter of the longer.
    let (short, long) = if la < lb { (a, b) } else { (b, a) };
    let (mut i, mut j, mut skipped) = (0, 0, false);
    while i < short.len() && j < long.len() {
        if short[i] == long[j] {
            i += 1;
        } else if skipped {
            return false;
        } else {
            skipped = true;
        }
        j += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_letters_and_suffixes_match_folded() {
        assert!(matches("Kim ÇAĞIRIYOR?", "kim cagir"));
        assert!(matches("parse_config nereden cagriliyor", "çağrıl"));
        assert!(matches("UserService'i çağıran yerler", "cagiran"));
        assert!(!matches("xcağır", "cagir"));
    }

    #[test]
    fn exact_word_rejects_suffixes() {
        assert!(exact_word("bu kararı kim verdi", "karari"));
        assert!(!exact_word("kararlı sürüm", "karar"));
    }

    #[test]
    fn near_word_accepts_one_typo_in_long_words_only() {
        assert!(near_word("wher is ShipmentRouter defined", "where"));
        assert!(near_word("find all refrences to render_tile", "references"));
        assert!(near_word("the defiend type", "defined"));
        assert!(!near_word("what is the cat", "where"));
        assert!(!near_word("it is a cat", "where"));
    }
}
