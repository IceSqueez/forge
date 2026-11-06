mod common;
use common::{ALERT_KIND, CHAT_KIND, text, with_kind};

use std::collections::BTreeMap;

use forge_overlay::config::{BADGE_IMAGES, BADGES, PLATFORM};
use forge_overlay::{OverlayConfig, settle_badge_images};
use forge_registry::USER_BADGE_IMAGES_VARIABLE;
use forge_types::{ArgStack, BadgeImage, Variant};

const BROADCASTER_IMAGE: &str = "https://static-cdn.jtvnw.net/badges/v1/5527c58c/2";
const SUBSCRIBER_IMAGE: &str = "https://static-cdn.jtvnw.net/badges/v1/5d9f2208/2";

fn picture(title: &str, url: &str) -> BadgeImage {
    BadgeImage {
        title: title.to_owned(),
        url: url.to_owned(),
    }
}

fn args_with(pictures: &[BadgeImage]) -> ArgStack {
    ArgStack::new().set(
        USER_BADGE_IMAGES_VARIABLE.to_owned(),
        Variant::String(serde_json::to_string(pictures).unwrap_or_default()),
    )
}

fn row(platform: &str, badges: &str) -> OverlayConfig {
    OverlayConfig::from([
        (PLATFORM.to_owned(), text(platform)),
        (BADGES.to_owned(), text(badges)),
    ])
}

fn settled(kind_id: &str, mut content: OverlayConfig, args: &ArgStack) -> OverlayConfig {
    with_kind(kind_id, |descriptor| {
        settle_badge_images(descriptor, &mut content, args)
    });
    content
}

fn shown(title: &str, url: &str) -> Variant {
    Variant::Object(BTreeMap::from([
        ("title".to_owned(), text(title)),
        ("url".to_owned(), text(url)),
    ]))
}

#[test]
fn a_twitch_chat_row_carries_its_badge_pictures_in_the_order_twitch_sent_them() {
    let args = args_with(&[
        picture("Broadcaster", BROADCASTER_IMAGE),
        picture("Subscriber", SUBSCRIBER_IMAGE),
    ]);

    let content = settled(CHAT_KIND, row("twitch", "BROADCASTER SUB"), &args);

    assert_eq!(
        content.get(BADGE_IMAGES),
        Some(&Variant::Array(vec![
            shown("Broadcaster", BROADCASTER_IMAGE),
            shown("Subscriber", SUBSCRIBER_IMAGE),
        ]))
    );
}

#[test]
fn a_picture_off_the_twitch_badge_host_is_dropped_and_the_rest_are_kept() {
    let args = args_with(&[
        picture("Evil", "https://evil.example/badge.png"),
        picture("Plain", "http://static-cdn.jtvnw.net/badges/v1/a/2"),
        picture("Quote", "https://static-cdn.jtvnw.net/a\"onerror=b"),
        picture("Subscriber", SUBSCRIBER_IMAGE),
    ]);

    let content = settled(CHAT_KIND, row("twitch", "SUB"), &args);

    assert_eq!(
        content.get(BADGE_IMAGES),
        Some(&Variant::Array(vec![shown("Subscriber", SUBSCRIBER_IMAGE)]))
    );
}

#[test]
fn no_picture_list_is_written_when_none_survive_or_none_apply() {
    let pictures = args_with(&[picture("Subscriber", SUBSCRIBER_IMAGE)]);
    let refused = args_with(&[picture("Evil", "https://evil.example/badge.png")]);
    for (case, kind, content, args) in [
        (
            "every picture refused",
            CHAT_KIND,
            row("twitch", "SUB"),
            &refused,
        ),
        (
            "no pictures sent",
            CHAT_KIND,
            row("twitch", "SUB"),
            &ArgStack::new(),
        ),
        (
            "chatter without badges",
            CHAT_KIND,
            row("twitch", "  "),
            &pictures,
        ),
        (
            "youtube row",
            CHAT_KIND,
            row("youtube", "MEMBER"),
            &pictures,
        ),
        ("kick row", CHAT_KIND, row("kick", "SUB"), &pictures),
        ("unknown platform", CHAT_KIND, row("", "SUB"), &pictures),
        (
            "not a chat overlay",
            ALERT_KIND,
            row("twitch", "SUB"),
            &pictures,
        ),
    ] {
        let before = content.clone();
        assert_eq!(settled(kind, content, args), before, "{case}");
    }
}
