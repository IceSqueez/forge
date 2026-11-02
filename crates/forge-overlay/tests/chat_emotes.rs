mod common;
use common::{ALERT_KIND, CHAT_KIND, text, with_kind};

use std::collections::BTreeSet;

use std::collections::BTreeMap;

use forge_overlay::config::{MESSAGE, MESSAGE_PARTS, PLATFORM, THIRD_PARTY_EMOTES};
use forge_overlay::{OverlayConfig, settle_message_parts};
use forge_types::{ArgStack, Variant};

const NATIVE_IDS_VARIABLE: &str = "message_emote_ids";
const LUL_ID: &str = "425618";
const LUL_IMAGE: &str = "https://static-cdn.jtvnw.net/emoticons/v2/425618/static/dark/2.0";
const KEKW_IMAGE: &str = "https://cdn.7tv.app/emote/01GB4CK01800090V9B3D8CGEEX/2x.webp";
const LUL_LOOKALIKE_IMAGE: &str = "https://cdn.betterttv.net/emote/5e4e7a1f08b4447d56a92967/2x";

#[derive(Debug, PartialEq, Eq)]
enum Part {
    Text(String),
    Emote(String, String),
}

fn text_part(value: &str) -> Part {
    Part::Text(value.to_owned())
}

fn emote_part(code: &str, url: &str) -> Part {
    Part::Emote(code.to_owned(), url.to_owned())
}

fn row(platform: &str, message: &str) -> OverlayConfig {
    OverlayConfig::from([
        (PLATFORM.to_owned(), text(platform)),
        (MESSAGE.to_owned(), text(message)),
    ])
}

fn native(ids: &[(&str, &str)]) -> ArgStack {
    let ids = ids
        .iter()
        .map(|(code, id)| ((*code).to_owned(), text(id)))
        .collect::<BTreeMap<_, _>>();
    ArgStack::new().set(NATIVE_IDS_VARIABLE.to_owned(), Variant::Object(ids))
}

fn third_party_off() -> OverlayConfig {
    OverlayConfig::from([(THIRD_PARTY_EMOTES.to_owned(), Variant::Bool(false))])
}

fn catalog(code: &str) -> Option<String> {
    match code {
        "KEKW" => Some(KEKW_IMAGE.to_owned()),
        "LUL" => Some(LUL_LOOKALIKE_IMAGE.to_owned()),
        _ => None,
    }
}

fn settle_with(
    kind_id: &str,
    stored: &OverlayConfig,
    content: &mut OverlayConfig,
    args: &ArgStack,
    third_party: &dyn Fn(&str) -> Option<String>,
) {
    with_kind(kind_id, |descriptor| {
        settle_message_parts(descriptor, stored, content, args, third_party);
    });
}

fn parts_of(
    platform: &str,
    message: &str,
    args: &ArgStack,
    stored: &OverlayConfig,
) -> Option<Vec<Part>> {
    let mut content = row(platform, message);
    settle_with(CHAT_KIND, stored, &mut content, args, &catalog);
    decoded_parts(&content)
}

fn twitch_parts(message: &str, args: &ArgStack) -> Option<Vec<Part>> {
    parts_of("twitch", message, args, &OverlayConfig::new())
}

fn field<'a>(fields: &'a BTreeMap<String, Variant>, key: &str) -> Option<&'a str> {
    fields.get(key).and_then(Variant::as_str)
}

fn decoded_parts(content: &OverlayConfig) -> Option<Vec<Part>> {
    let parts = content.get(MESSAGE_PARTS)?.as_array()?;
    Some(
        parts
            .iter()
            .map(|part| {
                let fields = part.as_object().cloned().unwrap_or_default();
                match (field(&fields, "emote"), field(&fields, "url")) {
                    (Some(code), Some(url)) => emote_part(code, url),
                    _ => text_part(field(&fields, "text").unwrap_or("<no text field>")),
                }
            })
            .collect(),
    )
}

#[test]
fn a_native_emote_becomes_an_image_part_between_the_text_around_it() {
    let parts = twitch_parts("so funny LUL right", &native(&[("LUL", LUL_ID)]));

    assert_eq!(
        parts,
        Some(vec![
            text_part("so funny "),
            emote_part("LUL", LUL_IMAGE),
            text_part(" right"),
        ])
    );
}

#[test]
fn a_native_emote_wins_over_a_third_party_emote_with_the_same_code() {
    let parts = twitch_parts("LUL", &native(&[("LUL", LUL_ID)]));

    assert_eq!(parts, Some(vec![emote_part("LUL", LUL_IMAGE)]));
}

#[test]
fn a_third_party_emote_fills_a_word_twitch_did_not_mark() {
    let parts = twitch_parts("KEKW", &ArgStack::new());

    assert_eq!(parts, Some(vec![emote_part("KEKW", KEKW_IMAGE)]));
}

#[test]
fn only_a_whole_whitespace_separated_word_becomes_an_emote() {
    let parts = twitch_parts("KEKW, (KEKW) KEKWait xKEKW KEKW!? KEKW", &ArgStack::new());

    assert_eq!(
        parts,
        Some(vec![
            text_part("KEKW, (KEKW) KEKWait xKEKW KEKW!? "),
            emote_part("KEKW", KEKW_IMAGE),
        ])
    );
}

#[test]
fn repeated_emotes_and_the_surrounding_spacing_survive_verbatim() {
    let message = "  KEKW KEKW\tпривіт  KEKW  ";

    let parts = twitch_parts(message, &ArgStack::new());

    assert_eq!(
        parts,
        Some(vec![
            text_part("  "),
            emote_part("KEKW", KEKW_IMAGE),
            text_part(" "),
            emote_part("KEKW", KEKW_IMAGE),
            text_part("\tпривіт  "),
            emote_part("KEKW", KEKW_IMAGE),
            text_part("  "),
        ])
    );
}

#[test]
fn a_message_without_any_emote_gets_no_parts() {
    for message in ["just words here", "", "   "] {
        assert_eq!(
            twitch_parts(message, &native(&[("LUL", LUL_ID)])),
            None,
            "{message:?}"
        );
    }
}

#[test]
fn a_row_from_another_platform_gets_no_parts() {
    for platform in ["kick", "youtube", "", "trovo"] {
        assert_eq!(
            parts_of(
                platform,
                "KEKW LUL",
                &native(&[("LUL", LUL_ID)]),
                &OverlayConfig::new()
            ),
            None,
            "{platform:?}"
        );
    }
}

#[test]
fn a_non_chat_overlay_is_left_untouched() {
    let mut content = row("twitch", "KEKW LUL");
    let before = content.clone();

    settle_with(
        ALERT_KIND,
        &OverlayConfig::new(),
        &mut content,
        &native(&[("LUL", LUL_ID)]),
        &catalog,
    );

    assert_eq!(content, before);
}

#[test]
fn switching_third_party_emotes_off_keeps_only_the_native_ones() {
    let parts = parts_of(
        "twitch",
        "LUL KEKW",
        &native(&[("LUL", LUL_ID)]),
        &third_party_off(),
    );

    assert_eq!(
        parts,
        Some(vec![emote_part("LUL", LUL_IMAGE), text_part(" KEKW")])
    );
}

#[test]
fn switching_third_party_emotes_off_never_consults_the_catalog() {
    let mut content = row("twitch", "KEKW");
    let consulted = std::cell::Cell::new(false);
    let spy = |_: &str| {
        consulted.set(true);
        Some(KEKW_IMAGE.to_owned())
    };

    settle_with(
        CHAT_KIND,
        &third_party_off(),
        &mut content,
        &ArgStack::new(),
        &spy,
    );

    assert!(!consulted.get());
    assert_eq!(decoded_parts(&content), None);
}

#[test]
fn a_third_party_image_is_shown_only_from_an_emote_cdn_over_https() {
    for (url, shown) in [
        (
            "https://cdn.7tv.app/emote/01GB4CK01800090V9B3D8CGEEX/2x.webp",
            true,
        ),
        (
            "https://cdn.betterttv.net/emote/5e4e7a1f08b4447d56a92967/2x",
            true,
        ),
        ("https://cdn.frankerfacez.com/emote/381875/2", true),
        ("http://cdn.7tv.app/emote/a/2x.webp", false),
        ("//cdn.7tv.app/emote/a/2x.webp", false),
        ("https://cdn.7tv.app.evil.com/emote/a/2x.webp", false),
        ("https://evilcdn.7tv.app.com/a.webp", false),
        ("https://cdn.7tv.app@evil.com/a.webp", false),
        ("https://cdn.7tv.app:8443/a.webp", false),
        ("https://7tv.app/emote/a/2x.webp", false),
        (
            "https://static-cdn.jtvnw.net/emoticons/v2/1/static/dark/2.0",
            false,
        ),
        ("https://cdn.7tv.app/a\"onerror=\"x.webp", false),
        ("javascript:alert(1)", false),
    ] {
        let mut content = row("twitch", "KEKW");
        let image = |_: &str| Some(url.to_owned());

        settle_with(
            CHAT_KIND,
            &OverlayConfig::new(),
            &mut content,
            &ArgStack::new(),
            &image,
        );

        let expected = shown.then(|| vec![emote_part("KEKW", url)]);
        assert_eq!(decoded_parts(&content), expected, "{url}");
    }
}

#[test]
fn a_native_emote_id_outside_the_safe_pattern_is_not_turned_into_an_image() {
    for id in [
        "",
        "../1",
        "1/../2",
        "a b",
        "emotesv2-abc",
        "1?x=2",
        "1#x",
        "\u{0}",
        "ü",
        &"9".repeat(65),
    ] {
        assert_eq!(
            twitch_parts("Kappa", &native(&[("Kappa", id)])),
            None,
            "{id:?}"
        );
    }
}

#[test]
fn a_native_emote_id_is_accepted_up_to_the_length_cap() {
    for id in ["emotesv2_dcd06b30a5c24f6eb871e8f5edbd44f7", &"9".repeat(64)] {
        let url = format!("https://static-cdn.jtvnw.net/emoticons/v2/{id}/static/dark/2.0");

        assert_eq!(
            twitch_parts("Kappa", &native(&[("Kappa", id)])),
            Some(vec![emote_part("Kappa", &url)]),
            "{id}"
        );
    }
}

#[test]
fn a_native_emote_with_an_unsafe_id_falls_back_to_the_third_party_image() {
    let parts = twitch_parts("LUL", &native(&[("LUL", "../1")]));

    assert_eq!(parts, Some(vec![emote_part("LUL", LUL_LOOKALIKE_IMAGE)]));
}

#[test]
fn the_chat_page_reads_the_settled_parts_under_the_keys_the_runtime_writes() {
    let mut content = row("twitch", "so funny LUL");
    settle_with(
        CHAT_KIND,
        &OverlayConfig::new(),
        &mut content,
        &native(&[("LUL", LUL_ID)]),
        &catalog,
    );
    let part_keys: BTreeSet<String> = content
        .get(MESSAGE_PARTS)
        .and_then(Variant::as_array)
        .into_iter()
        .flatten()
        .filter_map(Variant::as_object)
        .flat_map(|fields| fields.keys().cloned())
        .collect();
    let page = with_kind(CHAT_KIND, |chat| chat.page_assets().behavior.to_owned());

    assert!(page.contains(&format!("values.{MESSAGE_PARTS}")));
    assert_eq!(part_keys.len(), 3, "{part_keys:?}");
    for key in part_keys {
        assert!(
            page.contains(&format!("part.{key}")),
            "the chat page never reads the '{key}' field of a message part"
        );
    }
}
