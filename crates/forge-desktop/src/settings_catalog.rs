use forge_storage::SettingDisclosure::{Kind, Verbatim, Withheld};
use forge_storage::SettingEncoding::{Flag as FlagEncoding, Integer, Json, Text};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::settings::DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE;
use forge_storage::{
    Flag, OptionalText, Setting, SettingSpec, SettingsCatalog, reserved_keys as key,
};

use crate::first_run::FIRST_RUN_COMPLETED_KEY;
use crate::shortcut_overrides::KEYBOARD_SHORTCUTS;

pub const DENSITY: Setting<OptionalText> =
    Setting::new("density", OptionalText, Portable, Verbatim);

pub const SCHEDULED_BACKUP_FAILURE_OS_NOTICE: Setting<Flag> = Setting::new(
    "notifications.scheduled_backup_failure_os_notice",
    Flag {
        default: DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE,
    },
    Portable,
    Withheld,
);

pub const SETTING_SPECS: &[SettingSpec] = &[
    DENSITY.spec(),
    KEYBOARD_SHORTCUTS.spec(),
    SettingSpec::exact(key::AUDIO_SPEECH_ROUTE, Text, Portable, Verbatim),
    SettingSpec::exact(key::AUDIO_CLIPS_ROUTE, Text, Portable, Verbatim),
    SettingSpec::exact(
        FIRST_RUN_COMPLETED_KEY,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(key::STAY_AWAKE, FlagEncoding, MachineLocal, Verbatim),
    SettingSpec::exact(key::WINDOW_PLACEMENT, Json, MachineLocal, Kind),
    SettingSpec::exact(
        key::UPDATES_ROLLED_BACK_VERSION,
        Text,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(
        key::UPDATES_LEFTOVER_INSTALL_NOTICED,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(key::OBS_AUTO_RECONNECT, FlagEncoding, Portable, Withheld),
    SettingSpec::exact(key::OBS_CONNECT_ON_LAUNCH, FlagEncoding, Portable, Withheld),
    SettingSpec::exact(key::VTUBE_AUTO_RECONNECT, FlagEncoding, Portable, Withheld),
    SettingSpec::exact(
        key::VTUBE_CONNECT_ON_LAUNCH,
        FlagEncoding,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(key::MIDI_KNOWN_DEVICES, Json, MachineLocal, Withheld),
    SettingSpec::exact(
        key::DONATELLO_POLL_INTERVAL_SECS,
        Integer,
        Portable,
        Withheld,
    ),
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

pub fn registered_settings_catalog() -> SettingsCatalog {
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

    use forge_storage::{
        DataProvider, SettingDisclosure, SettingKey, SettingPortability, SettingSpec, SettingsRepo,
        report_undeclared_settings, reserved_keys as key,
    };

    use super::registered_settings_catalog;
    use crate::test_support::sandboxed_backend;

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

        let undeclared = report_undeclared_settings(settings, &registered_settings_catalog())
            .await
            .unwrap();

        assert_eq!(undeclared, Vec::<String>::new());
    }

    #[test]
    fn keys_that_can_name_a_host_a_path_a_device_or_an_import_are_never_disclosed_verbatim() {
        let catalog = registered_settings_catalog();
        for name in [
            key::SERVER_OVERLAY_ROOT,
            key::SERVER_BIND_ADDRESS,
            key::SERVER_ADDITIONAL_ORIGINS,
            key::AUDIO_OUTPUT_DEVICE_ID,
            key::AUDIO_VOICE_GATE_INPUT_DEVICE_ID,
            forge_soundboard::settings::OUTPUT_DEVICE.name(),
            key::FONT_BODY,
            key::FONT_MONO,
            key::SCRIPT_HTTP_ALLOWED_DOMAINS,
            key::PICKER_FAVORITES_SUB_ACTIONS,
            key::PICKER_FAVORITES_TRIGGERS,
            key::PICKER_FAVORITES_ICONS,
            key::MIDI_KNOWN_DEVICES,
            key::WINDOW_PLACEMENT,
        ] {
            let shown = catalog.disclosure_of(name).render(SECRET);

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
            key::SERVER_BIND_ADDRESS,
            key::SERVER_LAN_BIND_ENABLED,
            key::SERVER_AUTH_REQUIRED_FOR_READS,
            key::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN,
            key::SERVER_OVERLAY_CORS_ANY_ORIGIN,
            key::SERVER_ADDITIONAL_ORIGINS,
            key::SERVER_OVERLAY_ROOT,
            key::SCRIPT_HTTP_ALLOWED_DOMAINS,
            key::SCRIPT_HTTP_ALLOW_LOCAL,
            key::CORE_HTTP_ALLOW_LOCAL,
            key::WINDOW_PLACEMENT,
            key::AUDIO_OUTPUT_DEVICE_ID,
            key::AUDIO_VOICE_GATE_INPUT_DEVICE_ID,
            forge_soundboard::settings::OUTPUT_DEVICE.name(),
            key::MIDI_KNOWN_DEVICES,
            key::UPDATES_ROLLED_BACK_VERSION,
            key::UPDATES_LEFTOVER_INSTALL_NOTICED,
            key::CREDENTIALS_KEY_LOSS,
            key::STAY_AWAKE,
            crate::first_run::FIRST_RUN_COMPLETED_KEY,
            key::BACKUP_SCHEDULE_KEY,
            key::BACKUP_SCHEDULE_ENABLED,
            key::BACKUP_SCHEDULE_DESTINATION,
            key::BACKUP_SCHEDULE_LAST_SUCCESS_AT,
            key::BACKUP_SCHEDULE_CONTENT_FINGERPRINT,
        ]
        .into_iter()
        .filter(|name| catalog.is_portable(name))
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

    fn declared_classes(name: &str) -> Option<forge_storage::RowClasses> {
        let catalog = registered_settings_catalog();
        Some(forge_storage::RowClasses {
            portability: catalog.portability_of(name),
            disclosure: catalog.disclosure_of(name),
        })
    }

    #[tokio::test]
    async fn the_diagnostic_bundle_discloses_rows_by_their_stored_class() {
        let config = diagnostic_config(&[
            (key::SERVER_PORT, "8080", declared_classes(key::SERVER_PORT)),
            (
                key::SERVER_OVERLAY_ROOT,
                "/home/nova/overlays",
                declared_classes(key::SERVER_OVERLAY_ROOT),
            ),
            (
                key::SCRIPT_HTTP_ALLOWED_DOMAINS,
                r#"["nova.example"]"#,
                declared_classes(key::SCRIPT_HTTP_ALLOWED_DOMAINS),
            ),
            (
                "twitch.account_login",
                SECRET,
                declared_classes("twitch.account_login"),
            ),
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
                key::SERVER_PORT,
                key::SERVER_OVERLAY_ROOT,
                key::SCRIPT_HTTP_ALLOWED_DOMAINS,
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
        let config = diagnostic_config(&[(key::SERVER_PORT, SECRET, None)]).await;

        assert_eq!(config.get(key::SERVER_PORT), None);
    }
}
