//! Cursor helpers.

/// Makes a cursor `<query_id>:<offset>`.
pub fn make_cursor(query_id: &str, offset: usize) -> String {
    format!("{query_id}:{offset}")
}

/// Parses a cursor into the query ID and the offset.
pub fn parse_cursor(cursor: &str) -> Option<(String, usize)> {
    let (query_id, offset) = cursor.rsplit_once(':')?;
    Some((query_id.to_string(), offset.parse().ok()?))
}
