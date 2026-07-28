//! Key-parity guard for the Fluent localization catalogs.
//!
//! forge-desktop is a binary crate, so this integration test cannot import
//! crate internals. It reads the `.ftl` files as plain text and enforces the
//! invariants we previously checked by hand on every commit: the `en` and `uk`
//! catalogs must define the exact same set of top-level message keys, no key may
//! be defined twice within a single catalog, and a message must reference the
//! same `$placeholder` names in both locales.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

fn locale_path(locale: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("locales")
        .join(locale)
        .join("main.ftl")
}

fn load(locale: &str) -> String {
    let path = locale_path(locale);
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        !content.is_empty(),
        "could not read locale catalog at {}",
        path.display()
    );
    content
}

/// Extract top-level Fluent message keys, preserving order and duplicates.
///
/// A message key is a line of the form `key = value` OR `key =` (a block-only
/// value whose plural/attribute body lives on the following indented lines).
/// Comment lines (`#`), blank lines, terms (`-name`), and every indented
/// continuation line (plural selectors `[one]` / `*[other]`, attribute lines)
/// begin with a character that is not an ASCII lowercase letter and are skipped.
fn message_keys(content: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for raw in content.lines() {
        let line = raw.trim_end();
        let Some(first) = line.chars().next() else {
            continue;
        };
        if !first.is_ascii_lowercase() {
            continue;
        }
        let ident_len = line
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(line.len());
        let (key, rest) = line.split_at(ident_len);
        if rest == " =" || rest.starts_with(" = ") {
            keys.push(key.to_owned());
        }
    }
    keys
}

/// Map each message key to the set of `$placeholder` names its pattern references,
/// following indented continuation lines (plural selectors, attributes) into the
/// message they belong to.
fn placeholders_by_key(content: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut by_key: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut current: Option<String> = None;

    for raw in content.lines() {
        let line = raw.trim_end();
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if !indented {
            current = None;
            if let Some(first) = line.chars().next()
                && first.is_ascii_lowercase()
            {
                let ident_len = line
                    .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
                    .unwrap_or(line.len());
                let (key, rest) = line.split_at(ident_len);
                if rest == " =" || rest.starts_with(" = ") {
                    current = Some(key.to_owned());
                    by_key.entry(key.to_owned()).or_default();
                }
            }
        }
        let Some(key) = current.as_ref() else {
            continue;
        };
        let entry = by_key.entry(key.clone()).or_default();
        for name in placeholder_names(line) {
            entry.insert(name);
        }
    }

    by_key
}

fn placeholder_names(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut names = Vec::new();
    for (index, _) in line.match_indices('$') {
        let start = index + 1;
        let end = bytes[start..]
            .iter()
            .position(|b| !(b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_'))
            .map_or(line.len(), |offset| start + offset);
        if end > start {
            names.push(line[start..end].to_owned());
        }
    }
    names
}

fn duplicates(keys: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut dups = Vec::new();
    for key in keys {
        if !seen.insert(key.as_str()) {
            dups.push(key.clone());
        }
    }
    dups
}

#[test]
fn en_and_uk_define_identical_message_key_sets() {
    let en_keys = message_keys(&load("en"));
    let uk_keys = message_keys(&load("uk"));

    // Guard against a parser that silently matches nothing (which would make
    // the parity assertion below vacuously pass). These anchors also confirm
    // both the plain `key = value` and the block-only `key =` forms are parsed.
    for anchor in ["nav_home", "common_save", "triggers_override_badge"] {
        assert!(
            en_keys.iter().any(|k| k == anchor),
            "parser failed to find anchor key `{anchor}` in en catalog"
        );
    }

    let en: HashSet<&str> = en_keys.iter().map(String::as_str).collect();
    let uk: HashSet<&str> = uk_keys.iter().map(String::as_str).collect();

    let mut en_only: Vec<&str> = en.difference(&uk).copied().collect();
    let mut uk_only: Vec<&str> = uk.difference(&en).copied().collect();
    en_only.sort_unstable();
    uk_only.sort_unstable();

    assert!(
        en_only.is_empty() && uk_only.is_empty(),
        "locale key sets diverge:\n  en-only ({}): {en_only:?}\n  uk-only ({}): {uk_only:?}",
        en_only.len(),
        uk_only.len()
    );
}

#[test]
fn no_message_key_is_defined_twice_within_a_locale() {
    for locale in ["en", "uk"] {
        let dups = duplicates(&message_keys(&load(locale)));
        assert!(
            dups.is_empty(),
            "{locale} catalog defines these keys more than once: {dups:?}"
        );
    }
}

#[test]
fn en_and_uk_reference_the_same_placeholders_in_every_message() {
    let en = placeholders_by_key(&load("en"));
    let uk = placeholders_by_key(&load("uk"));

    // A message whose translation drops or renames a placeholder renders the raw
    // `{$name}` (or loses the value) at runtime, which the key-set parity test above
    // cannot see.
    assert_eq!(
        en.get("hotkeys_conflict_body")
            .map(|args| args.iter().cloned().collect::<Vec<_>>()),
        Some(vec!["holder".to_owned()]),
        "parser failed to find the anchor placeholder, making this test vacuous"
    );

    let mut diverging = Vec::new();
    for (key, en_args) in &en {
        let Some(uk_args) = uk.get(key) else {
            continue;
        };
        if en_args != uk_args {
            diverging.push(format!("{key}: en={en_args:?} uk={uk_args:?}"));
        }
    }

    assert!(
        diverging.is_empty(),
        "these messages reference different placeholders per locale:\n  {}",
        diverging.join("\n  ")
    );
}

/// Scan a Rust source tree for `tr!("key"` literals, tolerating the multi-line call
/// form the view code uses.
fn literal_tr_keys(root: &Path) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                collect_tr_keys(&content, &mut keys);
            }
        }
    }
    keys
}

fn collect_tr_keys(content: &str, keys: &mut BTreeSet<String>) {
    let bytes = content.as_bytes();
    for (index, _) in content.match_indices("tr!(") {
        // Skip the tail of a longer identifier such as `include_str!(`.
        if index > 0 && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_') {
            continue;
        }
        let mut cursor = index + "tr!(".len();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'"' {
            continue;
        }
        cursor += 1;
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b'"' {
            cursor += 1;
        }
        if cursor < bytes.len() && cursor > start {
            keys.insert(content[start..cursor].to_owned());
        }
    }
}

#[test]
fn every_tr_key_used_in_the_source_tree_exists_in_the_catalogs() {
    let crates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");

    let mut used = literal_tr_keys(&crates.join("forge-desktop").join("src"));
    used.extend(literal_tr_keys(
        &crates.join("forge-components").join("src"),
    ));

    // A parser that matched nothing would make the assertion below vacuous.
    assert!(
        used.len() > 500,
        "expected the source tree to reference many keys, found {}",
        used.len()
    );

    let defined: BTreeSet<String> = message_keys(&load("en")).into_iter().collect();
    let missing: Vec<&String> = used.difference(&defined).collect();

    assert!(
        missing.is_empty(),
        "these keys are used in code but absent from the catalog: {missing:?}"
    );
}
