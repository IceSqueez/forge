#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use forge_overlay::config::{DEFAULT_ICON, ICON};
use forge_overlay::{
    CURATED_ICONS, IconCategory, MediaIssue, OverlayConfig, clip_reference, curated_icon,
    glyph_media, image_reference,
};
use forge_types::Variant;
use sha2::{Digest, Sha256};

const CLIP_ULID: &str = "01j9p4s2m7q8v3x5y6z7a8b9c0";
const BLOB_IDENTITY: &str =
    "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";
const SVG_EXTENSION: &str = "svg";
const LICENSE_FILE: &str = "LICENSE";

fn icon_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("icons")
}

fn shipped_files() -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(icon_directory())
        .expect("the vendored icon directory ships with the crate")
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.file_name().and_then(|name| name.to_str()) != Some(LICENSE_FILE))
        .map(|path| {
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("a vendored icon has a utf-8 name")
                .to_owned();
            assert_eq!(
                path.extension().and_then(|ext| ext.to_str()),
                Some(SVG_EXTENSION),
                "{stem} ships beside the glyphs but is not one"
            );
            (stem, std::fs::read(&path).expect("a readable icon file"))
        })
        .collect()
}

fn icon_config(stored: &str) -> OverlayConfig {
    [(ICON.to_owned(), Variant::String(stored.to_owned()))]
        .into_iter()
        .collect()
}

fn content_identity(bytes: &[u8]) -> String {
    let hex: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sha256-{hex}")
}

#[test]
fn every_curated_row_names_a_shipped_glyph_and_every_shipped_glyph_has_a_row() {
    let files = shipped_files();
    let rows: BTreeSet<&str> = CURATED_ICONS.iter().map(|icon| icon.name).collect();
    let shipped: BTreeSet<&str> = files.keys().map(String::as_str).collect();

    assert_eq!(
        rows.len(),
        CURATED_ICONS.len(),
        "two rows answer to the same name, so one of them can never be chosen"
    );
    assert_eq!(
        rows, shipped,
        "a row without a file cannot be drawn and a file without a row is dead weight in the binary"
    );
}

#[test]
fn every_curated_row_carries_the_bytes_of_the_file_it_is_named_for() {
    let shipped = shipped_files();

    for icon in CURATED_ICONS {
        assert_eq!(
            icon.bytes(),
            shipped
                .get(icon.name)
                .expect("a row names a shipped glyph")
                .as_slice(),
            "the {} row carries another glyph's bytes, so the picker and the page disagree",
            icon.name
        );
    }
}

#[test]
fn every_curated_glyph_is_an_svg_document_carrying_nothing_executable() {
    for icon in CURATED_ICONS {
        let source =
            std::str::from_utf8(icon.bytes()).unwrap_or_else(|e| panic!("{}: {e}", icon.name));

        assert!(
            source.trim_start().starts_with("<svg"),
            "{} is served to a browser as an image resource but is not an svg document",
            icon.name
        );
        for forbidden in ["<script", "javascript:", "<foreignobject", "onload="] {
            assert!(
                !source.to_ascii_lowercase().contains(forbidden),
                "{} carries '{forbidden}', which must never reach a page even as an image",
                icon.name
            );
        }
    }
}

#[test]
fn every_category_is_reachable_and_the_icon_an_untouched_overlay_draws_is_in_the_roster() {
    let used: BTreeSet<&str> = CURATED_ICONS
        .iter()
        .map(|icon| icon.category.as_str())
        .collect();

    assert_eq!(
        used,
        IconCategory::ALL
            .iter()
            .map(|category| category.as_str())
            .collect::<BTreeSet<_>>(),
        "a category the picker offers holds no glyph, so it opens on an empty grid"
    );
    assert!(
        curated_icon(DEFAULT_ICON).is_some(),
        "the icon every untouched overlay falls back to is not in this build's roster"
    );
    assert!(
        curated_icon("no-such-glyph").is_none(),
        "a name outside the roster answered with a glyph"
    );
}

#[test]
fn a_search_reaches_a_glyph_through_any_name_it_carries_whatever_the_case_and_padding() {
    let icon = curated_icon("heart-handshake").expect("the roster carries heart-handshake");

    for query in [
        "",
        "   ",
        "heart",
        "HEART",
        "  Heart  ",
        "heart-hand",
        "heart hand",
        "Heart Handshake",
        "rt handsh",
        "support",
        "charity",
        "CARE",
    ] {
        assert!(
            icon.matches(query),
            "{query:?} did not reach heart-handshake"
        );
    }
    for query in ["rocket", "nature", "zzz", "handshakes", "heart  handshake"] {
        assert!(
            !icon.matches(query),
            "{query:?} reached a glyph it does not describe"
        );
    }
}

#[test]
fn every_curated_row_prints_a_phrase_the_picker_can_put_under_a_card() {
    for icon in CURATED_ICONS {
        let label = icon.label();

        assert!(
            !label.is_empty(),
            "{} prints nothing under its card",
            icon.name
        );
        assert!(
            !label.contains('-'),
            "{} prints the stored spelling {label:?} instead of a phrase",
            icon.name
        );
        assert!(
            !label.contains("  "),
            "{} prints {label:?}, which opens a gap under its card",
            icon.name
        );
        assert_eq!(
            label.chars().next().map(char::is_uppercase),
            Some(true),
            "{} prints {label:?}, which starts in lower case",
            icon.name
        );
    }
}

#[test]
fn a_glyph_is_written_under_a_name_derived_from_its_own_bytes() {
    for icon in CURATED_ICONS {
        let media = glyph_media(&icon_config(icon.name));

        assert!(
            media.issues.is_empty(),
            "{} is in the roster but did not resolve: {:?}",
            icon.name,
            media.issues
        );
        let [resolved] = media.resolved.as_slice() else {
            panic!("{} resolved to {} files", icon.name, media.resolved.len());
        };
        assert_eq!(
            resolved.file_name(),
            format!("{}.{SVG_EXTENSION}", content_identity(icon.bytes())),
            "{} is served under a name a changed glyph would keep, and the browser caches it \
             forever",
            icon.name
        );
        assert_eq!(resolved.bytes(), icon.bytes());
        assert_eq!(resolved.key(), ICON);
    }
}

#[test]
fn a_glyph_name_this_build_does_not_carry_is_named_as_an_issue_and_resolves_to_nothing() {
    for stored in [
        " ",
        "star_filled",
        "STAR-FILLED",
        "star-filled.svg",
        "../star-filled",
        "/etc/passwd",
        &clip_reference(CLIP_ULID),
    ] {
        let media = glyph_media(&icon_config(stored));

        assert_eq!(
            media.issues,
            vec![MediaIssue::UnknownGlyph {
                key: ICON.to_owned(),
                glyph: stored.to_owned(),
            }],
            "{stored:?} was answered with something other than an unknown glyph"
        );
        assert!(
            media.resolved.is_empty(),
            "{stored:?} steered a name into the generated namespace"
        );
    }
}

#[test]
fn an_icon_that_is_empty_or_imported_asks_nothing_of_the_glyph_roster() {
    for stored in ["", &image_reference(BLOB_IDENTITY)] {
        let media = glyph_media(&icon_config(stored));

        assert!(
            media.resolved.is_empty() && media.issues.is_empty(),
            "{stored:?} was read as a curated glyph: {media:?}"
        );
    }
}
