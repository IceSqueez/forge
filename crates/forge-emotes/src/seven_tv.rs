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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn files(names: &[&str]) -> Value {
        Value::Array(
            names
                .iter()
                .map(|name| {
                    json!({ "name": name, "static_name": name, "width": 64, "height": 64, "format": "WEBP" })
                })
                .collect(),
        )
    }

    fn entry(name: &str, host_url: &str, file_names: &[&str]) -> Value {
        json!({
            "id": "60ae958e229664e8667aea38",
            "name": name,
            "flags": 0,
            "timestamp": 1_621_993_870_000_u64,
            "actor_id": null,
            "data": {
                "id": "60ae958e229664e8667aea38",
                "name": "peepoHey",
                "flags": 0,
                "listed": true,
                "animated": true,
                "host": { "url": host_url, "files": files(file_names) },
            },
        })
    }

    fn emote_set(emotes: Vec<Value>) -> Value {
        json!({
            "id": "62cdd34e72a832540de95857",
            "name": "Global Emotes",
            "flags": 0,
            "immutable": true,
            "privileged": true,
            "emotes": emotes,
            "emote_count": 1,
            "capacity": 1000,
        })
    }

    fn codes_and_urls(emotes: Vec<ProviderEmote>) -> Vec<(String, String)> {
        emotes.into_iter().map(|e| (e.code, e.url)).collect()
    }

    fn codes(emotes: Vec<ProviderEmote>) -> Vec<String> {
        emotes.into_iter().map(|e| e.code).collect()
    }

    #[test]
    fn a_global_set_emote_is_served_from_its_https_host_at_double_size_webp() {
        let answer = emote_set(vec![entry(
            "peepoHey",
            "//cdn.7tv.app/emote/60ae958e229664e8667aea38",
            &[
                "1x.avif", "1x.webp", "2x.avif", "2x.webp", "3x.webp", "4x.webp",
            ],
        )]);

        assert_eq!(
            codes_and_urls(global_emotes(&answer)),
            [(
                "peepoHey".to_owned(),
                "https://cdn.7tv.app/emote/60ae958e229664e8667aea38/2x.webp".to_owned()
            )]
        );
    }

    #[test]
    fn a_set_emote_is_listed_under_its_alias_in_the_set() {
        let answer = emote_set(vec![entry(
            "HeyFriend",
            "//cdn.7tv.app/emote/a",
            &["2x.webp"],
        )]);

        assert_eq!(codes(global_emotes(&answer)), ["HeyFriend"]);
    }

    #[test]
    fn an_emote_without_double_size_webp_falls_back_to_another_webp_or_is_dropped() {
        let answer = emote_set(vec![
            entry("OnlyOne", "//cdn.7tv.app/emote/a/", &["1x.avif", "1x.webp"]),
            entry("NoWebp", "//cdn.7tv.app/emote/b", &["1x.avif", "2x.avif"]),
            entry("NoFiles", "//cdn.7tv.app/emote/c", &[]),
        ]);

        assert_eq!(
            codes_and_urls(global_emotes(&answer)),
            [(
                "OnlyOne".to_owned(),
                "https://cdn.7tv.app/emote/a/1x.webp".to_owned()
            )]
        );
    }

    #[test]
    fn an_emote_missing_its_host_or_carrying_an_unusable_name_is_dropped() {
        let mut no_host = entry("NoHost", "//cdn.7tv.app/emote/a", &["2x.webp"]);
        no_host["data"]["host"] = Value::Null;
        let answer = emote_set(vec![
            no_host,
            entry("two words", "//cdn.7tv.app/emote/b", &["2x.webp"]),
            entry("", "//cdn.7tv.app/emote/c", &["2x.webp"]),
            json!({ "id": "x", "data": { "host": { "url": "//cdn.7tv.app/emote/d", "files": files(&["2x.webp"]) } } }),
            entry("Kept", "//cdn.7tv.app/emote/e", &["2x.webp"]),
        ]);

        assert_eq!(codes(global_emotes(&answer)), ["Kept"]);
    }

    #[test]
    fn a_channel_user_answer_reads_the_emotes_of_its_active_set() {
        let answer = json!({
            "id": "01FCJ7QDJ80006EHDKGM5M9T6V",
            "platform": "TWITCH",
            "username": "streamer",
            "display_name": "Streamer",
            "linked_at": 1_628_000_000_000_u64,
            "emote_capacity": 600,
            "emote_set_id": null,
            "emote_set": emote_set(vec![entry("ChannelOnly", "//cdn.7tv.app/emote/a", &["2x.webp"])]),
            "user": { "id": "01FCJ7QDJ80006EHDKGM5M9T6V", "username": "streamer" },
        });

        assert_eq!(
            codes_and_urls(channel_emotes(&answer)),
            [(
                "ChannelOnly".to_owned(),
                "https://cdn.7tv.app/emote/a/2x.webp".to_owned()
            )]
        );
    }

    #[test]
    fn a_channel_without_an_active_set_has_no_emotes() {
        for answer in [
            json!({ "id": "01FCJ7QDJ80006EHDKGM5M9T6V", "platform": "TWITCH", "emote_set": null }),
            json!({ "id": "01FCJ7QDJ80006EHDKGM5M9T6V", "platform": "TWITCH" }),
            json!({ "emote_set": { "id": "x", "emotes": null } }),
        ] {
            assert!(channel_emotes(&answer).is_empty(), "{answer}");
        }
    }
}
