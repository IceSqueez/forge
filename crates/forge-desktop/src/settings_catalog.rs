use forge_storage::SettingDisclosure::{Verbatim, Withheld};
use forge_storage::SettingPortability::Portable;
use forge_storage::settings::DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE;
use forge_storage::{Flag, OptionalText, Setting, SettingSpec, SettingsCatalog};

use crate::audio_routes::{CLIPS_ROUTE, SPEECH_ROUTE};
use crate::donation_services::DONATELLO_POLL_INTERVAL_SECS;
use crate::first_run::FIRST_RUN_COMPLETED;
use crate::midi_screen::MIDI_KNOWN_DEVICES;
use crate::obs_credentials_form::{OBS_AUTO_RECONNECT, OBS_CONNECT_ON_LAUNCH};
use crate::shortcut_overrides::KEYBOARD_SHORTCUTS;
use crate::stay_awake::STAY_AWAKE;
use crate::update_leftover::LEFTOVER_INSTALL_NOTICED;
use crate::update_report::ROLLED_BACK_VERSION;
use crate::vtube_connect_form::{VTUBE_AUTO_RECONNECT, VTUBE_CONNECT_ON_LAUNCH};
use crate::window_placement::WINDOW_PLACEMENT;

pub(crate) const DENSITY: Setting<OptionalText> =
    Setting::new("density", OptionalText, Portable, Verbatim);

pub(crate) const SCHEDULED_BACKUP_FAILURE_OS_NOTICE: Setting<Flag> = Setting::new(
    "notifications.scheduled_backup_failure_os_notice",
    Flag {
        default: DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE,
    },
    Portable,
    Withheld,
);

pub(crate) const SETTING_SPECS: &[SettingSpec] = &[
    DENSITY.spec(),
    KEYBOARD_SHORTCUTS.spec(),
    SPEECH_ROUTE.spec(),
    CLIPS_ROUTE.spec(),
    FIRST_RUN_COMPLETED.spec(),
    STAY_AWAKE.spec(),
    WINDOW_PLACEMENT.spec(),
    ROLLED_BACK_VERSION.spec(),
    LEFTOVER_INSTALL_NOTICED.spec(),
    OBS_AUTO_RECONNECT.spec(),
    OBS_CONNECT_ON_LAUNCH.spec(),
    VTUBE_AUTO_RECONNECT.spec(),
    VTUBE_CONNECT_ON_LAUNCH.spec(),
    MIDI_KNOWN_DEVICES.spec(),
    DONATELLO_POLL_INTERVAL_SECS.spec(),
    SCHEDULED_BACKUP_FAILURE_OS_NOTICE.spec(),
];

const OWNER_SPECS: &[&[SettingSpec]] = &[
    forge_script::settings::SETTING_SPECS,
    forge_soundboard::settings::SETTING_SPECS,
    forge_hotkey::SETTING_SPECS,
    forge_server::settings::SETTING_SPECS,
    forge_runtime::settings::SETTING_SPECS,
    SETTING_SPECS,
];

pub(crate) fn registered_settings_catalog() -> SettingsCatalog {
    let mut catalog = SettingsCatalog::core();
    for specs in OWNER_SPECS {
        if let Err(error) = catalog.register(specs) {
            tracing::error!(%error, "owner settings specs left unregistered");
        }
    }
    catalog
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use forge_storage::settings::core_settings;
    use forge_storage::test_support::{MemorySettings, StoredSettingRow};
    use forge_storage::{
        DataProvider, RowClasses, SettingDisclosure, SettingKey, SettingPortability, SettingSkip,
        SettingSpec, SettingsRepo, load_setting_rows, write_setting,
    };

    use super::{DENSITY, registered_settings_catalog};
    use crate::test_support::sandboxed_backend;

    #[test]
    fn a_density_write_stamps_the_declared_row_classes() {
        let memory = MemorySettings::new();

        crate::test_support::runtime()
            .block_on(write_setting(
                &memory,
                &DENSITY,
                &Some("compact".to_owned()),
            ))
            .unwrap();

        assert_eq!(
            memory.row(DENSITY.name()),
            Some(StoredSettingRow {
                key: DENSITY.name().to_owned(),
                value: "compact".to_owned(),
                classes: RowClasses {
                    portability: SettingPortability::Portable,
                    disclosure: SettingDisclosure::Verbatim,
                },
            })
        );
    }

    const KEY_CATALOG_GOLDEN: &str = include_str!("../tests/fixtures/settings/key_catalog.tsv");
    const TEST_KEY: [u8; 32] = [0x5e; 32];
    const SECRET: &str = "nova_the_broadcaster";

    fn portability_label(portability: SettingPortability) -> &'static str {
        match portability {
            SettingPortability::Portable => "portable",
            SettingPortability::MachineLocal => "machine_local",
            SettingPortability::RepositoryOwned => "repository_owned",
        }
    }

    fn disclosure_label(disclosure: SettingDisclosure) -> &'static str {
        match disclosure {
            SettingDisclosure::Verbatim => "verbatim",
            SettingDisclosure::Kind => "kind",
            SettingDisclosure::Presence => "presence",
            SettingDisclosure::Withheld => "withheld",
        }
    }

    fn owner_slices() -> [(&'static str, &'static [SettingSpec]); 6] {
        [
            ("script", forge_script::settings::SETTING_SPECS),
            ("soundboard", forge_soundboard::settings::SETTING_SPECS),
            ("hotkey", forge_hotkey::SETTING_SPECS),
            ("server", forge_server::settings::SETTING_SPECS),
            ("runtime", forge_runtime::settings::SETTING_SPECS),
            ("desktop", super::SETTING_SPECS),
        ]
    }

    #[test]
    fn every_registered_key_keeps_its_name_portability_and_disclosure_class() {
        let mut lines: Vec<String> = registered_settings_catalog()
            .specs()
            .iter()
            .map(|spec| {
                let (shape, name) = match spec.key {
                    SettingKey::Exact(name) => ("key", name),
                    SettingKey::Prefix(name) => ("prefix", name),
                };
                format!(
                    "{shape}\t{name}\t{}\t{}",
                    portability_label(spec.portability),
                    disclosure_label(spec.disclosure)
                )
            })
            .collect();
        lines.sort();

        assert_eq!(format!("{}\n", lines.join("\n")), KEY_CATALOG_GOLDEN);
    }

    #[test]
    fn the_registered_catalog_holds_every_owner_spec_unchanged() {
        let catalog = registered_settings_catalog();
        let mut missing = Vec::new();
        for (owner, specs) in owner_slices() {
            for spec in specs {
                let row = catalog
                    .specs()
                    .iter()
                    .find(|row| row.key.name() == spec.key.name());
                if row != Some(spec) {
                    missing.push(format!("{owner}: {}", spec.key.name()));
                }
            }
        }

        assert_eq!(missing, Vec::<String>::new());
    }

    #[test]
    fn the_registered_catalog_declares_no_key_twice() {
        let catalog = registered_settings_catalog();
        let mut seen = BTreeSet::new();

        let repeated: Vec<&str> = catalog
            .specs()
            .iter()
            .map(|spec| spec.key.name())
            .filter(|name| !seen.insert(*name))
            .collect();

        assert_eq!(repeated, Vec::<&str>::new());
    }

    #[tokio::test]
    async fn every_setting_a_fresh_database_and_the_first_run_write_is_declared() {
        let backend = sandboxed_backend("sqlite::memory:", TEST_KEY).await;
        let settings: &dyn SettingsRepo = &*backend;
        crate::first_run::record_completed(settings).await.unwrap();
        let catalog = registered_settings_catalog();

        let undeclared: Vec<String> = load_setting_rows(settings)
            .await
            .unwrap()
            .into_iter()
            .filter(|row| {
                matches!(
                    catalog.admit(row.key(), row.value()),
                    Err(SettingSkip::Undeclared)
                )
            })
            .map(|row| row.key().to_owned())
            .collect();

        assert_eq!(undeclared, Vec::<String>::new());
    }

    #[test]
    fn keys_that_can_name_a_host_a_path_a_device_or_an_import_are_never_disclosed_verbatim() {
        let catalog = registered_settings_catalog();
        for name in [
            core_settings::SERVER_OVERLAY_ROOT.name(),
            forge_server::settings::BIND_ADDRESS.name(),
            forge_server::settings::ADDITIONAL_ORIGINS.name(),
            core_settings::AUDIO_OUTPUT_DEVICE_ID.name(),
            core_settings::AUDIO_VOICE_GATE_INPUT_DEVICE_ID.name(),
            forge_soundboard::settings::OUTPUT_DEVICE.name(),
            core_settings::FONT_BODY.name(),
            core_settings::FONT_MONO.name(),
            forge_script::settings::HTTP_ALLOWED_DOMAINS.name(),
            core_settings::PICKER_FAVORITES_SUB_ACTIONS.name(),
            core_settings::PICKER_FAVORITES_TRIGGERS.name(),
            core_settings::PICKER_FAVORITES_ICONS.name(),
            crate::midi_screen::MIDI_KNOWN_DEVICES.name(),
            crate::window_placement::WINDOW_PLACEMENT.name(),
        ] {
            let shown = catalog
                .spec_of(name)
                .and_then(|spec| spec.disclosure.render(SECRET));

            assert!(
                !shown.is_some_and(|shown| shown.contains(SECRET)),
                "{name} disclosed its value"
            );
        }
    }

    #[test]
    fn network_exposure_http_trust_devices_and_machine_state_never_travel() {
        let catalog = registered_settings_catalog();

        let portable: Vec<&str> = [
            forge_server::settings::BIND_ADDRESS.name(),
            forge_server::settings::LAN_BIND_ENABLED.name(),
            forge_server::settings::AUTH_REQUIRED_FOR_READS.name(),
            forge_server::settings::HTTP_OVERLAY_REQUIRE_TOKEN.name(),
            forge_server::settings::OVERLAY_CORS_ANY_ORIGIN.name(),
            forge_server::settings::ADDITIONAL_ORIGINS.name(),
            core_settings::SERVER_OVERLAY_ROOT.name(),
            forge_script::settings::HTTP_ALLOWED_DOMAINS.name(),
            forge_script::settings::HTTP_ALLOW_LOCAL.name(),
            forge_runtime::settings::CORE_HTTP_ALLOW_LOCAL.name(),
            crate::window_placement::WINDOW_PLACEMENT.name(),
            core_settings::AUDIO_OUTPUT_DEVICE_ID.name(),
            core_settings::AUDIO_VOICE_GATE_INPUT_DEVICE_ID.name(),
            forge_soundboard::settings::OUTPUT_DEVICE.name(),
            crate::midi_screen::MIDI_KNOWN_DEVICES.name(),
            crate::update_report::ROLLED_BACK_VERSION.name(),
            crate::update_leftover::LEFTOVER_INSTALL_NOTICED.name(),
            core_settings::CREDENTIALS_KEY_LOSS.name(),
            crate::stay_awake::STAY_AWAKE.name(),
            crate::first_run::FIRST_RUN_COMPLETED.name(),
            core_settings::BACKUP_SCHEDULE_KEY.name(),
            core_settings::BACKUP_SCHEDULE_ENABLED.name(),
            core_settings::BACKUP_SCHEDULE_DESTINATION.name(),
            core_settings::BACKUP_SCHEDULE_LAST_SUCCESS_AT.name(),
            core_settings::BACKUP_SCHEDULE_CONTENT_FINGERPRINT.name(),
        ]
        .into_iter()
        .filter(|name| {
            catalog
                .spec_of(name)
                .is_some_and(|spec| spec.portability == SettingPortability::Portable)
        })
        .collect();

        assert_eq!(portable, Vec::<&str>::new());
    }

    async fn diagnostic_config(
        rows: &[(&str, &str, Option<forge_storage::RowClasses>)],
    ) -> std::collections::BTreeMap<String, String> {
        let sandboxed = sandboxed_backend("sqlite::memory:", TEST_KEY).await;
        for (name, value, classes) in rows {
            sandboxed
                .seed_foreign_setting(name, value, *classes)
                .await
                .unwrap();
        }
        let backend = sandboxed.map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        crate::diagnostic_bundle::gather_input(
            Arc::clone(&backend),
            Vec::new(),
            forge_types::LogLevel::Info,
            false,
        )
        .await
        .unwrap()
        .config
    }

    fn declared_classes(spec: &SettingSpec) -> Option<forge_storage::RowClasses> {
        Some(RowClasses::from(spec))
    }

    #[tokio::test]
    async fn the_diagnostic_bundle_discloses_rows_by_their_stored_class() {
        let config = diagnostic_config(&[
            (
                forge_server::settings::PORT.name(),
                "8080",
                declared_classes(&forge_server::settings::PORT.spec()),
            ),
            (
                core_settings::SERVER_OVERLAY_ROOT.name(),
                "/home/nova/overlays",
                declared_classes(&core_settings::SERVER_OVERLAY_ROOT.spec()),
            ),
            (
                forge_script::settings::HTTP_ALLOWED_DOMAINS.name(),
                r#"["nova.example"]"#,
                declared_classes(&forge_script::settings::HTTP_ALLOWED_DOMAINS.spec()),
            ),
            ("twitch.account_login", SECRET, None),
            (
                "ui.last_note",
                "stored-verbatim",
                Some(forge_storage::RowClasses {
                    portability: forge_storage::SettingPortability::MachineLocal,
                    disclosure: forge_storage::SettingDisclosure::Verbatim,
                }),
            ),
        ])
        .await;

        assert_eq!(
            [
                forge_server::settings::PORT.name(),
                core_settings::SERVER_OVERLAY_ROOT.name(),
                forge_script::settings::HTTP_ALLOWED_DOMAINS.name(),
                "twitch.account_login",
                "ui.last_note",
            ]
            .map(|name| config.get(name).cloned()),
            [
                Some("8080".to_owned()),
                Some("set".to_owned()),
                Some("list(1)".to_owned()),
                None,
                Some("stored-verbatim".to_owned()),
            ]
        );
    }

    #[tokio::test]
    async fn a_classless_legacy_row_is_withheld_from_the_diagnostic_bundle_though_its_key_is_declared_verbatim()
     {
        let port = forge_server::settings::PORT.name();
        let config = diagnostic_config(&[(port, SECRET, None)]).await;

        assert_eq!(config.get(port), None);
    }
}
