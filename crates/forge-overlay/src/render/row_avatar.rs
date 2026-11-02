use forge_types::{PlatformId, Variant};
use serde_json::{Value, json};

use super::page_contract::AVATAR_FRAME;
use super::page_image::vetted_image_url;
use crate::catalog::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::config::{
    AVATAR, PLATFORM, SHOW_AVATARS, USER_ID, effective_overlay_config, read_str, text,
};
use crate::kinds::chat::KIND_ID;

const TWITCH_IMAGE_HOSTS: &[&str] = &["static-cdn.jtvnw.net"];
const YOUTUBE_IMAGE_HOSTS: &[&str] = &["ggpht.com", "googleusercontent.com"];
const KICK_IMAGE_HOSTS: &[&str] = &["kick.com"];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChatAvatarSlot {
    pub platform: PlatformId,
    pub user_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowAvatar {
    pub slot: ChatAvatarSlot,
    pub url: String,
}

impl RowAvatar {
    pub fn frame(&self) -> Value {
        json!({
            "frame": AVATAR_FRAME,
            "platform": self.slot.platform.as_str(),
            "userId": self.slot.user_id,
            "url": self.url,
        })
    }

    pub fn fill(&self, content: &mut OverlayConfig) {
        content.insert(AVATAR.to_owned(), text(&self.url));
    }
}

fn image_hosts(platform: PlatformId) -> &'static [&'static str] {
    match platform {
        PlatformId::Twitch => TWITCH_IMAGE_HOSTS,
        PlatformId::YouTube => YOUTUBE_IMAGE_HOSTS,
        PlatformId::Kick => KICK_IMAGE_HOSTS,
    }
}

pub fn page_avatar_url(platform: PlatformId, raw: &str) -> Option<String> {
    vetted_image_url(raw, image_hosts(platform))
}

pub fn settle_chat_avatar(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    content: &mut OverlayConfig,
) -> Option<ChatAvatarSlot> {
    if descriptor.id() != KIND_ID {
        return None;
    }
    let platform = PlatformId::from_wire(read_str(content, PLATFORM).trim());
    let given = read_str(content, AVATAR).trim().to_owned();
    if !given.is_empty() {
        match platform.and_then(|platform| page_avatar_url(platform, &given)) {
            Some(url) => {
                content.insert(AVATAR.to_owned(), text(&url));
                return None;
            }
            None => {
                content.insert(AVATAR.to_owned(), text(""));
            }
        }
    }
    let shown = effective_overlay_config(descriptor, stored).get(SHOW_AVATARS)
        == Some(&Variant::Bool(true));
    let user_id = read_str(content, USER_ID).trim();
    if !shown || user_id.is_empty() {
        return None;
    }
    Some(ChatAvatarSlot {
        platform: platform?,
        user_id: user_id.to_owned(),
    })
}
