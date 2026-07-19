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
