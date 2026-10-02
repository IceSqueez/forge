use forge_components::{ForgePalette, HubTileGlyph, Icon, PlatformKind, platform_color};
use forge_platform_core::{IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;
use gpui::Rgba;

use crate::screen::Screen;

pub fn declarations() -> [IntegrationDeclaration; 8] {
    [
        forge_platform_twitch::TWITCH_INTEGRATION,
        forge_platform_youtube::YOUTUBE_INTEGRATION,
        forge_platform_kick::KICK_INTEGRATION,
        forge_obs::OBS_INTEGRATION,
        forge_vtube::VTUBE_INTEGRATION,
        forge_discord::DISCORD_INTEGRATION,
        forge_midi::MIDI_INTEGRATION,
        forge_hotkey::HOTKEY_INTEGRATION,
    ]
}

pub fn declaration_of(id: &IntegrationId) -> Option<IntegrationDeclaration> {
    declarations()
        .into_iter()
        .find(|declaration| &declaration.id == id)
}

#[derive(Clone)]
pub struct CoreFeature {
    pub key: &'static str,
    pub category: IntegrationCategory,
    pub name_key: &'static str,
    pub description_key: &'static str,
    pub glyph: Icon,
    pub screen: Screen,
}

pub fn core_features() -> [CoreFeature; 4] {
    [
        CoreFeature {
            key: "tts",
            category: IntegrationCategory::AUDIO,
            name_key: "nav_item_tts",
            description_key: "integration_core_tts_description",
            glyph: Icon::Message2Share,
            screen: Screen::Tts(None),
        },
        CoreFeature {
            key: "soundboard",
            category: IntegrationCategory::AUDIO,
            name_key: "nav_item_soundboard",
            description_key: "integration_core_soundboard_description",
            glyph: Icon::Music,
            screen: Screen::Soundboard,
        },
        CoreFeature {
            key: "overlays",
            category: IntegrationCategory::TOOLS,
            name_key: "nav_item_overlays",
            description_key: "integration_core_overlays_description",
            glyph: Icon::Browser,
            screen: Screen::Overlays,
        },
        CoreFeature {
            key: "server",
            category: IntegrationCategory::TOOLS,
            name_key: "nav_item_ws_server",
            description_key: "integration_core_server_description",
            glyph: Icon::Network,
            screen: Screen::Server,
        },
    ]
}

pub fn core_tint(key: &str, palette: &ForgePalette) -> Rgba {
    match key {
        "tts" => palette.accent_teal,
        "soundboard" => palette.bits,
        "overlays" => palette.accent_pink_light,
        _ => palette.info,
    }
}

pub struct IntegrationLook {
    pub letter: Option<&'static str>,
    pub glyph: Icon,
    pub tint: Rgba,
}

impl IntegrationLook {
    pub fn tile_glyph(&self) -> HubTileGlyph {
        match self.letter {
            Some(letter) => HubTileGlyph::Letter(letter.into()),
            None => HubTileGlyph::Icon(self.glyph),
        }
    }
}

pub fn look_of(id: &IntegrationId, palette: &ForgePalette) -> IntegrationLook {
    let (letter, glyph, tint) = match id.as_str() {
        "twitch" => (
            Some("T"),
            Icon::Broadcast,
            platform_color(PlatformKind::Twitch, palette),
        ),
        "youtube" => (
            Some("Y"),
            Icon::Broadcast,
            platform_color(PlatformKind::YouTube, palette),
        ),
        "kick" => (
            Some("K"),
            Icon::Broadcast,
            platform_color(PlatformKind::Kick, palette),
        ),
        "obs" => (None, Icon::Broadcast, palette.success),
        "vtube" => (None, Icon::MoodTongue, palette.warning),
        "discord" => (None, Icon::BrandDiscord, palette.brand),
        "midi" => (None, Icon::Piano, palette.info),
        "hotkey" => (None, Icon::Keyboard, palette.success),
        _ => (None, Icon::Plug, palette.text_muted),
    };
    IntegrationLook {
        letter,
        glyph,
        tint,
    }
}

pub struct Disclaimer {
    pub short_key: &'static str,
    pub full_key: &'static str,
}

pub fn disclaimer_of(id: &IntegrationId) -> Option<Disclaimer> {
    (id.as_str() == "kick").then_some(Disclaimer {
        short_key: "integration_kick_disclaimer_short",
        full_key: "integration_kick_disclaimer",
    })
}

pub fn activity_key(id: &IntegrationId) -> String {
    format!("integration_activity_{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_integration_and_core_feature_sits_in_a_displayed_category() {
        let placed = declarations()
            .into_iter()
            .map(|declaration| (declaration.id.to_string(), declaration.category))
            .chain(
                core_features()
                    .into_iter()
                    .map(|feature| (feature.key.to_owned(), feature.category)),
            );
        for (name, category) in placed {
            assert!(
                IntegrationCategory::DISPLAY_ORDER.contains(&category),
                "{name} is filed under {}, which the hub and sidebar never render",
                category.key()
            );
        }
    }

    #[test]
    fn each_declared_integration_is_found_by_its_own_id_only() {
        for declaration in declarations() {
            assert_eq!(
                declaration_of(&declaration.id).map(|found| found.brand_name),
                Some(declaration.brand_name),
                "{}",
                declaration.id
            );
        }
        assert!(declaration_of(&IntegrationId::new("trovo")).is_none());
    }
}
