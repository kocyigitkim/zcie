//! Turkish and English words of one meaning (ADR-013). Users mix the two languages ("toolların
//! hepsi", "araç tanımları"), so a question term also matches its equivalent in the other language.
//! The words are stems: letter-folded and cut to `zcie_search::STEM_CHARS` characters.

/// Groups of stems with one meaning. Words that are prefixes of unrelated words ("inter", "veri")
/// are not in the list.
const GROUPS: &[&[&str]] = &[
    &["arac", "tool"],
    &["maliy", "cost"],
    &["bagla", "conte"],
    &["hafiz", "belle", "memor"],
    &["karar", "decis"],
    &["sorgu", "query"],
    &["indek", "index"],
    &["hata", "error", "bug"],
    &["ozell", "featu"],
    &["ayar", "setti", "confi"],
    &["tanim", "defin"],
    &["otoma", "autom"],
    &["oturu", "sessi"],
    &["verit", "datab"],
    &["onbel", "cache"],
    &["arama", "searc"],
    &["kural", "rule"],
    &["terci", "prefe"],
    &["sinir", "limit"],
    &["guven", "secur"],
    &["yetki", "permi"],
    &["sohbe", "chat", "conve"],
    &["gecik", "late", "overd"],
    &["faiz", "fee"],
    &["odeme", "payme"],
    &["fatur", "invoi"],
    &["tutar", "amoun"],
    &["kurus", "cent"],
    &["kuyru", "queue"],
    &["yedek", "backu"],
    &["surum", "versi"],
    &["dagit", "deplo"],
    &["belge", "dokum", "docum"],
    &["yorum", "comme"],
    &["gorev", "task"],
    &["redde", "rejec"],
    &["dogru", "accur", "corre"],
];

/// Shortest common prefix that matches two stems of different length (as in `relevance`).
const MIN_PREFIX_MATCH: usize = 4;

/// True if two stems are equal, or one is a prefix of the other with at least `MIN_PREFIX_MATCH`
/// characters ("onay" and "onayi").
pub fn same_stem(a: &str, b: &str) -> bool {
    a == b || a.len().min(b.len()) >= MIN_PREFIX_MATCH && (a.starts_with(b) || b.starts_with(a))
}

/// Returns the stems of the other words of the groups of `stem`.
pub fn equivalents(stem: &str) -> impl Iterator<Item = &'static str> + '_ {
    GROUPS.iter().filter(move |group| group.iter().any(|w| same_stem(w, stem))).flat_map(|group| group.iter().copied()).filter(move |w| !same_stem(w, stem))
}
