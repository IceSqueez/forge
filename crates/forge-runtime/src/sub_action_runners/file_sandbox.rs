use std::path::{Component, PathBuf};

/// Canonicalizes the deepest existing prefix to reject a symlink escape inside `assets/`.
pub(super) async fn resolve_sandboxed(rel: &str) -> Result<PathBuf, String> {
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
    let Ok(canon_root) = tokio::fs::canonicalize(&root).await else {
        return Ok(root.join(candidate));
    };

    let mut canon_prefix = canon_root.clone();
    let mut remaining = candidate.components().peekable();
    while let Some(component) = remaining.peek().copied() {
        let probe = canon_prefix.join(component.as_os_str());
        match tokio::fs::canonicalize(&probe).await {
            Ok(canon_probe) => {
                if !canon_probe.starts_with(&canon_root) {
                    return Err("path escapes sandbox root".to_owned());
                }
                canon_prefix = canon_probe;
                remaining.next();
            }
            Err(_) => break,
        }
    }

    let tail: PathBuf = remaining.collect();
    Ok(canon_prefix.join(tail))
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // Security boundary: every input here MUST be rejected. A regression that
    // lets any of them through is a sandbox escape (read/write outside assets/).
    #[tokio::test]
    async fn resolve_sandboxed_rejects_traversal_and_rooted_paths() {
        let escapes = [
            "..",                // bare parent
            "../etc/passwd",     // leading parent traversal
            "a/../../b",         // interior parent traversal escaping root
            "sub/../../../etc",  // traversal after a legal-looking prefix
            "/abs",              // absolute (unix root)
            "/etc/passwd",       // absolute system path
            "\\abs",             // leading backslash (Windows-style absolute / UNC)
            "\\\\server\\share", // UNC path
            "",                  // empty
        ];
        for bad in escapes {
            assert!(
                resolve_sandboxed(bad).await.is_err(),
                "expected sandbox rejection for {bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn resolve_sandboxed_joins_valid_relative_under_assets_root() {
        let root = forge_platform_core::paths::data_dir().join("assets");
        let expected_root = tokio::fs::canonicalize(&root).await.unwrap_or(root);
        let resolved = resolve_sandboxed("sub/file.txt").await.unwrap();
        // Confined to the assets root...
        assert!(
            resolved.starts_with(&expected_root),
            "{resolved:?} escaped {expected_root:?}"
        );
        // ...and the relative tail is appended verbatim (not dropped/rewritten).
        assert_eq!(resolved, expected_root.join("sub").join("file.txt"));
    }

    // `*` is the only wildcard; every other char (including `?`) is literal;
    // matching is case-sensitive. Each expected value is hand-derived.
    #[test]
    fn glob_matches_table() {
        let cases = [
            // empty / catch-all patterns match anything
            ("*", "anything.txt", true),
            ("", "anything.txt", true),
            // literal (no wildcard) must match exactly
            ("file.txt", "file.txt", true),
            ("file.txt", "other.txt", false),
            ("File", "file", false),  // case-sensitive
            ("abcdef", "abc", false), // pattern longer than name
            // leading-star = suffix match
            ("*.txt", "file.txt", true),
            ("*.txt", "file.md", false),
            // trailing-star = prefix match
            ("file.*", "file.txt", true),
            ("file.*", "other.txt", false),
            // interior star spans any (incl. empty) run
            ("a*b", "axxxb", true),
            ("a*b", "ab", true),    // star matches empty span
            ("a*b", "a", false),    // shorter than prefix+suffix
            ("a*b", "axbq", false), // suffix mismatch
            // multiple stars
            ("a*b*c", "axbyc", true),
            ("a*b*c", "axyc", false), // missing middle literal
            ("*mid*", "xxmidyy", true),
            ("*mid*", "xxxyy", false),
            // `?` is NOT a wildcard - treated literally
            ("f?le", "f?le", true),
            ("f?le", "file", false),
        ];
        for (pattern, name, expected) in cases {
            assert_eq!(
                glob_matches(pattern, name),
                expected,
                "glob_matches({pattern:?}, {name:?})"
            );
        }
    }
}
