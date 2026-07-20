use std::path::{Path, PathBuf};

pub(crate) async fn confine(root: &Path, requested: &Path) -> Option<PathBuf> {
    let canon_root = tokio::fs::canonicalize(root).await.ok()?;
    let joined = canon_root.join(requested);
    let canon_target = tokio::fs::canonicalize(&joined).await.ok()?;
    canon_target
        .starts_with(&canon_root)
        .then_some(canon_target)
}
