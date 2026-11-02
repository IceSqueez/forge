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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn emoticon(id: u64, name: &str) -> Value {
        json!({
            "id": id,
            "name": name,
            "height": 28,
            "width": 28,
            "public": true,
            "hidden": false,
            "modifier": false,
            "owner": { "_id": 1, "name": "someone", "display_name": "Someone" },
            "urls": {
                "1": format!("https://cdn.frankerfacez.com/emote/{id}/1"),
                "2": format!("https://cdn.frankerfacez.com/emote/{id}/2"),
                "4": format!("https://cdn.frankerfacez.com/emote/{id}/4"),
            },
        })
    }

    fn set(id: u64, emoticons: Vec<Value>) -> Value {
        json!({ "id": id, "_type": 1, "icon": null, "title": "Set", "css": null, "emoticons": emoticons })
    }

    fn codes(emotes: Vec<ProviderEmote>) -> Vec<String> {
        emotes.into_iter().map(|e| e.code).collect()
    }

    fn urls(emotes: Vec<ProviderEmote>) -> Vec<String> {
        emotes.into_iter().map(|e| e.url).collect()
    }

    #[test]
    fn the_global_answer_reads_only_its_default_sets() {
        let answer = json!({
            "default_sets": [3],
            "sets": {
                "3": set(3, vec![emoticon(25927, "CatBag")]),
                "4330": set(4330, vec![emoticon(9, "NotDefault")]),
            },
            "users": { "4330": ["someone"] },
        });

        assert_eq!(codes(global_emotes(&answer)), ["CatBag"]);
    }

    #[test]
    fn a_default_set_given_as_a_string_key_is_read_too() {
        let answer = json!({
            "default_sets": ["3"],
            "sets": { "3": set(3, vec![emoticon(25927, "CatBag")]) },
        });

        assert_eq!(codes(global_emotes(&answer)), ["CatBag"]);
    }

    #[test]
    fn an_emote_is_served_at_double_size_preferring_its_animated_image() {
        let mut animated = emoticon(720507, "Wiggle");
        animated["animated"] = json!({
            "1": "https://cdn.frankerfacez.com/emote/720507/animated/1",
            "2": "https://cdn.frankerfacez.com/emote/720507/animated/2",
        });
        let answer = json!({
            "room": { "_id": 1, "twitch_id": 12826, "set": 1 },
            "sets": { "1": set(1, vec![emoticon(381875, "KEKW"), animated]) },
        });

        assert_eq!(
            urls(channel_emotes(&answer)),
            [
                "https://cdn.frankerfacez.com/emote/381875/2",
                "https://cdn.frankerfacez.com/emote/720507/animated/2",
            ]
        );
    }

    #[test]
    fn a_missing_double_size_falls_back_to_single_then_quadruple() {
        let mut single = emoticon(1, "Single");
        single["urls"] = json!({ "1": "https://cdn.frankerfacez.com/emote/1/1", "4": "https://cdn.frankerfacez.com/emote/1/4" });
        let mut quadruple = emoticon(2, "Quadruple");
        quadruple["urls"] = json!({ "4": "https://cdn.frankerfacez.com/emote/2/4" });
        let mut none = emoticon(3, "NoImage");
        none["urls"] = json!({});
        let answer = json!({ "sets": { "1": set(1, vec![single, quadruple, none]) } });

        assert_eq!(
            urls(channel_emotes(&answer)),
            [
                "https://cdn.frankerfacez.com/emote/1/1",
                "https://cdn.frankerfacez.com/emote/2/4",
            ]
        );
    }

    #[test]
    fn a_protocol_relative_image_is_served_over_https() {
        let mut legacy = emoticon(5, "Legacy");
        legacy["urls"] = json!({ "2": "//cdn.frankerfacez.com/emote/5/2" });
        let answer = json!({ "sets": { "1": set(1, vec![legacy]) } });

        assert_eq!(
            urls(channel_emotes(&answer)),
            ["https://cdn.frankerfacez.com/emote/5/2"]
        );
    }

    #[test]
    fn a_channel_answer_reads_every_set_it_carries() {
        let answer = json!({
            "room": { "_id": 1, "twitch_id": 12826, "set": 1 },
            "sets": {
                "1": set(1, vec![emoticon(10, "RoomEmote")]),
                "2": set(2, vec![emoticon(11, "ExtraSetEmote")]),
            },
        });

        let mut found = codes(channel_emotes(&answer));
        found.sort();

        assert_eq!(found, ["ExtraSetEmote", "RoomEmote"]);
    }

    #[test]
    fn an_answer_without_sets_has_no_emotes() {
        for answer in [
            json!({ "error": "Not Found", "status": 404, "message": "No such room" }),
            json!({ "default_sets": [3], "sets": {} }),
            json!(null),
        ] {
            assert!(global_emotes(&answer).is_empty(), "{answer}");
            assert!(channel_emotes(&answer).is_empty(), "{answer}");
        }
    }
}
