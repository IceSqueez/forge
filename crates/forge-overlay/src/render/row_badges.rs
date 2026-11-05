use std::collections::BTreeMap;

use forge_registry::message_badge_images;
use forge_types::{ArgStack, BadgeImage, PlatformId, Variant};

use super::page_image::vetted_image_url;
use super::row_avatar::row_platform;
use crate::catalog::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::config::{BADGE_IMAGES, BADGES, read_str};
use crate::kinds::chat::KIND_ID;

const TWITCH_BADGE_HOSTS: &[&str] = &["static-cdn.jtvnw.net"];
const TITLE_KEY: &str = "title";
const URL_KEY: &str = "url";

fn image_variant(image: BadgeImage) -> Option<Variant> {
    let url = vetted_image_url(&image.url, TWITCH_BADGE_HOSTS)?;
    Some(Variant::Object(BTreeMap::from([
        (TITLE_KEY.to_owned(), Variant::String(image.title)),
        (URL_KEY.to_owned(), Variant::String(url)),
    ])))
}

pub fn settle_badge_images(
    descriptor: &dyn OverlayKindDescriptor,
    content: &mut OverlayConfig,
    args: &ArgStack,
) {
    if descriptor.id() != KIND_ID
        || row_platform(content) != Some(PlatformId::Twitch)
        || read_str(content, BADGES).trim().is_empty()
    {
        return;
    }
    let images: Vec<Variant> = message_badge_images(args)
        .into_iter()
        .filter_map(image_variant)
        .collect();
    if !images.is_empty() {
        content.insert(BADGE_IMAGES.to_owned(), Variant::Array(images));
    }
}
