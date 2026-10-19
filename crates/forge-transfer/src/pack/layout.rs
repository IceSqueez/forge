use crate::layout::{ARCHIVE_SEPARATOR, EntryShape, FORBIDDEN_NAME_CHARS};

pub const PACK_ROOT: &str = "pack";

const DOCUMENTS_FILE: &str = "documents.json";
const MEDIA_DIR: &str = "media";
const CLIPS_DIR: &str = "clips";
const OVERLAYS_DIR: &str = "overlays";
const CURRENT_DIR: &str = ".";
const PARENT_DIR: &str = "..";

pub(crate) fn documents_entry() -> String {
    pack_path([DOCUMENTS_FILE])
}

pub(crate) fn media_entry(index: usize) -> String {
    pack_path([MEDIA_DIR, &index.to_string()])
}

pub(crate) fn clip_entry(index: usize) -> String {
    pack_path([CLIPS_DIR, &index.to_string()])
}

pub(crate) fn overlay_entry(index: usize, file_segments: &[&str]) -> String {
    let index = index.to_string();
    pack_path(
        [OVERLAYS_DIR, index.as_str()]
            .into_iter()
            .chain(file_segments.iter().copied()),
    )
}

fn pack_path<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    std::iter::once(PACK_ROOT)
        .chain(segments)
        .collect::<Vec<_>>()
        .join(ARCHIVE_SEPARATOR)
}

pub(crate) fn safe_segments(name: &str) -> Option<Vec<&str>> {
    let segments: Vec<&str> = name.split(ARCHIVE_SEPARATOR).collect();
    let safe = segments.iter().all(|segment| {
        !segment.is_empty()
            && *segment != CURRENT_DIR
            && *segment != PARENT_DIR
            && !segment.contains(FORBIDDEN_NAME_CHARS)
    });
    safe.then_some(segments)
}

pub(crate) fn is_pack_scope(rest: &[&str], shape: EntryShape) -> bool {
    match (rest, shape) {
        ([], EntryShape::Dir) => true,
        ([name], EntryShape::File) => *name == DOCUMENTS_FILE,
        ([dir], EntryShape::Dir) => [MEDIA_DIR, CLIPS_DIR, OVERLAYS_DIR].contains(dir),
        ([dir, _], EntryShape::File) => [MEDIA_DIR, CLIPS_DIR].contains(dir),
        ([dir, _], EntryShape::Dir) => *dir == OVERLAYS_DIR,
        ([dir, _, _, ..], _) => *dir == OVERLAYS_DIR,
        _ => false,
    }
}
