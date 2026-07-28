//! Key-parity guard for the Fluent localization catalogs.
//!
//! forge-desktop is a binary crate, so this integration test cannot import
//! crate internals. It reads the `.ftl` files as plain text and enforces the
//! invariants we previously checked by hand on every commit: the `en` and `uk`
//! catalogs must define the exact same set of top-level message keys, no key may
//! be defined twice within a single catalog, and a message must reference the
//! same `$placeholder` names in both locales.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

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
