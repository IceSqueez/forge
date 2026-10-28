use crate::bundle::layout::{ARCHIVE_SEPARATOR, EntryShape, FORBIDDEN_NAME_CHARS};

pub const PACK_ROOT: &str = "pack";
pub const SETUP_ROOT: &str = "setup";

const DOCUMENTS_FILE: &str = "documents.json";
const MEDIA_DIR: &str = "media";
const CLIPS_DIR: &str = "clips";
const OVERLAYS_DIR: &str = "overlays";
const CURRENT_DIR: &str = ".";
const PARENT_DIR: &str = "..";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DocumentLayout {
    root: &'static str,
}

pub(crate) const PACK_LAYOUT: DocumentLayout = DocumentLayout::rooted_at(PACK_ROOT);
pub(crate) const SETUP_LAYOUT: DocumentLayout = DocumentLayout::rooted_at(SETUP_ROOT);

impl DocumentLayout {
    pub(crate) const fn rooted_at(root: &'static str) -> Self {
        Self { root }
    }

    pub(crate) fn root(self) -> &'static str {
        self.root
    }

    pub(crate) fn documents_entry(self) -> String {
        self.path([DOCUMENTS_FILE])
    }

    pub(crate) fn media_entry(self, index: usize) -> String {
        self.path([MEDIA_DIR, &index.to_string()])
    }

    pub(crate) fn clip_entry(self, index: usize) -> String {
        self.path([CLIPS_DIR, &index.to_string()])
    }

    pub(crate) fn overlay_entry(self, index: usize, file_segments: &[&str]) -> String {
        let index = index.to_string();
        self.path(
            [OVERLAYS_DIR, index.as_str()]
                .into_iter()
                .chain(file_segments.iter().copied()),
        )
    }

    fn path<'a>(self, segments: impl IntoIterator<Item = &'a str>) -> String {
        std::iter::once(self.root)
            .chain(segments)
            .collect::<Vec<_>>()
            .join(ARCHIVE_SEPARATOR)
    }
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

pub(crate) fn is_documents_scope(rest: &[&str], shape: EntryShape) -> bool {
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
