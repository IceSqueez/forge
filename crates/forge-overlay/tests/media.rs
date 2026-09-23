#![allow(clippy::unwrap_used, clippy::expect_used)]

use forge_overlay::config::{DEFAULT_ICON, HEADLINE, ICON, SOUND, SUBLINE};
use forge_overlay::{
    CLIP_REFERENCE_PREFIX, ClipReference, EmittedIcon, GENERATED_MEDIA_DIRECTORY,
    IMAGE_REFERENCE_PREFIX, IconValue, ImageReference, MediaValue, OverlayConfig, OverlayError,
    ResolvedMedia, clip_reference, clip_references, emitted_icon, emitted_media_value,
    image_reference, image_references, read_icon_value, read_media_value,
};
use forge_types::Variant;

const CLIP_ULID: &str = "01j9p4s2m7q8v3x5y6z7a8b9c0";
const BLOB_IDENTITY: &str =
    "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";
const WAV: &str = "wav";
const SVG: &str = "svg";
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

fn resolved_glyph() -> ResolvedMedia {
    ResolvedMedia::new(ICON, BLOB_IDENTITY, SVG, b"<svg/>".to_vec())
        .expect("a content id is a token")
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
        (
            "image:01j9p4s2m7q8v3x5y6z7a8b9c0",
            MediaValue::File("image:01j9p4s2m7q8v3x5y6z7a8b9c0"),
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
fn a_stored_icon_reads_as_nothing_a_curated_glyph_or_an_imported_image() {
    for (stored, expected) in [
        ("", IconValue::Empty),
        (DEFAULT_ICON, IconValue::Glyph(DEFAULT_ICON)),
        (" ", IconValue::Glyph(" ")),
        ("image:", IconValue::Image("")),
        (
            "image:sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8",
            IconValue::Image(BLOB_IDENTITY),
        ),
        (
            "clip:01j9p4s2m7q8v3x5y6z7a8b9c0",
            IconValue::Glyph("clip:01j9p4s2m7q8v3x5y6z7a8b9c0"),
        ),
    ] {
        assert_eq!(
            read_icon_value(stored),
            expected,
            "{stored:?} was read as something else"
        );
    }
}

#[test]
fn a_reference_is_only_read_from_the_key_that_speaks_its_vocabulary() {
    let clip = clip_reference(CLIP_ULID);
    let image = image_reference(BLOB_IDENTITY);
    let stored = config(&[
        (SOUND, clip.as_str()),
        (ICON, image.as_str()),
        (HEADLINE, clip.as_str()),
        (SUBLINE, image.as_str()),
    ]);

    assert_eq!(
        clip_references(&stored),
        vec![ClipReference {
            key: SOUND.to_owned(),
            clip: CLIP_ULID.to_owned(),
        }],
        "wording that opens with {CLIP_REFERENCE_PREFIX:?} must stay wording"
    );
    assert_eq!(
        image_references(&stored),
        vec![ImageReference {
            key: ICON.to_owned(),
            image: BLOB_IDENTITY.to_owned(),
        }],
        "wording that opens with {IMAGE_REFERENCE_PREFIX:?} must stay wording"
    );
}

#[test]
fn a_reference_written_in_the_other_keys_vocabulary_is_never_looked_up() {
    let stored = config(&[
        (SOUND, image_reference(BLOB_IDENTITY).as_str()),
        (ICON, clip_reference(CLIP_ULID).as_str()),
    ]);

    assert!(
        clip_references(&stored).is_empty(),
        "an image reference under the sound key was sent to the clip library"
    );
    assert!(
        image_references(&stored).is_empty(),
        "a clip reference under the icon key was sent to the blob library"
    );
}

#[test]
fn a_non_string_value_under_a_media_key_is_not_a_reference() {
    let stored: OverlayConfig = [
        (SOUND.to_owned(), Variant::Int(7)),
        (ICON.to_owned(), Variant::Bool(true)),
    ]
    .into_iter()
    .collect();

    assert!(
        clip_references(&stored).is_empty(),
        "a number under the sound key must not be read as a clip reference"
    );
    assert!(
        image_references(&stored).is_empty(),
        "a flag under the icon key must not be read as an image reference"
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
        (
            ICON,
            image_reference(BLOB_IDENTITY),
            Some(&media),
            None,
            "the sound emitter never speaks for a key holding another kind of media",
        ),
    ] {
        assert_eq!(
            emitted_media_value(key, &stored, resolved),
            expected,
            "{label}"
        );
    }
}

#[test]
fn an_icon_reaches_the_page_as_a_file_name_and_whether_the_accent_may_fill_it() {
    let media = resolved_glyph();

    for (stored, resolved, expected, label) in [
        (
            DEFAULT_ICON.to_owned(),
            Some(&media),
            EmittedIcon {
                file: media.page_path(),
                tintable: true,
            },
            "a curated glyph is filled with the overlay accent",
        ),
        (
            image_reference(BLOB_IDENTITY),
            Some(&media),
            EmittedIcon {
                file: media.page_path(),
                tintable: false,
            },
            "an imported image is drawn as it was imported",
        ),
        (
            String::new(),
            None,
            EmittedIcon {
                file: String::new(),
                tintable: false,
            },
            "an icon nobody chose names no file",
        ),
        (
            DEFAULT_ICON.to_owned(),
            None,
            EmittedIcon {
                file: String::new(),
                tintable: true,
            },
            "a glyph that did not resolve empties the file instead of leaking the name",
        ),
        (
            image_reference(BLOB_IDENTITY),
            None,
            EmittedIcon {
                file: String::new(),
                tintable: false,
            },
            "an image that did not resolve empties the file instead of leaking the blob id",
        ),
    ] {
        assert_eq!(emitted_icon(&stored, resolved), expected, "{label}");
    }
}
