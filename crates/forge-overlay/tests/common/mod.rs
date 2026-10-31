#![allow(dead_code, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
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

fn skip_quoted(code: &[u8], open: usize) -> usize {
    let quote = code[open];
    let mut at = open + 1;
    while at < code.len() && code[at] != quote {
        at += if code[at] == b'\\' { 2 } else { 1 };
    }
    at + 1
}

fn skip_comment(code: &[u8], open: usize) -> Option<usize> {
    let rest = &code[open..];
    if rest.starts_with(b"//") {
        rest.iter()
            .position(|byte| *byte == b'\n')
            .map(|end| open + end)
    } else if rest.starts_with(b"/*") {
        rest.windows(2)
            .position(|pair| pair == b"*/")
            .map(|end| open + end + 2)
    } else {
        None
    }
}

fn closing_brace(body: &str) -> Option<usize> {
    let code = body.as_bytes();
    let mut depth = 1_usize;
    let mut at = 0;
    while at < code.len() {
        if let Some(after) = skip_comment(code, at) {
            at = after;
            continue;
        }
        match code[at] {
            b'"' | b'\'' | b'`' => {
                at = skip_quoted(code, at);
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

pub(crate) fn js_block(source: &'static str, opening: &str) -> &'static str {
    let start = source
        .find(opening)
        .unwrap_or_else(|| panic!("the script no longer declares '{opening}'"))
        + opening.len();
    let rest = &source[start..];
    let end = closing_brace(rest).unwrap_or_else(|| panic!("'{opening}' is never closed"));
    &rest[..end]
}

pub(crate) fn js_function(source: &'static str, name: &str) -> &'static str {
    let opening = format!("function {name}(");
    let start = source
        .find(&opening)
        .unwrap_or_else(|| panic!("the script no longer declares '{opening}'"));
    let rest = &source[start..];
    let brace = rest
        .find('{')
        .unwrap_or_else(|| panic!("'{opening}' has no body"));
    let end =
        closing_brace(&rest[brace + 1..]).unwrap_or_else(|| panic!("'{opening}' is never closed"));
    &rest[..=brace + end]
}

pub(crate) fn js_object_keys(source: &'static str, opening: &str) -> BTreeSet<String> {
    let mut depth = 0_i32;
    let mut keys = BTreeSet::new();
    for line in js_block(source, opening).lines() {
        let line = line.trim();
        if depth == 0 && !line.is_empty() && !line.starts_with("//") {
            let key = line.split(':').next().unwrap_or(line);
            keys.insert(key.trim_matches([',', '"', ' ']).to_owned());
        }
        depth += line
            .chars()
            .map(|c| match c {
                '{' | '[' | '(' => 1,
                '}' | ']' | ')' => -1,
                _ => 0,
            })
            .sum::<i32>();
    }
    keys
}

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
