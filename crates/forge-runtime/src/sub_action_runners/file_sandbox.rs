use std::path::{Component, PathBuf};

/// Rejects absolute paths, Windows path prefixes, and parent-dir (`..`) traversal.
/// Confines the resolved result to `<data_dir>/assets/`.
pub(super) fn resolve_sandboxed(rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err("path is empty".to_owned());
    }
    if rel.starts_with('/') || rel.starts_with('\\') {
        return Err("absolute paths are forbidden".to_owned());
    }
    let candidate = PathBuf::from(rel);
    for component in candidate.components() {
        match component {
            Component::ParentDir => return Err("parent dir traversal forbidden".to_owned()),
            Component::Prefix(_) | Component::RootDir => {
                return Err("rooted paths are forbidden".to_owned());
            }
            _ => {}
        }
    }
    let root = forge_platform_core::paths::data_dir().join("assets");
    Ok(root.join(candidate))
}

/// `*` wildcard matches any character sequence in `name`; all other characters are literal.
/// Matching is case-sensitive and operates on the entry basename only.
pub(super) fn glob_matches(pattern: &str, name: &str) -> bool {
    if pattern.is_empty() || pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return name == pattern;
    }
    if !name.starts_with(parts[0]) {
        return false;
    }
    let suffix = parts[parts.len() - 1];
    if name.len() < parts[0].len() + suffix.len() {
        return false;
    }
    if !suffix.is_empty() && !name.ends_with(suffix) {
        return false;
    }
    let search_end = name.len() - suffix.len();
    let mut pos = parts[0].len();
    for part in &parts[1..parts.len() - 1] {
        match name[pos..search_end].find(part) {
            Some(i) => pos += i + part.len(),
            None => return false,
        }
    }
    true
}
