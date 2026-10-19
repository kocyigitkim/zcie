//! Search queries (`docs/09-query-and-context.md` section 4.3).

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{TantivyDocument, Term};

use crate::error::Result;
use crate::lexical::schema::{IndexName, STEM_CHARS, expand_identifiers, stem_text};
use crate::lexical::writer::SearchIndex;

/// Boost of the `name` field.
const NAME_BOOST: f32 = 3.0;
/// Boost of the `path` field.
const PATH_BOOST: f32 = 2.0;
/// Boost of the stem field (lower than exact words).
const STEM_BOOST: f32 = 0.5;

/// A search request.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// Tenant ID filter.
    pub tenant_id: String,
    /// Workspace ID filter.
    pub workspace_id: String,
    /// Optional repository filter.
    pub repository_id: Option<String>,
    /// Question text.
    pub text: String,
    /// Indexes in search order.
    pub indexes: Vec<IndexName>,
    /// Maximum hits per index.
    pub limit: usize,
}

/// One search hit.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// Entity ref.
    pub entity_ref: String,
    /// Entity kind.
    pub kind: String,
    /// File path.
    pub path: String,
    /// Name or title.
    pub name: String,
    /// Tantivy score.
    pub score: f32,
    /// Sequence of the indexed state.
    pub seq: u64,
    /// The index of the hit.
    pub index: IndexName,
}

impl SearchIndex {
    /// Searches the given indexes. Results are filtered by tenant, workspace and repository.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        let mut hits = Vec::new();
        for name in &query.indexes {
            hits.extend(self.search_one(*name, query)?);
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(hits)
    }

    fn search_one(&self, name: IndexName, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        let position = name.position();
        let Some(text_query) = self.text_query(position, &query.text) else { return Ok(Vec::new()) };
        let fields = self.fields;
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = vec![
            (Occur::Must, term_query(fields.tenant_id, &query.tenant_id)),
            (Occur::Must, term_query(fields.workspace_id, &query.workspace_id)),
            (Occur::Must, text_query),
        ];
        if let Some(repository) = &query.repository_id {
            clauses.push((Occur::Must, term_query(fields.repository_id, repository)));
        }
        let searcher = self.readers[position].searcher();
        let top = searcher.search(&BooleanQuery::new(clauses), &TopDocs::with_limit(query.limit.max(1)).order_by_score())?;
        let mut out = Vec::with_capacity(top.len());
        for (score, address) in top {
            let doc: TantivyDocument = searcher.doc(address)?;
            out.push(SearchHit {
                entity_ref: text_value(&doc, fields.entity_ref),
                kind: text_value(&doc, fields.kind),
                path: text_value(&doc, fields.path),
                name: text_value(&doc, fields.name).lines().next().unwrap_or_default().to_string(),
                score,
                seq: doc.get_first(fields.seq).and_then(|v| v.as_u64()).unwrap_or(0),
                index: name,
            });
        }
        Ok(out)
    }

    fn text_query(&self, position: usize, text: &str) -> Option<Box<dyn Query>> {
        let fields = self.fields;
        let mut parser = QueryParser::for_index(&self.indexes[position], vec![fields.name, fields.path, fields.body]);
        parser.set_field_boost(fields.name, NAME_BOOST);
        parser.set_field_boost(fields.path, PATH_BOOST);
        let cleaned: String = expand_identifiers(text)
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c.is_whitespace() { c } else { ' ' })
            .collect();
        if cleaned.trim().is_empty() {
            return None;
        }
        let (exact, _errors) = parser.parse_query_lenient(&cleaned);
        // Conversations (messages, memory, decisions) are mostly inflected natural language: there
        // every word also matches by its stem. Code and documents use stems only for words that no
        // index has as an exact word, so that exact identifiers keep their rank.
        let missing = if position == IndexName::Conversations.position() { long_word_stems(&cleaned) } else { self.missing_word_stems(&cleaned) };
        if missing.is_empty() {
            return Some(exact);
        }
        let mut stem_parser = QueryParser::for_index(&self.indexes[position], vec![fields.stems]);
        stem_parser.set_field_boost(fields.stems, STEM_BOOST);
        let (stemmed, _errors) = stem_parser.parse_query_lenient(&missing);
        Some(Box::new(BooleanQuery::new(vec![(Occur::Should, exact), (Occur::Should, stemmed)])))
    }

    /// Returns the stems of the question words that no document of the index has as an exact word,
    /// for example the inflected Turkish "motorun" (the text has "motor"). Only these words use the
    /// stem field, so that a word that exists ("quaternion") keeps its exact ranking (ADR-013).
    /// A word counts as present if any of the three indexes has it, so that an exact match in one
    /// index is not outranked by a stem match in another.
    fn missing_word_stems(&self, text: &str) -> String {
        let searchers: Vec<_> = self.readers.iter().map(|r| r.searcher()).collect();
        let fields = self.fields;
        let absent = |word: &str| {
            let lower = word.to_lowercase();
            searchers.iter().all(|searcher| {
                [fields.name, fields.body].iter().all(|f| searcher.doc_freq(&Term::from_field_text(*f, &lower)).unwrap_or(0) == 0)
            })
        };
        text.split_whitespace().filter(|w| w.chars().count() > STEM_CHARS && absent(w)).map(stem_text).collect::<Vec<_>>().join(" ")
    }
}

/// Shortest question word that the conversation search also matches by its stem. Inflected
/// Turkish words are long ("doğruluğu", "maliyeti"); short words are mostly roots, and a short
/// English word would collide with others of the same prefix ("attack", "attachment").
const MIN_STEMMED_WORD_CHARS: usize = 7;

/// Returns the stems of the long words of a question.
fn long_word_stems(text: &str) -> String {
    text.split_whitespace().filter(|w| w.chars().count() >= MIN_STEMMED_WORD_CHARS).map(stem_text).collect::<Vec<_>>().join(" ")
}

fn term_query(field: tantivy::schema::Field, value: &str) -> Box<dyn Query> {
    Box::new(TermQuery::new(Term::from_field_text(field, value), IndexRecordOption::Basic))
}

fn text_value(doc: &TantivyDocument, field: tantivy::schema::Field) -> String {
    doc.get_first(field).and_then(|v| v.as_str()).unwrap_or_default().to_string()
}
