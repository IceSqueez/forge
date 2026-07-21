use std::collections::HashSet;

use gpui::SharedString;

pub fn to_set(ids: Vec<String>) -> HashSet<SharedString> {
    ids.into_iter().map(SharedString::from).collect()
}

pub fn to_ids(favorites: &HashSet<SharedString>) -> Vec<String> {
    favorites.iter().map(|s| s.to_string()).collect()
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
    fn to_ids_then_to_set_round_trips_set_contents() {
        for original in [
            set(&[]),
            set(&["sub.enable"]),
            set(&["sub.enable", "trigger.chat", "sub.raw"]),
        ] {
            let restored = to_set(to_ids(&original));
            assert_eq!(restored, original);
        }
    }

    #[test]
    fn to_set_deduplicates_repeated_ids() {
        let favorites = to_set(vec![
            "sub.enable".to_owned(),
            "sub.enable".to_owned(),
            "trigger.chat".to_owned(),
        ]);
        assert_eq!(favorites, set(&["sub.enable", "trigger.chat"]));
    }
}
