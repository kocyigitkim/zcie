//! Token estimates and budget limits.

use serde::Serialize;

pub use zcie_types::estimate_tokens;

/// Smallest permitted token budget.
pub const MIN_BUDGET: u32 = 50;

/// Returns the budget to use: the request budget or the default, at most `max`.
pub fn clamp_budget(requested: Option<u32>, default: u32, max: u32) -> u32 {
    requested.unwrap_or(default).clamp(MIN_BUDGET, max.max(MIN_BUDGET))
}

/// Estimates the tokens of a value serialized as JSON.
pub fn json_tokens<T: Serialize>(value: &T) -> u32 {
    serde_json::to_string(value).map(|text| estimate_tokens(&text)).unwrap_or(0)
}
