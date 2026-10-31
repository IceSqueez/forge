use forge_types::IntegrationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetailExtension {
    TwitchBotSection,
    ObsSettingsModal,
}

fn registered_extensions() -> [(IntegrationId, DetailExtension); 2] {
    [
        (
            forge_platform_twitch::TWITCH_INTEGRATION.id,
            DetailExtension::TwitchBotSection,
        ),
        (
            forge_obs::OBS_INTEGRATION.id,
            DetailExtension::ObsSettingsModal,
        ),
    ]
}

pub(crate) fn has_extension(id: &IntegrationId, extension: DetailExtension) -> bool {
    registered_extensions()
        .iter()
        .any(|(owner, registered)| owner == id && *registered == extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_extension_belongs_to_its_one_integration() {
        let twitch = forge_platform_twitch::TWITCH_INTEGRATION.id;
        let obs = forge_obs::OBS_INTEGRATION.id;
        let kick = forge_platform_kick::KICK_INTEGRATION.id;
        for (id, extension, expected) in [
            (&twitch, DetailExtension::TwitchBotSection, true),
            (&twitch, DetailExtension::ObsSettingsModal, false),
            (&obs, DetailExtension::ObsSettingsModal, true),
            (&obs, DetailExtension::TwitchBotSection, false),
            (&kick, DetailExtension::TwitchBotSection, false),
            (&kick, DetailExtension::ObsSettingsModal, false),
        ] {
            assert_eq!(has_extension(id, extension), expected, "{id} {extension:?}");
        }
    }
}
