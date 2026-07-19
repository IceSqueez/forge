use std::collections::HashSet;

use gpui::SharedString;

pub fn parse(raw: Option<String>) -> HashSet<SharedString> {
    raw.as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
        .into_iter()
        .map(SharedString::from)
        .collect()
}

pub fn encode(favorites: &HashSet<SharedString>) -> String {
    let ids: Vec<&str> = favorites.iter().map(SharedString::as_ref).collect();
    serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[&str]) -> HashSet<SharedString> {
        ids.iter()
            .map(|s| SharedString::from(s.to_string()))
            .collect()
    }

    #[test]
    fn encode_then_parse_round_trips_set_contents() {
        // Order-independence is intrinsic: HashSet equality ignores iteration order,
        // and encode() emits in arbitrary order. The empty case pins that an empty
        // set encodes to a representation parse() reads back as empty.
        for original in [
            set(&[]),
            set(&["sub.enable"]),
            set(&["sub.enable", "trigger.chat", "sub.raw"]),
        ] {
            let restored = parse(Some(encode(&original)));
            assert_eq!(restored, original);
        }
    }

    #[test]
    fn parse_returns_empty_set_for_missing_malformed_or_wrong_shape() {
        // A corrupted / legacy / manually-edited settings row must never break the
        // picker: every non-`Vec<String>` payload degrades to an empty selection.
        let raws = [
            None,                                   // key absent
            Some(String::new()),                    // empty string
            Some("not json".to_owned()),            // invalid JSON
            Some("{}".to_owned()),                  // object, not array
            Some("123".to_owned()),                 // number
            Some("null".to_owned()),                // null
            Some("true".to_owned()),                // bool
            Some("\"sub.enable\"".to_owned()),      // bare string, not array
            Some("[1, 2, 3]".to_owned()),           // array of non-strings
            Some("[\"sub.enable\", 7]".to_owned()), // mixed array
        ];
        for raw in raws {
            assert!(parse(raw.clone()).is_empty(), "expected empty for {raw:?}");
        }
    }

    #[test]
    fn parse_deduplicates_repeated_ids() {
        let favorites = parse(Some(
            "[\"sub.enable\", \"sub.enable\", \"trigger.chat\"]".to_owned(),
        ));
        assert_eq!(favorites, set(&["sub.enable", "trigger.chat"]));
    }

    #[test]
    fn parse_reads_persisted_string_array_as_favorites() {
        // Pins the on-disk wire format: a plain JSON string array. A swapped impl
        // using any other encoding would fail here while round-trip stayed self-consistent.
        let favorites = parse(Some("[\"sub.enable\", \"trigger.chat\"]".to_owned()));
        assert_eq!(favorites, set(&["sub.enable", "trigger.chat"]));
    }
}
