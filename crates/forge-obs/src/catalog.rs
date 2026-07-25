use std::collections::HashMap;

use forge_platform_core::{
    BuiltinContent, ContentList, ContentListItem, DetailSection, SectionIcon, TokenColor,
    TrailingToken,
};

use crate::client::ObsClient;
use crate::source::SourceInfo;

#[derive(Default)]
pub(crate) struct ObsCatalog {
    pub scenes: Vec<String>,
    pub current_scene: Option<String>,
    pub current_preview_scene: Option<String>,
    pub sources: HashMap<String, Vec<SourceInfo>>,
    pub audio_inputs: Vec<String>,
}

fn scene_to_item(
    name: &str,
    current_scene: Option<&str>,
    source_count: Option<usize>,
) -> ContentListItem {
    let is_current = current_scene == Some(name);
    let mut trailing = Vec::new();
    if !is_current && let Some(count) = source_count {
        trailing.push(TrailingToken::Label(format!("{count} src")));
    }
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
        trailing,
        enabled: true,
    }
}

/// Classifies an OBS input kind id into the closest matching source glyph. Kind ids differ by
/// OS/OBS version (e.g. `monitor_capture` on Linux/macOS vs `game_capture` on Windows), so this
/// matches on stable substrings rather than an exhaustive enum.
fn icon_for_kind(kind: Option<&str>) -> SectionIcon {
    let Some(kind) = kind else {
        return SectionIcon::new("device-desktop");
    };
    if kind == "image_source" {
        SectionIcon::new("photo")
    } else if kind.starts_with("text_") {
        SectionIcon::new("typography")
    } else if kind == "browser_source" {
        SectionIcon::new("browser")
    } else if kind.contains("ffmpeg_source")
        || kind.contains("vlc_source")
        || kind.contains("slideshow")
    {
        SectionIcon::new("movie")
    } else if kind.contains("input_capture") {
        SectionIcon::new("microphone")
    } else if kind.contains("output_capture") {
        SectionIcon::new("volume")
    } else if kind.contains("v4l2") || kind.contains("dshow") || kind.contains("avcapture") {
        SectionIcon::new("video")
    } else {
        SectionIcon::new("device-desktop")
    }
}

/// Audio-capable kinds are the only ones a `GetInputVolume` request succeeds against.
pub(crate) fn is_audio_kind(kind: Option<&str>) -> bool {
    kind.is_some_and(|k| k.contains("input_capture") || k.contains("output_capture"))
}

fn source_to_item(info: &SourceInfo) -> ContentListItem {
    let mut trailing = Vec::new();
    trailing.push(TrailingToken::Icon(
        SectionIcon::new(if info.visible { "eye" } else { "eye-off" }),
        if info.visible {
            TokenColor::Green
        } else {
            TokenColor::Muted
        },
    ));
    trailing.push(TrailingToken::Icon(
        SectionIcon::new(if info.locked { "lock" } else { "lock-open" }),
        if info.locked {
            TokenColor::Yellow
        } else {
            TokenColor::Muted
        },
    ));
    if let Some(db) = info.audio_db {
        trailing.push(TrailingToken::Label(format!("{db:.1} dB")));
    }
    ContentListItem {
        icon: icon_for_kind(info.kind.as_deref()),
        name: info.name.clone(),
        monospace_name: true,
        active: false,
        active_label: None,
        trailing,
        enabled: info.visible,
    }
}

impl BuiltinContent for ObsClient {
    fn sections(&self) -> Vec<DetailSection> {
        let Ok(catalog) = self.catalog_state.try_read() else {
            return vec![];
        };

        let scene_items: Vec<ContentListItem> = catalog
            .scenes
            .iter()
            .map(|s| {
                let source_count = catalog.sources.get(s).map(Vec::len);
                scene_to_item(s, catalog.current_scene.as_deref(), source_count)
            })
            .collect();
        let scene_count = format!("{}", catalog.scenes.len());

        let source_items: Vec<ContentListItem> = catalog
            .current_scene
            .as_deref()
            .and_then(|scene| catalog.sources.get(scene))
            .map(|sources| sources.iter().map(source_to_item).collect())
            .unwrap_or_default();
        let source_count_label = match catalog.current_scene.as_deref() {
            Some(scene) => format!("in {scene} \u{00b7} {} total", source_items.len()),
            None => format!("{} total", source_items.len()),
        };

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
            count_label: Some(source_count_label),
            items: source_items,
            footer: None,
        };

        vec![DetailSection::TwoColumnLists { left, right }]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_platform_core::BuiltinContent;

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
                    kind: Some("monitor_capture".to_owned()),
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
