use crate::platform_visuals::platform_color;
use forge_components::{ForgePalette, Icon};
use forge_host::integration_catalog::declaration_of;
use forge_platform_core::{
    IntegrationCategory, IntegrationDisclaimer, IntegrationIdentity, IntegrationTint, TileMark,
};
use forge_types::{IntegrationId, PlatformId};
use gpui::Rgba;

use crate::hub_card::HubTileGlyph;
use crate::navigation::Screen;

#[derive(Clone)]
pub(crate) struct CoreFeature {
    pub key: &'static str,
    pub category: IntegrationCategory,
    pub name_key: &'static str,
    pub description_key: &'static str,
    pub glyph: Icon,
    pub screen: Screen,
}

pub(crate) fn core_features() -> [CoreFeature; 4] {
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

pub(crate) fn core_tint(key: &str, palette: &ForgePalette) -> Rgba {
    match key {
        "tts" => palette.accent_teal,
        "soundboard" => palette.accent_peach,
        "overlays" => palette.accent_pink_light,
        _ => palette.info,
    }
}

pub(crate) struct IntegrationLook {
    pub letter: Option<&'static str>,
    pub glyph: Icon,
    pub tint: Rgba,
}

impl IntegrationLook {
    pub(crate) fn tile_glyph(&self) -> HubTileGlyph {
        match self.letter {
            Some(letter) => HubTileGlyph::Letter(letter.into()),
            None => HubTileGlyph::Icon(self.glyph),
        }
    }
}

pub(crate) fn tint_color(tint: IntegrationTint, palette: &ForgePalette) -> Rgba {
    match tint {
        IntegrationTint::PlatformBrand(platform) => platform_color(platform, palette),
        IntegrationTint::ThemeBrand => palette.theme_brand,
        IntegrationTint::AccentPinkLight => palette.accent_pink_light,
        IntegrationTint::AccentTeal => palette.accent_teal,
        IntegrationTint::TextSecondary => palette.text_secondary,
        IntegrationTint::Success => palette.success,
        IntegrationTint::Warning => palette.warning,
        IntegrationTint::Danger => palette.danger,
        IntegrationTint::Info => palette.info,
    }
}

pub(crate) fn identity_of(id: &IntegrationId) -> Option<IntegrationIdentity> {
    declaration_of(id).map(|declaration| declaration.identity)
}

pub(crate) fn look_of(id: &IntegrationId, palette: &ForgePalette) -> IntegrationLook {
    match identity_of(id) {
        Some(identity) => IntegrationLook {
            letter: (identity.tile_mark == TileMark::Monogram).then_some(identity.monogram),
            glyph: Icon::from_name(identity.icon),
            tint: tint_color(identity.tile_tint, palette),
        },
        None => IntegrationLook {
            letter: None,
            glyph: Icon::Plug,
            tint: palette.text_muted,
        },
    }
}

pub(crate) fn disclaimer_of(id: &IntegrationId) -> Option<IntegrationDisclaimer> {
    declaration_of(id)?.disclaimer
}

pub(crate) fn chat_platform_of(id: &IntegrationId) -> Option<PlatformId> {
    declaration_of(id)?.chat_platform()
}

pub(crate) fn activity_key(id: &IntegrationId) -> String {
    format!("integration_activity_{id}")
}

#[cfg(test)]
mod tests {
    use forge_host::integration_catalog::declarations;

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

    #[test]
    fn every_declared_glyph_name_is_a_known_icon() {
        for declaration in declarations() {
            assert!(
                Icon::named(declaration.identity.icon).is_some(),
                "{}: {}",
                declaration.id,
                declaration.identity.icon
            );
        }
    }

    #[test]
    fn every_declared_monogram_is_one_uppercase_letter() {
        for declaration in declarations() {
            let mut letters = declaration.identity.monogram.chars();
            assert!(
                letters.next().is_some_and(char::is_uppercase) && letters.next().is_none(),
                "{}: {:?}",
                declaration.id,
                declaration.identity.monogram
            );
        }
    }

    #[test]
    fn chat_platforms_are_exactly_the_three_oauth_integrations() {
        let routed: Vec<_> = declarations()
            .iter()
            .filter_map(|declaration| {
                Some((
                    declaration.id.to_string(),
                    chat_platform_of(&declaration.id)?,
                ))
            })
            .collect();
        assert_eq!(
            routed,
            [
                ("twitch".to_owned(), PlatformId::Twitch),
                ("youtube".to_owned(), PlatformId::YouTube),
                ("kick".to_owned(), PlatformId::Kick),
            ]
        );
    }

    #[test]
    fn only_kick_declares_a_disclaimer() {
        let declared: Vec<_> = declarations()
            .into_iter()
            .filter(|declaration| disclaimer_of(&declaration.id).is_some())
            .map(|declaration| declaration.id.to_string())
            .collect();
        assert_eq!(declared, ["kick"]);
    }

    #[test]
    fn each_tile_keeps_its_mark_and_tint() {
        let palette = forge_components::FORGE_DEFAULT;
        for (id, letter, glyph, tint) in [
            (
                "twitch",
                Some("T"),
                Icon::Broadcast,
                palette.platform_twitch,
            ),
            (
                "youtube",
                Some("Y"),
                Icon::Broadcast,
                palette.platform_youtube,
            ),
            ("kick", Some("K"), Icon::Broadcast, palette.platform_kick),
            (
                "donatello",
                Some("D"),
                Icon::Coin,
                palette.accent_pink_light,
            ),
            ("monobank", Some("M"), Icon::Coin, palette.text_secondary),
            ("obs", None, Icon::Broadcast, palette.success),
            ("vtube", None, Icon::MoodTongue, palette.warning),
            ("discord", None, Icon::BrandDiscord, palette.theme_brand),
            ("midi", None, Icon::Piano, palette.info),
            ("hotkey", None, Icon::Keyboard, palette.success),
        ] {
            let look = look_of(&IntegrationId::new(id), &palette);
            assert_eq!(
                (look.letter, look.glyph, look.tint),
                (letter, glyph, tint),
                "{id}"
            );
        }
    }

    #[test]
    fn an_undeclared_id_gets_the_neutral_look_and_no_route() {
        let palette = forge_components::FORGE_DEFAULT;
        let unknown = IntegrationId::new("trovo");
        let look = look_of(&unknown, &palette);

        assert_eq!(
            (look.letter, look.glyph, look.tint),
            (None, Icon::Plug, palette.text_muted)
        );
        assert_eq!(chat_platform_of(&unknown), None);
        assert_eq!(disclaimer_of(&unknown).map(|d| d.full_key), None);
    }
}
