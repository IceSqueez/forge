use forge_platform_core::IntegrationDeclaration;
use forge_types::{IntegrationId, MessageCatalog};

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

pub fn declaration_of(id: &IntegrationId) -> Option<IntegrationDeclaration> {
    declarations()
        .into_iter()
        .find(|declaration| &declaration.id == id)
}
