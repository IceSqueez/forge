use std::collections::HashMap;

use forge_platform_core::{
    ContentList, ContentListItem, DetailSection, IntegrationContent, SectionIcon, TrailingToken,
};

use crate::client::ObsClient;
use crate::source::SourceInfo;

#[derive(Default)]
pub(crate) struct ObsCatalog {
    pub scenes: Vec<String>,
    pub current_scene: Option<String>,
    pub sources: HashMap<String, Vec<SourceInfo>>,
    pub audio_inputs: Vec<String>,
}

fn scene_to_item(name: &str, current_scene: Option<&str>) -> ContentListItem {
    let is_current = current_scene == Some(name);
    ContentListItem {
        icon: if is_current {
            SectionIcon::new("eye")
        } else {
            SectionIcon::new("layout")
        },
        name: name.to_owned(),
        monospace_name: false,
        active: is_current,
        active_label: if is_current {
            Some("LIVE".to_owned())
        } else {
            None
        },
        trailing: vec![],
        enabled: true,
    }
}

fn source_to_item(info: &SourceInfo) -> ContentListItem {
    let mut trailing = Vec::new();
    trailing.push(TrailingToken::Icon(SectionIcon::new(if info.visible {
        "eye"
    } else {
        "eye-off"
    })));
    trailing.push(TrailingToken::Icon(SectionIcon::new(if info.locked {
        "lock"
    } else {
        "lock-open"
    })));
    if let Some(db) = info.audio_db {
        trailing.push(TrailingToken::Label(format!("{db:.1} dB")));
    }
    ContentListItem {
        icon: SectionIcon::new("device-desktop"),
        name: info.name.clone(),
        monospace_name: true,
        active: false,
        active_label: None,
        trailing,
        enabled: info.visible,
    }
}

impl IntegrationContent for ObsClient {
    fn sections(&self) -> Vec<DetailSection> {
        let Ok(catalog) = self.catalog_state.try_read() else {
            return vec![];
        };

        let scene_items: Vec<ContentListItem> = catalog
            .scenes
            .iter()
            .map(|s| scene_to_item(s, catalog.current_scene.as_deref()))
            .collect();
        let scene_count = format!("{}", catalog.scenes.len());

        let source_items: Vec<ContentListItem> = catalog
            .current_scene
            .as_deref()
            .and_then(|scene| catalog.sources.get(scene))
            .map(|sources| sources.iter().map(source_to_item).collect())
            .unwrap_or_default();
        let source_count = format!("{}", source_items.len());

        let left = ContentList {
            title: "Scenes".to_owned(),
            icon: SectionIcon::new("layout-grid"),
            count_label: Some(scene_count),
            items: scene_items,
            footer: None,
        };

        let right = ContentList {
            title: "Sources".to_owned(),
            icon: SectionIcon::new("stack-2"),
            count_label: Some(source_count),
            items: source_items,
            footer: None,
        };

        vec![DetailSection::TwoColumnLists { left, right }]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_platform_core::IntegrationContent;

    use super::*;
    use crate::client::ObsClient;

    #[test]
    fn sections_empty_catalog_returns_single_two_column_lists_with_empty_items() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let sections = client.sections();
        assert_eq!(sections.len(), 1);
        let DetailSection::TwoColumnLists { left, right } = &sections[0] else {
            panic!("expected TwoColumnLists");
        };
        assert!(left.items.is_empty());
        assert!(right.items.is_empty());
    }

    #[test]
    fn sections_populated_catalog_marks_current_scene_active_with_live_label() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        {
            let mut catalog = client.catalog_state.write().unwrap();
            catalog.scenes = vec!["Gameplay".to_owned(), "BRB".to_owned()];
            catalog.current_scene = Some("Gameplay".to_owned());
            catalog.sources.insert(
                "Gameplay".to_owned(),
                vec![SourceInfo {
                    name: "Game Capture".to_owned(),
                    visible: true,
                    locked: false,
                    audio_db: Some(-12.5),
                }],
            );
        }
        let sections = client.sections();
        let DetailSection::TwoColumnLists { left, right } = &sections[0] else {
            panic!("expected TwoColumnLists");
        };
        assert_eq!(left.items.len(), 2);
        assert!(left.items[0].active);
        assert_eq!(left.items[0].name, "Gameplay");
        assert_eq!(left.items[0].active_label.as_deref(), Some("LIVE"));
        assert!(!left.items[1].active);
        assert_eq!(right.items.len(), 1);
        assert_eq!(right.items[0].name, "Game Capture");
        assert!(right.items[0].enabled);
    }
}
