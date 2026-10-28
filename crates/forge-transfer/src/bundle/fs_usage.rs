use std::path::Path;

pub(crate) fn file_bytes(path: &Path) -> u64 {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.file_type().is_file())
        .map_or(0, |metadata| metadata.len())
}

pub(crate) fn directory_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let child = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => directory_bytes(&child),
                Ok(kind) if kind.is_file() => file_bytes(&child),
                _ => 0,
            }
        })
        .sum()
}
