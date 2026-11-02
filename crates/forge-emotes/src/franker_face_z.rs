use serde_json::Value;

use crate::provider::{ProviderEmote, absolute_https, emote};

pub(crate) const GLOBAL_PATH: &str = "/set/global";
const CHANNEL_PATH_PREFIX: &str = "/room/id/";
const SCALE_PREFERENCE: [&str; 3] = ["2", "1", "4"];

pub(crate) fn channel_path(broadcaster_id: &str) -> String {
    format!("{CHANNEL_PATH_PREFIX}{broadcaster_id}")
}

pub(crate) fn global_emotes(answer: &Value) -> Vec<ProviderEmote> {
    answer["default_sets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(set_key)
        .flat_map(|key| set_emotes(&answer["sets"][key.as_str()]))
        .collect()
}

pub(crate) fn channel_emotes(answer: &Value) -> Vec<ProviderEmote> {
    answer["sets"]
        .as_object()
        .into_iter()
        .flat_map(|sets| sets.values())
        .flat_map(set_emotes)
        .collect()
}

fn set_key(id: &Value) -> Option<String> {
    id.as_u64()
        .map(|id| id.to_string())
        .or_else(|| id.as_str().map(str::to_owned))
}

fn set_emotes(set: &Value) -> Vec<ProviderEmote> {
    set["emoticons"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let url = scaled(&entry["animated"]).or_else(|| scaled(&entry["urls"]))?;
            emote(entry["name"].as_str()?, url)
        })
        .collect()
}

fn scaled(urls: &Value) -> Option<String> {
    SCALE_PREFERENCE
        .iter()
        .find_map(|scale| urls[*scale].as_str())
        .map(absolute_https)
}
