use serde_json::Value;

use crate::provider::{ProviderEmote, absolute_https, emote};

pub(crate) const GLOBAL_PATH: &str = "/emote-sets/global";
const CHANNEL_PATH_PREFIX: &str = "/users/twitch/";
const PREFERRED_FILE: &str = "2x.webp";
const ANIMATABLE_EXTENSION: &str = ".webp";

pub(crate) fn channel_path(broadcaster_id: &str) -> String {
    format!("{CHANNEL_PATH_PREFIX}{broadcaster_id}")
}

pub(crate) fn global_emotes(answer: &Value) -> Vec<ProviderEmote> {
    set_emotes(answer)
}

pub(crate) fn channel_emotes(answer: &Value) -> Vec<ProviderEmote> {
    set_emotes(&answer["emote_set"])
}

fn set_emotes(set: &Value) -> Vec<ProviderEmote> {
    set["emotes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| emote(entry["name"].as_str()?, image_url(&entry["data"]["host"])?))
        .collect()
}

fn image_url(host: &Value) -> Option<String> {
    let base = host["url"].as_str()?.trim_end_matches('/');
    let names: Vec<&str> = host["files"]
        .as_array()?
        .iter()
        .filter_map(|file| file["name"].as_str())
        .collect();
    let file = names
        .iter()
        .find(|name| **name == PREFERRED_FILE)
        .or_else(|| {
            names
                .iter()
                .find(|name| name.ends_with(ANIMATABLE_EXTENSION))
        })?;
    Some(format!("{}/{file}", absolute_https(base)))
}
