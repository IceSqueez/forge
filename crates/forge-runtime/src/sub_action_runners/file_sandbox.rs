use std::path::{Component, Path, PathBuf};

const DRIVE_SEPARATOR: u8 = b':';

pub(super) async fn resolve_sandboxed(rel: &str) -> Result<PathBuf, String> {
    if rel.trim().is_empty() {
        return Err("path is empty".to_owned());
    }
    if is_rooted(rel) {
        return Err(absolute_path_error());
    }
    let candidate = PathBuf::from(rel);
    for component in candidate.components() {
        match component {
            Component::ParentDir => {
                return Err(
                    "\"..\" is not allowed - the path must stay inside the files folder".to_owned(),
                );
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(absolute_path_error());
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    let root = forge_platform_core::paths::assets_dir();
    let _ = tokio::fs::create_dir_all(&root).await;
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
                    return Err(escape_error());
                }
                canon_prefix = canon_probe;
                remaining.next();
            }
            Err(_) => {
                if is_symlink(&probe).await {
                    return Err(escape_error());
                }
                break;
            }
        }
    }

    let tail: PathBuf = remaining.collect();
    if tail.as_os_str().is_empty() {
        return Ok(canon_prefix);
    }
    Ok(canon_prefix.join(tail))
}

fn is_rooted(rel: &str) -> bool {
    let drive_letter = matches!(
        rel.as_bytes(),
        [letter, DRIVE_SEPARATOR, ..] if letter.is_ascii_alphabetic()
    );
    rel.starts_with('/') || rel.starts_with('\\') || drive_letter
}

async fn is_symlink(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path)
        .await
        .is_ok_and(|meta| meta.file_type().is_symlink())
}

fn absolute_path_error() -> String {
    "absolute paths are not allowed - put the file in the files folder and use a path relative to it".to_owned()
}

fn escape_error() -> String {
    "path goes through a link that is broken or leads outside the files folder".to_owned()
}

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

    #[test]
    fn glob_matches_table() {
        let cases = [
            ("*", "anything.txt", true),
            ("", "anything.txt", true),
            ("file.txt", "file.txt", true),
            ("file.txt", "other.txt", false),
            ("File", "file", false),
            ("abcdef", "abc", false),
            ("*.txt", "file.txt", true),
            ("*.txt", "file.md", false),
            ("file.*", "file.txt", true),
            ("file.*", "other.txt", false),
            ("a*b", "axxxb", true),
            ("a*b", "ab", true),
            ("a*b", "a", false),
            ("a*b", "axbq", false),
            ("a*b*c", "axbyc", true),
            ("a*b*c", "axyc", false),
            ("*mid*", "xxmidyy", true),
            ("*mid*", "xxxyy", false),
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
