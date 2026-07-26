use std::collections::HashMap;

use forge_platform_core::{
    BuiltinContent, ContentList, ContentListItem, DetailSection, SectionIcon, TokenColor,
    TrailingToken,
};

use crate::client::ObsClient;
use crate::source::SourceInfo;

const PANEL_VISIBLE_ROWS: u16 = 8;
const SCENE_ROW_PADDING_Y_PX: u8 = 8;
const SOURCE_ROW_PADDING_Y_PX: u8 = 7;

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
        icon_tint: None,
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
    } else if kind.contains("v4l2")
        || kind.contains("dshow")
        || kind.contains("avcapture")
        || kind.contains("av_capture")
    {
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
        trailing.push(TrailingToken::TintedLabel(
            format!("{db:.1} dB"),
            TokenColor::Subtle,
        ));
    }
    ContentListItem {
        icon: if info.visible {
            icon_for_kind(info.kind.as_deref())
        } else {
            SectionIcon::new("eye-off")
        },
        icon_tint: Some(if info.visible {
            TokenColor::Accent
        } else {
            TokenColor::Muted
        }),
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
        let source_scene_label = catalog
            .current_scene
            .as_deref()
            .map(|scene| format!("in {scene}"));
        let source_count_label = format!("{} total", source_items.len());

        let left = ContentList {
            title: "Scenes".to_owned(),
            icon: SectionIcon::new("layout-grid"),
            inline_label: Some(scene_count),
            count_label: None,
            visible_rows: Some(PANEL_VISIBLE_ROWS),
            row_padding_y_px: SCENE_ROW_PADDING_Y_PX,
            refreshable: true,
            items: scene_items,
            footer: None,
        };

        let right = ContentList {
            title: "Sources".to_owned(),
            icon: SectionIcon::new("stack-2"),
            inline_label: source_scene_label,
            count_label: Some(source_count_label),
            visible_rows: Some(PANEL_VISIBLE_ROWS),
            row_padding_y_px: SOURCE_ROW_PADDING_Y_PX,
            refreshable: false,
            items: source_items,
            footer: None,
        };

        vec![DetailSection::TwoColumnLists {
            left: Box::new(left),
            right: Box::new(right),
        }]
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

    fn source(visible: bool, locked: bool, audio_db: Option<f32>, kind: &str) -> SourceInfo {
        SourceInfo {
            name: "Cam".to_owned(),
            visible,
            locked,
            audio_db,
            kind: Some(kind.to_owned()),
        }
    }

    #[test]
    fn icon_for_kind_maps_input_kinds_to_their_glyph_family() {
        for (kind, expected) in [
            (Some("image_source"), "photo"),
            (Some("text_ft2_source_v2"), "typography"),
            (Some("text_gdiplus_v3"), "typography"),
            (Some("browser_source"), "browser"),
            (Some("ffmpeg_source"), "movie"),
            (Some("vlc_source"), "movie"),
            (Some("slideshow_v2"), "movie"),
            (Some("wasapi_input_capture"), "microphone"),
            (Some("pulse_input_capture"), "microphone"),
            (Some("wasapi_output_capture"), "volume"),
            (Some("v4l2_input"), "video"),
            (Some("dshow_input"), "video"),
            (Some("monitor_capture"), "device-desktop"),
            (Some(""), "device-desktop"),
            (None, "device-desktop"),
        ] {
            assert_eq!(
                icon_for_kind(kind).as_str(),
                expected,
                "icon_for_kind({kind:?})",
            );
        }
    }

    #[test]
    fn is_audio_kind_accepts_only_capture_inputs_and_outputs() {
        for kind in [
            Some("wasapi_input_capture"),
            Some("coreaudio_output_capture"),
        ] {
            assert!(is_audio_kind(kind), "{kind:?} should be audio");
        }
        for kind in [Some("browser_source"), Some("monitor_capture"), None] {
            assert!(!is_audio_kind(kind), "{kind:?} should not be audio");
        }
    }

    #[test]
    fn active_scene_row_carries_the_live_label_instead_of_a_source_count() {
        let item = scene_to_item("Gameplay", Some("Gameplay"), Some(4));
        assert_eq!(item.active_label.as_deref(), Some("LIVE"));
        assert!(item.trailing.is_empty());
    }

    #[test]
    fn inactive_scene_row_carries_its_source_count_and_no_live_label() {
        let item = scene_to_item("BRB", Some("Gameplay"), Some(4));
        assert!(item.active_label.is_none());
        assert_eq!(
            item.trailing,
            vec![TrailingToken::Label("4 src".to_owned())]
        );
    }

    #[test]
    fn scene_row_with_an_uncached_source_count_carries_no_trailing_token() {
        let item = scene_to_item("BRB", Some("Gameplay"), None);
        assert!(item.trailing.is_empty());
    }

    #[test]
    fn source_row_tints_the_visibility_glyph_by_state() {
        for (visible, glyph, color) in [
            (true, "eye", TokenColor::Green),
            (false, "eye-off", TokenColor::Muted),
        ] {
            let item = source_to_item(&source(visible, false, None, "browser_source"));
            assert_eq!(
                item.trailing[0],
                TrailingToken::Icon(SectionIcon::new(glyph), color),
            );
        }
    }

    #[test]
    fn source_row_tints_the_lock_glyph_by_state() {
        for (locked, glyph, color) in [
            (true, "lock", TokenColor::Yellow),
            (false, "lock-open", TokenColor::Muted),
        ] {
            let item = source_to_item(&source(true, locked, None, "browser_source"));
            assert_eq!(
                item.trailing[1],
                TrailingToken::Icon(SectionIcon::new(glyph), color),
            );
        }
    }

    #[test]
    fn source_row_appends_a_one_decimal_db_label_only_when_a_level_is_known() {
        let with_level = source_to_item(&source(true, false, Some(-12.34), "wasapi_input_capture"));
        assert_eq!(
            with_level.trailing.last(),
            Some(&TrailingToken::TintedLabel(
                "-12.3 dB".to_owned(),
                TokenColor::Subtle,
            )),
        );

        let without_level = source_to_item(&source(true, false, None, "wasapi_input_capture"));
        assert_eq!(without_level.trailing.len(), 2);
    }

    #[test]
    fn source_row_leading_glyph_and_tint_follow_visibility() {
        for (visible, glyph, tint) in [
            (true, "browser", TokenColor::Accent),
            (false, "eye-off", TokenColor::Muted),
        ] {
            let item = source_to_item(&source(visible, false, None, "browser_source"));
            assert_eq!(item.icon.as_str(), glyph, "glyph for visible={visible}");
            assert_eq!(item.icon_tint, Some(tint), "tint for visible={visible}");
        }
    }

    #[test]
    fn hidden_source_row_is_rendered_disabled() {
        assert!(!source_to_item(&source(false, false, None, "browser_source")).enabled);
    }
}
