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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entry(id: &str, code: &str) -> Value {
        json!({ "id": id, "code": code, "imageType": "png", "animated": false, "userId": "5561169bd6b9d206222a8c19" })
    }

    fn codes(emotes: Vec<ProviderEmote>) -> Vec<String> {
        emotes.into_iter().map(|e| e.code).collect()
    }

    #[test]
    fn a_global_emote_is_served_from_the_emote_cdn_at_double_size() {
        let answer = json!([{
            "id": "54fa925e01e468494b85b54d",
            "code": "OhMyGoodness",
            "imageType": "gif",
            "animated": true,
            "userId": "5561169bd6b9d206222a8c19",
            "modifier": false,
        }]);

        assert_eq!(
            global_emotes(&answer),
            [ProviderEmote {
                code: "OhMyGoodness".to_owned(),
                url: "https://cdn.betterttv.net/emote/54fa925e01e468494b85b54d/2x".to_owned(),
            }]
        );
    }

    #[test]
    fn a_channel_answer_lists_its_own_emotes_then_the_shared_ones() {
        let answer = json!({
            "id": "5f1b2d3c4a5e6f7a8b9c0d1e",
            "bots": [],
            "avatar": "https://static-cdn.jtvnw.net/jtv_user_pictures/x-profile_image-300x300.png",
            "channelEmotes": [entry("5e4e7a1f08b4447d56a92967", "ChannelOwn")],
            "sharedEmotes": [{
                "id": "5f1b0186cf6d2144653d2970",
                "code": "SharedOne",
                "imageType": "gif",
                "animated": true,
                "user": { "id": "5561169bd6b9d206222a8c19", "name": "someone", "displayName": "Someone", "providerId": "1" },
            }],
        });

        assert_eq!(codes(channel_emotes(&answer)), ["ChannelOwn", "SharedOne"]);
    }

    #[test]
    fn an_emote_whose_id_could_leave_the_emote_path_is_dropped() {
        let at_cap = "a".repeat(64);
        let past_cap = "a".repeat(65);
        let answer = json!([
            entry("../../evil", "Traversal"),
            entry("a/b", "Slash"),
            entry("a?b", "Query"),
            entry("a%2Fb", "Encoded"),
            entry("", "Empty"),
            entry(&past_cap, "PastCap"),
            { "id": 42, "code": "NumericId" },
            entry(&at_cap, "AtCap"),
            entry("5e4e7a1f08b4447d56a92967", "Plain"),
            entry("abc_DEF-123", "Punctuated"),
        ]);

        assert_eq!(
            codes(global_emotes(&answer)),
            ["AtCap", "Plain", "Punctuated"]
        );
    }

    #[test]
    fn an_answer_that_is_not_a_list_has_no_emotes() {
        for answer in [
            json!({ "message": "user not found" }),
            json!(null),
            json!({ "channelEmotes": null, "sharedEmotes": {} }),
        ] {
            assert!(global_emotes(&answer).is_empty(), "{answer}");
            assert!(channel_emotes(&answer).is_empty(), "{answer}");
        }
    }
}
