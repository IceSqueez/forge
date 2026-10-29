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

#[cfg(test)]
mod tests {
    use super::*;

    fn current_of(id: &str) -> Option<String> {
        (id == "obs.scenes.switch_current").then(|| "obs.scene.switch_current".to_owned())
    }

    #[test]
    fn rename_favorite_ids_rewrites_only_lists_naming_a_retired_id() {
        for (stored, expected) in [
            (
                r#"["core.logic.wait","obs.scenes.switch_current","core.log.write"]"#,
                Some(r#"["core.logic.wait","obs.scene.switch_current","core.log.write"]"#),
            ),
            (
                r#"[ "obs.scenes.switch_current" ]"#,
                Some(r#"["obs.scene.switch_current"]"#),
            ),
            (r#"["core.logic.wait","obs.scene.switch_current"]"#, None),
            ("[]", None),
            ("", None),
            ("not json", None),
            (r#"{"obs.scenes.switch_current":true}"#, None),
            (r#"["obs.scenes.switch_current",7]"#, None),
        ] {
            assert_eq!(
                rename_favorite_ids(stored, &current_of).as_deref(),
                expected,
                "{stored:?}"
            );
        }
    }
}
