//! Search queries (`docs/09-query-and-context.md` section 4.3).

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{TantivyDocument, Term};

use crate::error::Result;
use crate::lexical::schema::{IndexName, expand_identifiers};
use crate::lexical::writer::SearchIndex;

/// Boost of the `name` field.
const NAME_BOOST: f32 = 3.0;
/// Boost of the `path` field.
const PATH_BOOST: f32 = 2.0;

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
        let (parsed, _errors) = parser.parse_query_lenient(&cleaned);
        Some(parsed)
    }
}

fn term_query(field: tantivy::schema::Field, value: &str) -> Box<dyn Query> {
    Box::new(TermQuery::new(Term::from_field_text(field, value), IndexRecordOption::Basic))
}

fn text_value(doc: &TantivyDocument, field: tantivy::schema::Field) -> String {
    doc.get_first(field).and_then(|v| v.as_str()).unwrap_or_default().to_string()
}
