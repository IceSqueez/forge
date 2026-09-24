use std::path::{Component, Path, PathBuf};

const FORBIDDEN_REQUEST_CHARS: [char; 3] = ['\\', ':', '\0'];
const PARENT_MARKER: &str = "..";
const HIDDEN_PREFIX: char = '.';

/// Never touches the filesystem: a backslash, drive or stream colon, hidden or parent segment
/// is refused here, so a UNC share or an absolute path is never handed to `canonicalize`.
pub(crate) fn request_relative_path(url_path: &str) -> Option<PathBuf> {
    if url_path.contains(FORBIDDEN_REQUEST_CHARS) || url_path.contains(PARENT_MARKER) {
        return None;
    }
    let mut relative = PathBuf::new();
    for segment in url_path.split('/').filter(|segment| !segment.is_empty()) {
        if segment.starts_with(HIDDEN_PREFIX) {
            return None;
        }
        relative.push(segment);
    }
    relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then_some(relative)
}

pub(crate) async fn confine(root: &Path, requested: &Path) -> Option<PathBuf> {
    let canon_root = tokio::fs::canonicalize(root).await.ok()?;
    let joined = canon_root.join(requested);
    let canon_target = tokio::fs::canonicalize(&joined).await.ok()?;
    canon_target
        .starts_with(&canon_root)
        .then_some(canon_target)
}
