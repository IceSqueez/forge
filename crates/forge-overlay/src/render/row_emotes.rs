use std::collections::BTreeMap;

use forge_registry::message_emote_ids;
use forge_types::{ArgStack, PlatformId, Variant};

use super::page_image::vetted_image_url;
use super::row_avatar::row_platform;
use crate::catalog::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::config::{
    MESSAGE, MESSAGE_PARTS, THIRD_PARTY_EMOTES, effective_overlay_config, read_str,
};
use crate::kinds::chat::KIND_ID;

const TWITCH_EMOTE_BASE: &str = "https://static-cdn.jtvnw.net/emoticons/v2/";
const TWITCH_EMOTE_STYLE: &str = "static/dark/2.0";
const TWITCH_EMOTE_HOSTS: &[&str] = &["static-cdn.jtvnw.net"];
const THIRD_PARTY_EMOTE_HOSTS: &[&str] =
    &["cdn.7tv.app", "cdn.betterttv.net", "cdn.frankerfacez.com"];
const EMOTE_ID_LENGTH_CAP: usize = 64;
const TEXT_KEY: &str = "text";
const EMOTE_KEY: &str = "emote";
const URL_KEY: &str = "url";

enum Part<'a> {
    Text(&'a str),
    Emote { code: &'a str, url: String },
}

fn twitch_emote_url(id: &str) -> Option<String> {
    let path_safe = !id.is_empty()
        && id.len() <= EMOTE_ID_LENGTH_CAP
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    path_safe
        .then(|| format!("{TWITCH_EMOTE_BASE}{id}/{TWITCH_EMOTE_STYLE}"))
        .and_then(|url| vetted_image_url(&url, TWITCH_EMOTE_HOSTS))
}

fn split_words(message: &str, mut image_of: impl FnMut(&str) -> Option<String>) -> Vec<Part<'_>> {
    let mut parts = Vec::new();
    let mut text_start = 0;
    let mut cursor = 0;
    for word in message.split_whitespace() {
        let Some(offset) = message[cursor..].find(word) else {
            break;
        };
        let start = cursor + offset;
        cursor = start + word.len();
        if let Some(url) = image_of(word) {
            if text_start < start {
                parts.push(Part::Text(&message[text_start..start]));
            }
            parts.push(Part::Emote { code: word, url });
            text_start = cursor;
        }
    }
    if text_start < message.len() {
        parts.push(Part::Text(&message[text_start..]));
    }
    parts
}

fn part_variant(part: Part<'_>) -> Variant {
    let fields = match part {
        Part::Text(text) => {
            BTreeMap::from([(TEXT_KEY.to_owned(), Variant::String(text.to_owned()))])
        }
        Part::Emote { code, url } => BTreeMap::from([
            (EMOTE_KEY.to_owned(), Variant::String(code.to_owned())),
            (URL_KEY.to_owned(), Variant::String(url)),
        ]),
    };
    Variant::Object(fields)
}

pub fn settle_message_parts(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    content: &mut OverlayConfig,
    args: &ArgStack,
    third_party: &dyn Fn(&str) -> Option<String>,
) {
    if descriptor.id() != KIND_ID || row_platform(content) != Some(PlatformId::Twitch) {
        return;
    }
    let third_party_shown = effective_overlay_config(descriptor, stored).get(THIRD_PARTY_EMOTES)
        == Some(&Variant::Bool(true));
    let native = message_emote_ids(args);
    let message = read_str(content, MESSAGE).to_owned();
    let parts = split_words(&message, |word| {
        native
            .get(word)
            .and_then(|id| twitch_emote_url(id))
            .or_else(|| {
                third_party_shown
                    .then(|| third_party(word))
                    .flatten()
                    .and_then(|url| vetted_image_url(&url, THIRD_PARTY_EMOTE_HOSTS))
            })
    });
    if parts.iter().any(|part| matches!(part, Part::Emote { .. })) {
        content.insert(
            MESSAGE_PARTS.to_owned(),
            Variant::Array(parts.into_iter().map(part_variant).collect()),
        );
    }
}
