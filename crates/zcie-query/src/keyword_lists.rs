//! Keyword lists of the classifier and the intent selector. Lists are lowercase and ASCII-folded
//! (see `keywords`). Phrases and stems match with `matches`, whole words with `exact_word`, and
//! typo-tolerant words with `near_word`.

/// Phrases of the `SymbolDefinition` class.
pub const DEFINITION_PHRASES: &[&str] = &["definition of", "declaration of"];
/// Stems of the `SymbolDefinition` class ("tanımı", "tanımlı", "tanımlandı").
pub const DEFINITION_STEMS: &[&str] = &["tanim"];
/// Words that make a "where ..." question a definition question ("where X is defined").
pub const DEFINITION_WHERE: &[&str] = &["defined", "declared"];
/// Verbs of a definition question ("which file defines X").
pub const DEFINITION_VERBS: &[&str] = &["defines", "declares"];
/// Words that ask for a declaration with a code name ("show me the X struct", "the signature of X").
pub const DEFINITION_SHAPE: &[&str] = &["struct", "class", "function", "fn", "enum", "trait", "method", "signature", "imzasi", "imzasini"];
/// Words that ask to show a declaration ("show me the X struct", "what is the signature of X").
pub const DEFINITION_ASK: &[&str] = &["show", "signature", "what", "imzasi", "imzasini"];

/// Phrases of the `Callers` class.
pub const CALLER_PHRASES: &[&str] = &[
    "who calls", "who call", "who uses", "who use", "callers of", "kim call", "kim kullan", "kim cagir", "call site",
    "nerede kullan", "nerelerde kullan", "nereden cagril", "called anywhere", "called by", "is called", "referans eden",
    "referans veren", "kullanan yer",
];
/// Stems of the `Callers` class ("çağır", "çağıran", "reference").
pub const CALLER_STEMS: &[&str] = &["cagir", "referenc", "caller"];
/// Whole words of the `Callers` class.
pub const CALLER_WORDS: &[&str] = &["calls", "calling"];
/// Typo-tolerant words of the `Callers` class ("refrences").
pub const CALLER_NEAR: &[&str] = &["references", "referenced", "callers"];
/// Words that make a "where ..." question a callers question ("where is X used").
pub const CALLER_WHERE: &[&str] = &["used", "referenced", "called", "calls"];
/// Verb stems of a use or a call; they count with a cue word ("kim", "nerede", "hangi").
pub const CALLER_VERB_STEMS: &[&str] = &["cagril", "tetikl", "kullanil", "kullanan", "referans", "invok", "trigger"];
/// Whole verb words of a use; they count with a cue word, or with a place word.
pub const CALLER_VERB_WORDS: &[&str] = &["used", "uses"];
/// Words of a use, for a question about places of use ("which modules use X").
pub const CALLER_USE_WORDS: &[&str] = &["use", "uses", "used"];
/// Words for places of use ("which modules use X").
pub const CALLER_PLACES: &[&str] = &[
    "modules", "module", "files", "file", "places", "place", "locations", "yerler", "yerleri", "yerlerde", "dosyalar",
    "moduller", "modulleri",
];
/// Cue words of a who, where or which question about a use.
pub const CALLER_CUES: &[&str] = &[
    "kim", "nerede", "nereden", "nerelerde", "nerelerden", "nereye", "hangi", "where", "who", "anywhere", "which",
    "besides", "yer", "yerler", "yerleri", "yerlerde",
];

/// Phrases of the `Timeline` class (the past conversation).
pub const TIMELINE_PHRASES: &[&str] = &[
    "what did i ask", "what did we", "what changed since", "history of", "timeline", "daha once", "ne yapmistik",
    "ne yaptik", "did we", "have we", "we decided", "we chose", "we picked", "we rejected", "we agreed", "any rule",
    "did the user", "the user want", "the user asked", "the user said", "the user set", "what rule", "was decided",
    "karar ver", "remind me", "i tell you", "i told you", "i gave you", "last chat", "our chat", "our conversation",
    "previous conversation", "we said", "we talked", "we approved", "we dropped", "remember what", "who came up with",
    "the rule about", "planning call", "planning meeting", "planning chat", "did you say", "you said", "you suggested",
    "you recommended", "earlier in this chat", "earlier in this conversation", "what did i say", "last week i said", "i said",
];
/// Stems of the `Timeline` class.
pub const TIMELINE_STEMS: &[&str] = &[
    "decision", "kural", "hatirla", "vazgec", "anlast", "anlasmis", "konustuk", "konusmus", "sectik", "secmis", "demis",
    "belirled", "soylemis",
];
/// Whole words of the `Timeline` class ("karar" must not match "kararlı").
pub const TIMELINE_WORDS: &[&str] = &["karar", "karari", "kararlar", "kararlari", "kararini", "kararin"];
/// Words that, with "kullanıcı" (the user) in the question, ask about the ideas of the user.
pub const USER_INTENT: &[&str] = &[
    "istedi", "istiyor", "istemis", "dedi", "soyledi", "bekliyor", "beklenti", "karar", "tercih", "onayl", "gore",
    "spesifikasyon", "isteg", "fikr", "gorus", "dusun",
];
/// Words that, with "ekip" or "takım" (the team), ask what they said.
pub const TEAM_SAID: &[&str] = &["demis", "dedi", "soyled"];
/// First-person past suffixes of a verb: "onayladık", "bıraktık", "konuştuk" (we did).
pub const PAST_WE_SUFFIXES: &[&str] = &["tik", "tuk", "dik", "duk"];
/// Words that, with a past-we verb, ask about the past conversation ("hangi", "neden", "geçen").
pub const PAST_CUES: &[&str] = &["hangi", "ne", "neden", "why", "gecen", "hatirla"];
/// Decision verbs: "why did the project replace X" is a question about a decision.
pub const DECISION_WORDS: &[&str] = &["replace", "replaced", "chose", "picked", "dropped", "switched", "adopted", "rejected"];

/// Phrases of the `Implementation` class.
pub const IMPLEMENTATION_PHRASES: &[&str] = &["which commit", "hangi commit", "hangi degisiklik", "which change", "what change"];
/// Whole words that, after "when", ask when something landed ("when did X land", "when was X added").
pub const WHEN_IMPL_WORDS: &[&str] = &["added", "introduced", "landed", "land", "shipped", "merged"];
/// Verbs that start an instruction to create code (they count as an edit only with a code name or code noun).
pub const CREATE_VERBS: &[&str] = &["write", "create", "generate", "port"];
/// Nouns of code, with a creation verb: "write a unit test", "create a function".
pub const CODE_NOUNS: &[&str] = &["function", "fn", "class", "struct", "test", "tests", "file", "module", "method", "endpoint", "route", "handler", "query", "code", "fonksiyon", "sinif", "dosya", "modul", "metot"];
/// Verb stems that, with a commit, PR, merge or change, ask which one did something.
pub const IMPLEMENTATION_VERBS: &[&str] = &["implement", "introduc", "added", "ekledi", "eklendi", "yazdi", "getird", "geldi", "brought", "introduced"];
/// Whole-word nouns of a change (with a verb from `IMPLEMENTATION_VERBS`): "PR", "merge".
pub const IMPLEMENTATION_WORD_NOUNS: &[&str] = &["pr", "merge"];
/// Phrase nouns of a change: "pull request", "değişiklik".
pub const IMPLEMENTATION_PHRASE_NOUNS: &[&str] = &["pull request", "degisiklik"];

/// English verbs that start an instruction to change code.
pub const EDIT_VERBS: &[&str] = &[
    "add", "rename", "change", "remove", "delete", "replace", "implement", "fix", "apply", "refactor", "update", "move",
    "insert", "extract", "make", "set", "simplify", "split", "drop", "clean", "rewrite", "convert", "merge", "inline",
    "optimize", "wire", "hook", "bump", "revert", "write", "create", "generate", "port",
];
/// Words before the verb of a polite request ("can you add", "go ahead and delete").
pub const EDIT_PREFIXES: &[&str] = &["please", "pls", "can", "could", "would", "you", "go", "ahead", "and"];
/// Turkish verb stems of an instruction to change code, as the last word of the sentence.
pub const EDIT_STEMS_TR: &[&str] = &["ekle", "degistir", "duzelt", "sil", "kaldir", "uygula", "guncelle", "tasi", "adlandir"];
/// Turkish edit verbs that count only as the whole last word ("yazılıyor" is not an edit).
pub const EDIT_WORDS_TR: &[&str] = &["yaz", "yazin", "olustur", "olusturun"];
/// Question particles after a verb ("düzeltebilir misin"): the verb is the word before them.
pub const QUESTION_PARTICLES: &[&str] = &["mi", "mu", "misin", "musun", "miyim", "misiniz", "musunuz"];

/// Phrases of an explanation question.
pub const EXPLAIN_PHRASES: &[&str] = &[
    "explain", "how does", "how do ", "how is ", "what does", "what is the purpose", "neden calis", "nasil calis",
    "acikla", "ne ise yariyor",
];
/// Nouns and names of code (stems): "why" is a code question only with one of them, or a code name.
pub const EXPLAIN_CODE_NOUNS: &[&str] = &[
    "fonksiyon", "function", "kod", "code", "sinif", "class", "dosya", "modul", "module", "test", "build", "parser", "router",
    "service", "servis", "cache", "worker", "queue", "kuyruk", "thread", "lock", "schema", "loader", "index", "migration",
    "tokenizer", "config", "ayar", "deploy", "webhook", "middleware", "handler", "scheduler", "database", "api", "endpoint",
    "pipeline", "logic", "struct", "trait", "method", "crate", "query", "tablo", "table", "retry", "dongu", "loop", "log",
    "checkout", "server", "sunucu", "filter",
];
/// Verbs of a call or a use of a named code thing: "how is X called", "why does everything use X".
pub const CALL_WORDS: &[&str] = &["called", "call", "calls", "uses", "use", "used"];
/// Stems of a call or a use in Turkish (çağrılıyor, kullanıyor).
pub const CALL_STEMS: &[&str] = &["cagriliyor", "cagrildi", "cagrilir"];
/// Turkish "kullanıyor" (we use) is a call question only with a why or how cue ("neden … kullanıyor").
pub const USE_WHY_CUES: &[&str] = &["neden", "why", "nasil", "how"];
/// Phrases of a call chain or a call path: a flow, not a list of callers.
pub const CALL_FLOW_PHRASES: &[&str] = &["call chain", "call path"];
/// Words that, with "nasıl" (how), ask for an explanation ("nasıl … ediyor, kısaca anlat").
pub const EXPLAIN_HOW_WORDS: &[&str] = &["anlat", "acikla", "kisaca"];
/// Phrases of a question about earlier decisions or history.
pub const MEMORY_PHRASES: &[&str] = &["did we ", "we decided", "we agreed", "daha once", "karar ver", "neden vazgec", "was decided"];

/// Phrases of a flow question.
pub const TRACE_PHRASES: &[&str] = &[
    "trace", "call chain", "call path", "follow the flow", "nasil ilerli", "adim adim", "nereye kadar", "step by step",
    "izi sur", "izini sur", "sirasiyla", "sirayla",
];
/// Turkish stems of a flow question ("akışı", "akışını", "çağrı zinciri").
pub const TRACE_STEMS_TR: &[&str] = &["akis", "zincir"];
/// Words of a flow question. "path" alone is not one ("Give the file path").
pub const TRACE_WORDS: &[&str] = &["flow", "sequence", "steps", "yol", "yolu"];
/// With "from X to Y", one of these makes a flow question.
pub const TRACE_FROM_TO_MARKERS: &[&str] = &["::", "flow", "path", "sequence", "step", "goes"];
/// Words that make "from X to Y" a flow ("how does a request go from A to B").
pub const TRACE_FROM_TO_TOKENS: &[&str] = &["go", "goes", "flow", "flows", "travel", "travels", "path", "sequence", "steps"];
/// Ablative suffixes as separate tokens ("webhook'tan", "queue'den") with "kadar" (until): a flow to a place.
pub const ABLATIVE_TOKENS: &[&str] = &["dan", "den", "tan", "ten"];

/// Phrases that ask where code is.
pub const LOCATE_PHRASES: &[&str] = &["hangi dosya", "where is the"];
/// Words that ask where code is.
pub const LOCATE_WORDS: &[&str] = &["nerede", "nereyi"];
/// Nouns of a code place (with a verb from `LOCATE_VERBS`): "which file/module/crate ...".
pub const LOCATE_NOUNS: &[&str] = &["file", "dosya", "modul", "crate", "layer", "katman", "klasor", "folder", "package", "paket"];
/// Verbs of a code place question: "which file do I change", "which crate is X implemented".
pub const LOCATE_VERBS: &[&str] = &["change", "degistirme", "comput", "implement", "duruyor", "yapiliyor", "yerini", "hesapl", "karar veriyor", "handl", "decid"];
/// Whole words of a code place question: "which file has X", "which file contains X".
pub const LOCATE_HAS_WORDS: &[&str] = &["has", "contains"];
/// Nouns of a where-is-the-logic, code or handler question.
pub const WHERE_LOCATE_NOUNS: &[&str] = &["logic", "code", "handler"];
/// Phrases of a status question (ZC-19 nerede kaldı, where are we on X): not a location.
pub const STATUS_PHRASES: &[&str] = &["nerede kald", "where are we", "ne durumda"];
/// Words of a "where would I put / where does the app read" question.
pub const WHERE_LOCATE_WORDS: &[&str] = &["put", "decide", "decides", "belong", "belongs", "read", "reads", "load", "loads", "handle", "handles", "repo", "codebase"];
