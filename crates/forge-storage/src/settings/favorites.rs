pub fn rename_favorite_ids(
    stored: &str,
    current_of: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let mut ids: Vec<String> = serde_json::from_str(stored).ok()?;
    let mut renamed = false;
    for id in &mut ids {
        if let Some(current) = current_of(id) {
            *id = current;
            renamed = true;
        }
    }
    if !renamed {
        return None;
    }
    serde_json::to_string(&ids).ok()
}
