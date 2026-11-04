#![expect(clippy::expect_used, reason = "test fixtures assert directly")]
mod common;
use common::{ALERT_KIND, BLOB_IDENTITY, CHAT_KIND, registry, sub_alert_instance};

use forge_overlay::config::{
    BACKGROUND_COLOR, BACKGROUND_OPACITY, KICK_ICON, PLATFORM, PLATFORM_ICON_KEYS, ROW_WIDTH,
    ROW_WIDTH_FIT, ROW_WIDTH_FULL, TWITCH_ICON, YOUTUBE_ICON,
};
use forge_overlay::metrics::{CHAT, SURFACE_RGB};
use forge_overlay::{
    GENERATED_MEDIA_DIRECTORY, OverlayConfig, OverlayError, OverlayMedia, PreviewComposition,
    PreviewSurface, ResolvedMedia, config_document, effective_overlay_config, hex_color_channels,
    image_reference, validate_overlay_config,
};
use forge_types::{NamesLanguage, PlatformId, Variant};
use serde_json::Value;

const PNG: &str = "png";

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn stored(entries: &[(&str, &str)]) -> OverlayConfig {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), text(value)))
        .collect()
}

fn composition(kind_id: &str, config: &OverlayConfig) -> PreviewComposition {
    let reg = registry();
    let descriptor = reg.get(kind_id).expect("the kind ships in this build");
    descriptor.preview(
        &effective_overlay_config(descriptor, config),
        NamesLanguage::En,
    )
}

fn chat_page_config(config: OverlayConfig, media: OverlayMedia) -> Value {
    let reg = registry();
    let descriptor = reg.get(CHAT_KIND).expect("the chat kind ships");
    let mut instance = sub_alert_instance(CHAT_KIND);
    instance.config = config;
    instance.media = media;
    let document = config_document(&instance, descriptor).expect("the config document builds");
    let parsed: Value = serde_json::from_str(&document).expect("the document is valid JSON");
    parsed["config"].clone()
}

fn chat_assets() -> (&'static str, &'static str) {
    let reg = registry();
    let assets = reg
        .get(CHAT_KIND)
        .expect("the chat kind ships")
        .page_assets();
    (assets.behavior, assets.style)
}

#[test]
fn an_untouched_chat_hands_its_page_the_row_surface_the_stylesheet_always_drew() {
    let page = chat_page_config(OverlayConfig::new(), OverlayMedia::default());
    let [red, green, blue] = hex_color_channels(
        page[BACKGROUND_COLOR]
            .as_str()
            .expect("the page carries a row color"),
    )
    .expect("the default row color is a hex color");
    let opacity = page[BACKGROUND_OPACITY]
        .as_i64()
        .expect("the page carries a row opacity");
    let painted = format!(
        "background: rgba({red}, {green}, {blue}, {});",
        opacity as f64 / 100.0
    );

    assert!(
        chat_assets().1.contains(&painted),
        "an untouched chat would paint '{painted}', which its stylesheet never drew"
    );
}

#[test]
fn an_untouched_chat_keeps_full_width_rows() {
    let page = chat_page_config(OverlayConfig::new(), OverlayMedia::default());

    assert_eq!(page[ROW_WIDTH], ROW_WIDTH_FULL);
    assert!(!composition(CHAT_KIND, &OverlayConfig::new()).fit_rows);
}

#[test]
fn the_chat_preview_paints_the_row_surface_the_page_draws() {
    let mut chosen = stored(&[(BACKGROUND_COLOR, "#123456")]);
    chosen.insert(BACKGROUND_OPACITY.to_owned(), Variant::Int(40));

    for (config, expected) in [
        (
            OverlayConfig::new(),
            PreviewSurface {
                rgb: SURFACE_RGB,
                alpha: CHAT.surface_alpha,
            },
        ),
        (
            chosen,
            PreviewSurface {
                rgb: [18, 52, 86],
                alpha: 0.4,
            },
        ),
    ] {
        assert_eq!(composition(CHAT_KIND, &config).surface, Some(expected));
    }
}

#[test]
fn only_a_chat_set_to_fit_shrinks_its_rows_in_the_preview() {
    for (kind_id, width, expected) in [
        (CHAT_KIND, ROW_WIDTH_FIT, true),
        (CHAT_KIND, ROW_WIDTH_FULL, false),
        (CHAT_KIND, "a width a newer build offers", false),
        (ALERT_KIND, ROW_WIDTH_FIT, false),
    ] {
        assert_eq!(
            composition(kind_id, &stored(&[(ROW_WIDTH, width)])).fit_rows,
            expected,
            "{kind_id} with row width {width:?}"
        );
    }
}

#[test]
fn the_chat_preview_shows_the_icon_of_the_sample_platform_or_else_the_first_one_chosen() {
    let picked = image_reference(BLOB_IDENTITY);
    for (platform, chosen, expected) in [
        (
            "twitch",
            &[TWITCH_ICON, YOUTUBE_ICON][..],
            Some(TWITCH_ICON),
        ),
        ("Kick", &[TWITCH_ICON, KICK_ICON][..], Some(KICK_ICON)),
        ("youtube", &[TWITCH_ICON][..], Some(TWITCH_ICON)),
        ("", &[YOUTUBE_ICON, KICK_ICON][..], Some(YOUTUBE_ICON)),
        ("twitch", &[][..], None),
    ] {
        let mut config = stored(&[(PLATFORM, platform)]);
        for key in chosen {
            config.insert((*key).to_owned(), text(&picked));
        }

        let preview = composition(CHAT_KIND, &config);

        assert_eq!(
            preview.platform_icon, expected,
            "{platform:?} with {chosen:?} chosen"
        );
        assert_eq!(
            preview.picture_key(),
            expected,
            "the preview loads a different picture than the icon it draws"
        );
    }
}

#[test]
fn a_platform_icon_takes_only_a_library_image_or_nothing() {
    let reg = registry();
    let descriptor = reg.get(CHAT_KIND).expect("the chat kind ships");
    for key in PLATFORM_ICON_KEYS {
        for (value, accepted) in [
            ("", true),
            (&image_reference(BLOB_IDENTITY)[..], true),
            ("heart", false),
            ("../../etc/passwd", false),
            ("image:../../etc/passwd", false),
            ("clip:01j9p4s2m7q8v3x5y6z7a8b9c0", false),
        ] {
            let verdict = validate_overlay_config(descriptor, &stored(&[(key, value)]));

            assert_eq!(verdict.is_ok(), accepted, "{key} = {value:?}");
            if !accepted {
                assert!(
                    matches!(&verdict, Err(OverlayError::UnknownChoice { key: refused, .. }) if refused == key),
                    "{key} = {value:?} produced {verdict:?}"
                );
            }
        }
    }
}

#[test]
fn the_chat_page_gets_each_platform_icon_as_its_generated_file_and_never_the_reference() {
    for key in PLATFORM_ICON_KEYS {
        let config = stored(&[(key, &image_reference(BLOB_IDENTITY))]);
        let resolved = ResolvedMedia::new(key, BLOB_IDENTITY, PNG, b"\x89PNG".to_vec())
            .expect("a content id is a token");

        let served = chat_page_config(
            config.clone(),
            OverlayMedia {
                resolved: vec![resolved],
                issues: Vec::new(),
            },
        );
        let unresolved = chat_page_config(config, OverlayMedia::default());

        assert_eq!(
            served[key],
            format!("{GENERATED_MEDIA_DIRECTORY}/{BLOB_IDENTITY}.{PNG}"),
            "{key}"
        );
        assert_eq!(unresolved[key], "", "an unresolved {key} must draw nothing");
    }
}

#[test]
fn the_chat_page_looks_up_each_platform_icon_under_the_key_the_config_writes() {
    let script = chat_assets().0;
    let mapped = common::js_block(script, "var PLATFORM_ICON_KEYS = {");

    for (platform, key) in PlatformId::ALL.into_iter().zip(PLATFORM_ICON_KEYS) {
        assert!(
            mapped.contains(&format!("{}: \"{key}\"", platform.as_str())),
            "a {platform:?} row would never find the icon stored under {key}"
        );
    }
}

#[test]
fn the_chat_page_shrinks_rows_on_the_width_value_the_config_writes() {
    let (script, style) = chat_assets();

    assert!(script.contains(&format!("var ROW_WIDTH_FIT = \"{ROW_WIDTH_FIT}\";")));
    assert!(script.contains(&format!("config.{ROW_WIDTH} === ROW_WIDTH_FIT")));
    assert!(script.contains("var FIT_ROWS_CLASS = \"fit-rows\";"));
    assert!(style.contains("#rows.fit-rows .row {"));
}
