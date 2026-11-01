use forge_platform_core::{
    ChatLimits, EndpointSurface, IntegrationDeclaration, ViewerAction, ViewerRef, ViewerStep,
};
use forge_types::{IntegrationId, MessageCatalog, PlatformId};

pub const INTEGRATION_MESSAGE_CATALOGS: &[MessageCatalog] = &[
    forge_platform_twitch::vocabulary::messages::TWITCH_MESSAGES,
    forge_platform_youtube::vocabulary::messages::YOUTUBE_MESSAGES,
    forge_platform_kick::vocabulary::messages::KICK_MESSAGES,
    forge_obs::vocabulary::messages::OBS_MESSAGES,
    forge_vtube::vocabulary::messages::VTUBE_MESSAGES,
    forge_midi::vocabulary::messages::MIDI_MESSAGES,
    forge_hotkey::vocabulary::messages::HOTKEY_MESSAGES,
    forge_discord::vocabulary::messages::DISCORD_MESSAGES,
    forge_platform_core::TEST_DONATION_MESSAGES,
    forge_donatello::DONATELLO_MESSAGES,
    forge_monobank::MONOBANK_MESSAGES,
];

pub fn declarations() -> [IntegrationDeclaration; 10] {
    [
        forge_platform_twitch::TWITCH_INTEGRATION,
        forge_platform_youtube::YOUTUBE_INTEGRATION,
        forge_platform_kick::KICK_INTEGRATION,
        forge_donatello::DONATELLO_INTEGRATION,
        forge_monobank::MONOBANK_INTEGRATION,
        forge_obs::OBS_INTEGRATION,
        forge_vtube::VTUBE_INTEGRATION,
        forge_discord::DISCORD_INTEGRATION,
        forge_midi::MIDI_INTEGRATION,
        forge_hotkey::HOTKEY_INTEGRATION,
    ]
}

pub const fn chat_declaration(platform: PlatformId) -> IntegrationDeclaration {
    match platform {
        PlatformId::Twitch => forge_platform_twitch::TWITCH_INTEGRATION,
        PlatformId::YouTube => forge_platform_youtube::YOUTUBE_INTEGRATION,
        PlatformId::Kick => forge_platform_kick::KICK_INTEGRATION,
    }
}

pub const fn chat_limits(platform: PlatformId) -> ChatLimits {
    match platform {
        PlatformId::Twitch => forge_platform_twitch::TWITCH_CHAT_LIMITS,
        PlatformId::YouTube => forge_platform_youtube::YOUTUBE_CHAT_LIMITS,
        PlatformId::Kick => forge_platform_kick::KICK_CHAT_LIMITS,
    }
}

pub const fn loopback_authorize_endpoint(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::YouTube => forge_platform_youtube::GOOGLE_AUTHORIZE_ENDPOINT,
        PlatformId::Kick => forge_platform_kick::KICK_AUTHORIZE_ENDPOINT,
        PlatformId::Twitch => "",
    }
}

pub fn viewer_step(
    platform: PlatformId,
    viewer: ViewerRef<'_>,
    action: &ViewerAction,
) -> Option<ViewerStep> {
    match platform {
        PlatformId::Twitch => forge_platform_twitch::viewer_step(viewer, action),
        PlatformId::YouTube => forge_platform_youtube::viewer_step(viewer, action),
        PlatformId::Kick => forge_platform_kick::viewer_step(viewer, action),
    }
}

pub fn endpoint_surfaces() -> Vec<EndpointSurface> {
    [
        forge_platform_twitch::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_platform_kick::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_platform_youtube::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_donatello::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_monobank::endpoint_surfaces::ENDPOINT_SURFACES,
        forge_discord::endpoint_surfaces::ENDPOINT_SURFACES,
    ]
    .concat()
}

pub fn declaration_of(id: &IntegrationId) -> Option<IntegrationDeclaration> {
    declarations()
        .into_iter()
        .find(|declaration| &declaration.id == id)
}

#[cfg(test)]
mod endpoint_tests {
    use std::collections::BTreeSet;

    use forge_platform_core::{EndpointRefusal, PlatformEndpoints, PlatformError};

    use super::endpoint_surfaces;

    const PRODUCTION: [(&str, &str); 13] = [
        ("FORGE_TWITCH_API_BASE_URL", "https://api.twitch.tv"),
        (
            "FORGE_TWITCH_EVENTSUB_WS_URL",
            "wss://eventsub.wss.twitch.tv/ws",
        ),
        ("FORGE_TWITCH_OAUTH_BASE_URL", "https://id.twitch.tv/oauth2"),
        ("FORGE_KICK_API_BASE_URL", "https://api.kick.com/public/v1"),
        ("FORGE_KICK_CHANNEL_API_BASE_URL", "https://kick.com/api/v2"),
        ("FORGE_KICK_CHAT_WS_BASE_URL", "wss://ws-us2.pusher.com/app"),
        ("FORGE_KICK_OAUTH_BASE_URL", "https://id.kick.com/oauth"),
        (
            "FORGE_YOUTUBE_API_BASE_URL",
            "https://www.googleapis.com/youtube/v3",
        ),
        (
            "FORGE_YOUTUBE_UPLOAD_BASE_URL",
            "https://www.googleapis.com/upload/youtube/v3",
        ),
        (
            "FORGE_YOUTUBE_OAUTH_BASE_URL",
            "https://oauth2.googleapis.com",
        ),
        (
            "FORGE_DONATELLO_API_BASE_URL",
            "https://donatello.to/api/v1",
        ),
        ("FORGE_MONOBANK_API_BASE_URL", "https://api.monobank.ua"),
        ("FORGE_DISCORD_API_BASE_URL", "https://discord.com/api"),
    ];

    #[test]
    fn every_surface_keeps_its_variable_and_production_url_in_roster_order() {
        let endpoints = PlatformEndpoints::default();
        let declared: Vec<(&str, &str)> = endpoint_surfaces()
            .into_iter()
            .map(|surface| (surface.env_var(), endpoints.base_url(surface)))
            .collect::<Vec<_>>();
        assert_eq!(declared, PRODUCTION);
    }

    #[test]
    fn each_surface_reads_a_distinct_variable() {
        let surfaces = endpoint_surfaces();
        let variables: BTreeSet<&str> = surfaces.iter().map(|s| s.env_var()).collect();
        assert_eq!(variables.len(), surfaces.len());
    }

    #[test]
    fn the_socket_surfaces_refuse_an_http_override_and_the_rest_a_socket_one() {
        for surface in endpoint_surfaces() {
            let is_socket = surface.env_var().contains("_WS_");
            let wrong_scheme = if is_socket {
                "http://127.0.0.1"
            } else {
                "ws://127.0.0.1"
            };
            let result = PlatformEndpoints::resolve(&[surface], |_| Some(wrong_scheme.into()));
            assert!(
                matches!(
                    result,
                    Err(PlatformError::EndpointOverrideRefused {
                        reason: EndpointRefusal::SchemeMismatch,
                        ..
                    })
                ),
                "{}",
                surface.env_var()
            );
        }
    }
}
