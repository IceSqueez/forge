#![allow(clippy::unwrap_used, clippy::expect_used)]

use forge_overlay::config::{HEADLINE, SOUND, SUBLINE};
use forge_overlay::{
    CLIP_REFERENCE_PREFIX, ClipReference, GENERATED_MEDIA_DIRECTORY, MediaValue, OverlayConfig,
    OverlayError, ResolvedMedia, clip_reference, clip_references, emitted_media_value,
    read_media_value,
};
use forge_types::Variant;

const CLIP_ULID: &str = "01j9p4s2m7q8v3x5y6z7a8b9c0";
const BLOB_IDENTITY: &str =
    "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";
const WAV: &str = "wav";
const LEGACY_FILE: &str = "fanfare.mp3";

fn config(pairs: &[(&str, &str)]) -> OverlayConfig {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
        .collect()
}

fn resolved(key: &str) -> ResolvedMedia {
    ResolvedMedia::new(key, BLOB_IDENTITY, WAV, b"RIFF".to_vec()).expect("a content id is a token")
}

#[test]
fn a_stored_sound_reads_as_nothing_a_library_reference_or_a_legacy_filename() {
    for (stored, expected) in [
        ("", MediaValue::Empty),
        (LEGACY_FILE, MediaValue::File(LEGACY_FILE)),
        ("forge-media/x.wav", MediaValue::File("forge-media/x.wav")),
        ("clip:", MediaValue::Clip("")),
        (
            "clip:01j9p4s2m7q8v3x5y6z7a8b9c0",
            MediaValue::Clip(CLIP_ULID),
        ),
    ] {
        assert_eq!(
            read_media_value(stored),
            expected,
            "{stored:?} was read as something else"
        );
    }
}

#[test]
fn a_reference_is_only_read_from_a_key_that_holds_media() {
    let reference = clip_reference(CLIP_ULID);
    let stored = config(&[
        (SOUND, reference.as_str()),
        (HEADLINE, reference.as_str()),
        (SUBLINE, "clip: joins us tonight"),
    ]);

    assert_eq!(
        clip_references(&stored),
        vec![ClipReference {
            key: SOUND.to_owned(),
            clip: CLIP_ULID.to_owned(),
        }],
        "wording that opens with {CLIP_REFERENCE_PREFIX:?} must stay wording"
    );
}

#[test]
fn a_non_string_value_under_a_media_key_is_not_a_reference() {
    let stored: OverlayConfig = [(SOUND.to_owned(), Variant::Int(7))].into_iter().collect();

    assert!(
        clip_references(&stored).is_empty(),
        "a number under the sound key must not be read as a clip reference"
    );
}

#[test]
fn a_generated_file_name_is_refused_unless_both_halves_are_lowercase_ascii_tokens() {
    for (identity, extension, label) in [
        ("SHA256-AB", WAV, "an uppercase identity"),
        ("sha256_ab", WAV, "an underscore"),
        ("sha256 ab", WAV, "a space"),
        ("sha256-ab.wav", WAV, "a dot inside the identity"),
        ("..", WAV, "parent traversal"),
        (".", WAV, "the current directory"),
        ("a/b", WAV, "a unix separator"),
        ("a\\b", WAV, "a windows separator"),
        ("/abs", WAV, "an absolute path"),
        ("sha256-\u{0430}b", WAV, "a non-ascii identity"),
        ("", WAV, "an empty identity"),
        (BLOB_IDENTITY, "WAV", "an uppercase extension"),
        (BLOB_IDENTITY, "wa-v", "a hyphen in the extension"),
        (BLOB_IDENTITY, "w.av", "a dot in the extension"),
        (BLOB_IDENTITY, "../wav", "an escaping extension"),
        (BLOB_IDENTITY, "", "an empty extension"),
    ] {
        let refused = ResolvedMedia::new(SOUND, identity, extension, Vec::new())
            .expect_err("a name that reaches a path must be a token");

        assert!(
            matches!(&refused, OverlayError::UnsafeIdentity(got) if got == &format!("{identity}.{extension}")),
            "{label} ({identity:?}.{extension:?}) produced {refused:?}"
        );
    }
}

#[test]
fn a_resolved_file_is_named_for_its_content_and_lives_under_the_generated_namespace() {
    let media = resolved(SOUND);

    assert_eq!(media.file_name(), format!("{BLOB_IDENTITY}.{WAV}"));
    assert_eq!(
        media.page_path(),
        format!("{GENERATED_MEDIA_DIRECTORY}/{BLOB_IDENTITY}.{WAV}")
    );
}

#[test]
fn only_a_media_key_holding_a_reference_has_its_page_value_rewritten() {
    let media = resolved(SOUND);

    for (key, stored, resolved, expected, label) in [
        (
            SOUND,
            clip_reference(CLIP_ULID),
            Some(&media),
            Some(media.page_path()),
            "a resolved reference becomes the generated path",
        ),
        (
            SOUND,
            clip_reference(CLIP_ULID),
            None,
            Some(String::new()),
            "an unresolved reference empties the value instead of leaking clip:",
        ),
        (
            SOUND,
            LEGACY_FILE.to_owned(),
            None,
            None,
            "a hand-placed filename is left alone",
        ),
        (
            SOUND,
            String::new(),
            None,
            None,
            "an empty sound is left alone",
        ),
        (
            HEADLINE,
            clip_reference(CLIP_ULID),
            Some(&media),
            None,
            "a non-media key is never rewritten",
        ),
    ] {
        assert_eq!(
            emitted_media_value(key, &stored, resolved),
            expected,
            "{label}"
        );
    }
}
