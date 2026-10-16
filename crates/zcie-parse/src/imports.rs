//! Resolution of relative import paths (`docs/07-ingest-and-indexing.md` section 8.2).

/// Returns candidate repository-relative paths for a relative import.
///
/// TypeScript: `./x` and `../x`. Python: `.x` and `..x`. Other imports give an empty list.
pub fn resolve_relative_import(from_path: &str, import: &str) -> Vec<String> {
    if import.starts_with("./") || import.starts_with("../") {
        return resolve_typescript(from_path, import);
    }
    if import.starts_with('.') {
        return resolve_python(from_path, import);
    }
    Vec::new()
}

fn resolve_typescript(from_path: &str, import: &str) -> Vec<String> {
    let joined = format!("{}/{import}", parent_dir(from_path));
    let Some(base) = normalize(&joined) else {
        return Vec::new();
    };
    ["", ".ts", ".tsx", "/index.ts"]
        .iter()
        .map(|suffix| format!("{base}{suffix}"))
        .collect()
}

fn resolve_python(from_path: &str, import: &str) -> Vec<String> {
    let dots = import.chars().take_while(|c| *c == '.').count();
    let rest = import[dots..].replace('.', "/");
    let mut dir = parent_dir(from_path).to_string();
    for _ in 1..dots {
        dir.push_str("/..");
    }
    let Some(base) = normalize(&dir) else {
        return Vec::new();
    };
    let prefix = if base.is_empty() { String::new() } else { format!("{base}/") };
    if rest.is_empty() {
        return vec![format!("{prefix}__init__.py")];
    }
    vec![format!("{prefix}{rest}.py"), format!("{prefix}{rest}/__init__.py")]
}

/// Returns the folder part of a path, or an empty string.
fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

/// Removes `.` and `..` parts. Returns `None` if the path leaves the root.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}
