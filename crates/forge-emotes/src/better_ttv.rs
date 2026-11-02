use serde_json::Value;

use crate::provider::{ProviderEmote, emote, is_path_safe_id};

pub(crate) const GLOBAL_PATH: &str = "/cached/emotes/global";
const CHANNEL_PATH_PREFIX: &str = "/cached/users/twitch/";
const IMAGE_BASE: &str = "https://cdn.betterttv.net/emote/";
const IMAGE_SCALE: &str = "2x";
const CHANNEL_LISTS: [&str; 2] = ["channelEmotes", "sharedEmotes"];

pub(crate) fn channel_path(broadcaster_id: &str) -> String {
    format!("{CHANNEL_PATH_PREFIX}{broadcaster_id}")
}

pub(crate) fn global_emotes(answer: &Value) -> Vec<ProviderEmote> {
    list_emotes(answer)
}

pub(crate) fn channel_emotes(answer: &Value) -> Vec<ProviderEmote> {
    CHANNEL_LISTS
        .iter()
        .flat_map(|list| list_emotes(&answer[*list]))
        .collect()
}

fn list_emotes(list: &Value) -> Vec<ProviderEmote> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry["id"].as_str().filter(|id| is_path_safe_id(id))?;
            emote(
                entry["code"].as_str()?,
                format!("{IMAGE_BASE}{id}/{IMAGE_SCALE}"),
            )
        })
        .collect()
}
