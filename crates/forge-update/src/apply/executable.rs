use std::io;
use std::path::Path;

#[cfg(unix)]
pub(super) fn copy_mode_with_execute(target: &Path, staged: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    const PERMISSION_BITS: u32 = 0o7777;
    const EXECUTE_BITS: u32 = 0o111;
    const FALLBACK_MODE: u32 = 0o755;

    let base = std::fs::metadata(target)
        .map(|metadata| metadata.permissions().mode() & PERMISSION_BITS)
        .unwrap_or(FALLBACK_MODE);
    std::fs::set_permissions(staged, std::fs::Permissions::from_mode(base | EXECUTE_BITS))
}

#[cfg(not(unix))]
pub(super) fn copy_mode_with_execute(_target: &Path, _staged: &Path) -> io::Result<()> {
    Ok(())
}
