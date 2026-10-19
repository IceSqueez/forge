use crate::pack::layout::DocumentLayout;

pub const SETUP_ROOT: &str = "setup";

pub(crate) const SETUP_LAYOUT: DocumentLayout = DocumentLayout::rooted_at(SETUP_ROOT);
