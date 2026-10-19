//! The two MCP tools: `zcie_query` and `zcie_expand` (`docs/10-api-and-gateways.md` sections 4.1, 4.2).

use std::sync::Arc;

use rmcp::model::{JsonObject, Tool};
use serde_json::{Value, json};
use zcie_sdk::Client;
use zcie_types::query::{ExpandLevel, QueryMode, QueryRequest};

/// Name of the query tool.
pub const QUERY_TOOL: &str = "zcie_query";
/// Name of the expand tool.
pub const EXPAND_TOOL: &str = "zcie_expand";

const QUERY_DESCRIPTION: &str = "Search project code, symbols, tasks, commits and past conversations in the ZCIE memory (local index of all projects on this computer, verified links). Default: the current project; scope=global or project=<name> reads other projects. Returns cited refs, a short answer when the data is exact, and the source code of the best matching symbols. Usually no file read is needed after this call. Use zcie_expand only for more source or history.";
const EXPAND_DESCRIPTION: &str = "Expand ZCIE refs into evidence, source code or original records under a token budget. One call can read several line ranges \"path:start-end\" (at most 10, 100 lines each; a longer range returns a \"more\" range). Use it instead of reading whole files. with=[\"callers\",\"tests\"] adds the lines that use a symbol ref.";

/// Returns the definitions of the two tools.
pub fn tool_list() -> Vec<Tool> {
    vec![
        Tool::new(QUERY_TOOL, QUERY_DESCRIPTION, Arc::new(schema_object(query_schema()))),
        Tool::new(EXPAND_TOOL, EXPAND_DESCRIPTION, Arc::new(schema_object(expand_schema()))),
    ]
}

fn schema_object(value: Value) -> JsonObject {
    match value {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    }
}

fn query_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "question": { "type": "string", "description": "Question, symbol name or ref." },
            "mode": { "type": "string", "enum": ["auto", "exact", "search", "timeline", "memory", "map", "locate"] },
            "token_budget": { "type": "integer", "minimum": 50, "maximum": 4000 },
            "include_source": { "type": "boolean", "description": "Add the source of the best symbols (default true)." },
            "scope": { "type": "string", "enum": ["project", "global"], "description": "project (default): this project; global: all projects on this computer." },
            "project": { "type": "string", "description": "Another project by name or ID (default: the current project)." }
        },
        "required": ["question"]
    })
}

fn expand_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "refs": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 20, "description": "Refs (sym:..., file:..., mem:...) or line ranges path:start-end." },
            "with": { "type": "array", "items": { "type": "string", "enum": ["callers", "tests"] }, "description": "For symbol refs: add the lines of the callers and the tests (at most 5 each)." },
            "level": { "type": "string", "enum": ["evidence", "source", "audit"] },
            "token_budget": { "type": "integer", "minimum": 50, "maximum": 4000 }
        },
        "required": ["refs"]
    })
}

/// Runs one tool call. Returns the response JSON text, or an error message.
pub fn run_tool(client: &Client, name: &str, arguments: &JsonObject) -> Result<String, String> {
    let response = match name {
        QUERY_TOOL => run_query(client, arguments)?,
        EXPAND_TOOL => run_expand(client, arguments)?,
        other => return Err(format!("unknown tool: {other}")),
    };
    serde_json::to_string(&response).map_err(|e| e.to_string())
}

fn run_query(client: &Client, arguments: &JsonObject) -> Result<Value, String> {
    let question = arguments
        .get("question")
        .and_then(Value::as_str)
        .ok_or("question is required")?;
    let mode = match arguments.get("mode") {
        Some(value) => serde_json::from_value::<QueryMode>(value.clone()).map_err(|e| e.to_string())?,
        None => QueryMode::Auto,
    };
    let total = budget(arguments).unwrap_or(DEFAULT_QUERY_BUDGET);
    let scope = match arguments.get("scope").and_then(Value::as_str) {
        Some("global") => zcie_types::QueryScope::Global,
        _ => zcie_types::QueryScope::Project,
    };
    let project = arguments.get("project").and_then(Value::as_str).unwrap_or(client.workspace());
    let request = QueryRequest {
        workspace: project.to_string(),
        scope,
        cross_project: true,
        question: question.to_string(),
        mode,
        token_budget: Some((total / 2).max(MIN_PART_BUDGET)),
        ..QueryRequest::default()
    };
    let response = client.query_request(request).map_err(|e| e.to_string())?;
    let source_budget = total.saturating_sub(response.token_estimate);
    let include_source = arguments.get("include_source").and_then(Value::as_bool).unwrap_or(true);
    let mut value = serde_json::to_value(&response).map_err(|e| e.to_string())?;
    if include_source && source_budget >= MIN_PART_BUDGET {
        let refs: Vec<String> = response.results.iter().filter(|r| r.kind == "symbol").take(SOURCE_SYMBOLS).map(|r| r.entity_ref.clone()).collect();
        if !refs.is_empty()
            && let Ok(expanded) = client.expand(&refs, ExpandLevel::Source, Some(source_budget))
        {
            value["source"] = serde_json::to_value(expanded.items).unwrap_or(Value::Null);
        }
    }
    Ok(compact(value))
}

/// Default total token budget of one `zcie_query` call (answer plus source).
const DEFAULT_QUERY_BUDGET: u32 = 900;
/// Smallest budget of one part of the answer.
const MIN_PART_BUDGET: u32 = 120;
/// Number of symbols whose source `zcie_query` adds.
const SOURCE_SYMBOLS: usize = 2;

/// Removes null values and empty arrays, so the model reads fewer tokens.
fn compact(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, v)| !v.is_null() && !matches!(v, Value::Array(a) if a.is_empty()))
                .map(|(k, v)| (k, compact(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(compact).collect()),
        other => other,
    }
}

fn run_expand(client: &Client, arguments: &JsonObject) -> Result<Value, String> {
    let refs: Vec<String> = arguments
        .get("refs")
        .and_then(Value::as_array)
        .ok_or("refs is required")?
        .iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    if refs.is_empty() {
        return Err("refs is empty".to_string());
    }
    let level = match arguments.get("level") {
        Some(value) => serde_json::from_value::<ExpandLevel>(value.clone()).map_err(|e| e.to_string())?,
        None => ExpandLevel::Evidence,
    };
    let with: Vec<String> = arguments
        .get("with")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let response = client.expand_with(&refs, level, budget(arguments), &with).map_err(|e| e.to_string())?;
    serde_json::to_value(response).map(compact).map_err(|e| e.to_string())
}

fn budget(arguments: &JsonObject) -> Option<u32> {
    arguments
        .get("token_budget")
        .and_then(Value::as_u64)
        .map(|value| value.min(u64::from(u32::MAX)) as u32)
}
