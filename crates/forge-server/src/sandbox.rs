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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::request_relative_path;

    #[test]
    fn a_request_path_carrying_a_network_drive_parent_or_hidden_shape_is_refused() {
        for refused in [
            "\\\\host\\s\\x",
            "/\\\\host\\s\\x",
            "alerts\\..\\secret",
            "alerts/\\\\host\\s",
            "C:/x",
            "c:x",
            "a:b",
            "alerts/index.html:stream",
            "a/../b",
            "..",
            "alerts/..",
            ".hidden",
            "alerts/.git/config",
            "alerts/./index.html",
            "a\0b",
        ] {
            assert_eq!(
                request_relative_path(refused),
                None,
                "{refused:?} must be refused before any filesystem call"
            );
        }
    }

    #[test]
    fn a_plain_relative_request_path_maps_to_its_normal_segments() {
        for (requested, expected) in [
            (
                "alerts/index.html",
                PathBuf::from("alerts").join("index.html"),
            ),
            ("//alerts/", PathBuf::from("alerts")),
            (
                "/alerts//media/clip.wav",
                PathBuf::from("alerts").join("media").join("clip.wav"),
            ),
            ("", PathBuf::new()),
            ("/", PathBuf::new()),
            ("alerts/файл.js", PathBuf::from("alerts").join("файл.js")),
        ] {
            assert_eq!(
                request_relative_path(requested),
                Some(expected),
                "{requested:?}"
            );
        }
    }
}
