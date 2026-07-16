//! Key-parity guard for the Fluent localization catalogs.
//!
//! forge-desktop is a binary crate, so this integration test cannot import
//! crate internals. It reads the `.ftl` files as plain text and enforces the
//! two invariants we previously checked by hand on every commit: the `en` and
//! `uk` catalogs must define the exact same set of top-level message keys, and
//! no key may be defined twice within a single catalog.

use std::collections::HashSet;
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
