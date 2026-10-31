#![allow(dead_code, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use forge_overlay::{
    OverlayConfig, OverlayInstance, OverlayKindDescriptor, OverlayKindRegistry, OverlayMedia,
    RESERVED_DIRECTORY, SampleContext, register_builtin_kinds,
};
use forge_types::Variant;
use tempfile::TempDir;

pub(crate) const ALERT_KIND: &str = forge_overlay::kinds::alert::KIND_ID;
pub(crate) const BLANK_KIND: &str = forge_overlay::kinds::blank::KIND_ID;
pub(crate) const CHAT_KIND: &str = forge_overlay::kinds::chat::KIND_ID;
pub(crate) const FRAME_KIND: &str = forge_overlay::kinds::frame::KIND_ID;
pub(crate) const GOAL_KIND: &str = forge_overlay::kinds::goal::KIND_ID;
pub(crate) const LATEST_KIND: &str = forge_overlay::kinds::latest::KIND_ID;
pub(crate) const TICKER_KIND: &str = forge_overlay::kinds::ticker::KIND_ID;
pub(crate) const IDENTITY: &str = "sub-alert-1";
pub(crate) const BLOB_IDENTITY: &str =
    "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";
pub(crate) const PAGE_CREDENTIAL: &str = "7c1e4a90b2d84f36a5c0e9b71d3f8a24";

pub(crate) const UNSAFE_IDENTITIES: &[(&str, &str)] = &[
    ("..", "parent traversal"),
    (".", "the current directory"),
    ("a/b", "a nested path"),
    ("/abs", "an absolute path"),
    ("a\\b", "a windows separator"),
    ("Up-Case", "an uppercase letter"),
    ("dot.name", "a dot segment"),
    ("with space", "a space"),
    ("", "an empty name"),
    ("-leading-dash", "a leading dash"),
    (RESERVED_DIRECTORY, "the reserved shared subtree"),
];

pub(crate) fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

pub(crate) fn with_kind<T>(kind_id: &str, body: impl FnOnce(&dyn OverlayKindDescriptor) -> T) -> T {
    let reg = registry();
    body(reg.get(kind_id).expect("a registered builtin kind"))
}

pub(crate) fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

pub(crate) fn sub_alert_instance(kind_id: &str) -> OverlayInstance {
    OverlayInstance {
        id: IDENTITY.to_owned(),
        display_name: "Sub alert".to_owned(),
        kind_id: kind_id.to_owned(),
        config: OverlayConfig::new(),
        source_overrides: Vec::new(),
        credential: None,
        media: OverlayMedia::default(),
        sample: SampleContext::neutral(),
    }
}

pub(crate) fn unborn_root(home: &TempDir) -> PathBuf {
    home.path().join("overlays")
}

pub(crate) fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("the directory exists")
        .map(|entry| {
            entry
                .expect("a readable directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}
