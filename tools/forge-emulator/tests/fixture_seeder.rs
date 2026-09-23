#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use forge_emulator::EmulatorError;
use forge_emulator::fixture::{
    ChatCommand, EventTrigger, Fixture, SeedReport, TwitchAccount, seed,
};
use forge_events::{Event, EventSource};
use forge_platform_twitch::TwitchCredentialsManager;
use forge_registry::TriggerRegistry;
use forge_server::{AuthState, ServerSettings};
use forge_storage::{CredentialsRepo, DataProvider, SettingsRepo, reserved_keys};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{SubActionStep, Variant};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

const EMULATOR: &str = env!("CARGO_BIN_EXE_forge-emulator");

async fn seed_fresh(fixture: &Fixture) -> (TempDir, SeedReport) {
    let dir = tempfile::tempdir().unwrap();
    let report = seed(Path::new(EMULATOR), dir.path(), fixture)
        .await
        .unwrap();
    (dir, report)
}

/// Decodes the key file with forge's on-disk format, so a key written anywhere else fails here.
async fn reopen(dir: &Path) -> SqliteBackend {
    let hex = std::fs::read_to_string(dir.join("credentials-key")).unwrap();
    let hex = hex.trim();
    assert_eq!(
        hex.len(),
        64,
        "credential key file holds {} chars",
        hex.len()
    );
    let mut key = [0u8; 32];
    for (byte, pair) in key.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
    }
    let url = format!("sqlite://{}?mode=rw", dir.join("forge.db").display());
    SqliteBackend::open_with_key(&url, key).await.unwrap()
}

fn log_step(message: &str) -> SubActionStep {
    SubActionStep {
        kind_id: "core.log.write".to_owned(),
        config: BTreeMap::from([("message".to_owned(), Variant::String(message.to_owned()))]),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect()
}

#[tokio::test]
async fn seeded_server_is_explicitly_enabled_on_the_reported_loopback_port() {
    let (dir, report) = seed_fresh(&Fixture::default()).await;
    let backend = reopen(dir.path()).await;
    let loaded = ServerSettings::load(&backend).await.unwrap();
    let stored = backend.load_all().await.unwrap();
    backend.shutdown().await;

    assert!(loaded.enabled);
    assert_eq!(loaded.bind_address, "127.0.0.1");
    assert_eq!(loaded.port, report.server.port);
    for key in [
        reserved_keys::SERVER_ENABLED,
        reserved_keys::SERVER_BIND_ADDRESS,
        reserved_keys::SERVER_PORT,
    ] {
        assert!(
            stored.contains_key(key),
            "{key} rides on forge's default instead of the fixture"
        );
    }
}

#[tokio::test]
async fn seeded_bearer_token_is_the_one_the_server_authenticates() {
    let (dir, report) = seed_fresh(&Fixture::default()).await;
    let backend = reopen(dir.path()).await;
    let auth = AuthState::load(false, &backend).await.unwrap();
    let accepted = auth.verify(&report.server.bearer_token).await;
    backend.shutdown().await;

    assert!(accepted);
}

#[tokio::test]
async fn seeded_twitch_credential_loads_with_the_fixture_identity_and_no_refresh_token() {
    let account = TwitchAccount {
        user_id: "4242".to_owned(),
        login: "fixture_login".to_owned(),
        access_token: "fixture-access-token".to_owned(),
        ..TwitchAccount::default()
    };
    let fixture = Fixture {
        twitch: Some(account.clone()),
        ..Fixture::default()
    };
    let (dir, report) = seed_fresh(&fixture).await;
    let backend = reopen(dir.path()).await;
    let stored = forge_platform_twitch::credentials::load(&backend)
        .await
        .unwrap()
        .expect("twitch credential is stored");
    backend.shutdown().await;

    assert_eq!(stored.access_token.expose(), account.access_token);
    assert_eq!(stored.user_id, account.user_id);
    assert_eq!(stored.login, account.login);
    assert!(
        stored.refresh_token.is_none(),
        "a refresh token lets forge call the real token endpoint"
    );
    let seeded = report.twitch.expect("report names the twitch account");
    assert_eq!(seeded.client_id, account.client_id);
}

#[tokio::test]
async fn seeded_twitch_access_token_is_served_without_entering_the_refresh_path() {
    let (dir, report) = seed_fresh(&Fixture::chat_command_mvp()).await;
    let backend = Arc::new(reopen(dir.path()).await);
    let client_id = report.twitch.expect("mvp fixture seeds twitch").client_id;
    let manager =
        TwitchCredentialsManager::new(Arc::clone(&backend) as Arc<dyn CredentialsRepo>, client_id);
    let served = manager.get_valid_access_token().await;
    backend.shutdown().await;

    let token = served.expect("a token inside the refresh buffer would demand re-authorization");
    assert_eq!(token.expose(), TwitchAccount::default().access_token);
}

#[tokio::test]
async fn server_only_fixture_stores_no_twitch_credential() {
    let (dir, report) = seed_fresh(&Fixture::default()).await;
    let backend = reopen(dir.path()).await;
    let stored = forge_platform_twitch::credentials::load(&backend)
        .await
        .unwrap();
    backend.shutdown().await;

    assert!(stored.is_none());
    assert!(report.twitch.is_none());
}

#[tokio::test]
async fn seeded_chat_command_action_is_enabled_on_the_default_queue_with_its_steps() {
    let steps = vec![log_step("pong"), log_step("second")];
    let fixture = Fixture {
        chat_commands: vec![ChatCommand {
            phrase: "!ping".to_owned(),
            action_name: "Ping".to_owned(),
            steps: steps.clone(),
        }],
        ..Fixture::default()
    };
    let (dir, report) = seed_fresh(&fixture).await;
    let backend = reopen(dir.path()).await;
    let action = backend
        .action_repo()
        .get(report.chat_commands[0].action_id)
        .await
        .unwrap()
        .expect("action is stored");
    let default_queue = backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .unwrap()
        .expect("migrations create the Default queue");
    backend.shutdown().await;

    assert_eq!(action.name, "Ping");
    assert!(action.enabled);
    assert_eq!(action.queue_id, default_queue.id);
    assert_eq!(action.sub_actions, steps);
}

#[tokio::test]
async fn seeded_chat_command_trigger_is_linked_to_its_action_and_matched_by_forge() {
    let (dir, report) = seed_fresh(&Fixture::chat_command_mvp()).await;
    let seeded = &report.chat_commands[0];
    let backend = reopen(dir.path()).await;
    let linked = backend
        .trigger_instance_repo()
        .list_for_action(seeded.action_id)
        .await
        .unwrap();
    backend.shutdown().await;

    let [instance] = linked.as_slice() else {
        panic!("expected exactly one linked trigger, got {linked:?}");
    };
    assert_eq!(instance.id, seeded.trigger_instance_id);
    assert!(instance.enabled);

    let mut registry = TriggerRegistry::new();
    forge_platform_twitch::register_twitch_triggers(&mut registry).unwrap();
    let descriptor = registry
        .get(&instance.kind_id)
        .unwrap_or_else(|| panic!("forge registers no trigger kind `{}`", instance.kind_id));
    let chat = Event::new(
        EventSource::Twitch,
        "chat.message",
        serde_json::json!({ "message": "!PING with arguments" }),
    );
    assert!(descriptor.matches_trigger(&instance.overrides, &chat));
}

#[tokio::test]
async fn seeding_an_already_seeded_directory_is_refused_and_keeps_the_first_seed() {
    let (dir, first) = seed_fresh(&Fixture::default()).await;
    let second = seed(Path::new(EMULATOR), dir.path(), &Fixture::default()).await;
    assert!(
        matches!(second, Err(EmulatorError::DataDirNotEmpty { .. })),
        "got {second:?}"
    );

    let backend = reopen(dir.path()).await;
    let auth = AuthState::load(false, &backend).await.unwrap();
    let first_still_accepted = auth.verify(&first.server.bearer_token).await;
    backend.shutdown().await;
    assert!(first_still_accepted);
}

#[tokio::test]
async fn seed_returns_with_the_database_cleanly_closed() {
    let (dir, _report) = seed_fresh(&Fixture::chat_command_mvp()).await;
    let leftovers: Vec<PathBuf> = entries(dir.path())
        .into_iter()
        .filter(|path| {
            let name = path.file_name().unwrap().to_string_lossy();
            name.ends_with("-wal") || name.ends_with("-shm")
        })
        .collect();
    assert!(leftovers.is_empty(), "open database files: {leftovers:?}");
}

enum Refusal {
    DataDirUnset,
    NonEmptyDir,
    KeyFileOverride,
    GarbageFixture,
    EmptyPhrase,
}

#[tokio::test]
async fn seeder_process_refuses_to_write_outside_a_fresh_fixture_environment() {
    let valid = serde_json::to_vec(&Fixture::chat_command_mvp()).unwrap();
    let empty_phrase = serde_json::to_vec(&Fixture {
        chat_commands: vec![ChatCommand {
            phrase: String::new(),
            action_name: "Silent".to_owned(),
            steps: Vec::new(),
        }],
        ..Fixture::default()
    })
    .unwrap();

    for case in [
        Refusal::DataDirUnset,
        Refusal::NonEmptyDir,
        Refusal::KeyFileOverride,
        Refusal::GarbageFixture,
        Refusal::EmptyPhrase,
    ] {
        let data_dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut command = tokio::process::Command::new(EMULATOR);
        command
            .arg("seed")
            .env("HOME", home.path())
            .env("XDG_DATA_HOME", home.path())
            .env("APPDATA", home.path())
            .env("FORGE_DATA_DIR", data_dir.path())
            .env_remove("FORGE_CREDENTIAL_KEY_FILE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut input = valid.as_slice();
        let mut expected_entries = 0;
        let key_file = home.path().join("elsewhere-credentials-key");
        let label = match case {
            Refusal::DataDirUnset => {
                command.env_remove("FORGE_DATA_DIR");
                "unset data dir"
            }
            Refusal::NonEmptyDir => {
                std::fs::write(data_dir.path().join("stray"), b"").unwrap();
                expected_entries = 1;
                "non-empty data dir"
            }
            Refusal::KeyFileOverride => {
                command.env("FORGE_CREDENTIAL_KEY_FILE", &key_file);
                "key file override"
            }
            Refusal::GarbageFixture => {
                input = b"{ not json";
                "garbage fixture"
            }
            Refusal::EmptyPhrase => {
                input = empty_phrase.as_slice();
                "empty phrase"
            }
        };

        let mut child = command.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input).await.unwrap();
        drop(stdin);
        let output = child.wait_with_output().await.unwrap();

        assert!(
            !output.status.success(),
            "{label}: seeder exited successfully"
        );
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("bearer_token"),
            "{label}: a refused seed printed a report"
        );
        assert_eq!(
            entries(data_dir.path()).len(),
            expected_entries,
            "{label}: fixture directory was written"
        );
        assert!(
            entries(home.path()).is_empty(),
            "{label}: something was written under the user's data directory"
        );
    }
}

fn overlay_send_step(target: &str, headline: &str) -> SubActionStep {
    SubActionStep {
        kind_id: "overlay.send".to_owned(),
        config: BTreeMap::from([
            ("overlay_id".to_owned(), Variant::String(target.to_owned())),
            ("headline".to_owned(), Variant::String(headline.to_owned())),
        ]),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn alert_fixture() -> Fixture {
    Fixture {
        overlays: vec![forge_emulator::fixture::OverlayFixture {
            display_name: "Alert Box".to_owned(),
            kind_id: "overlay.alert".to_owned(),
            config: BTreeMap::from([(
                "headline".to_owned(),
                Variant::String("stored headline".to_owned()),
            )]),
        }],
        chat_commands: vec![ChatCommand {
            phrase: "!alert".to_owned(),
            action_name: "Raise Alert".to_owned(),
            steps: vec![overlay_send_step(
                "Alert Box",
                "%user_login% raised an alert",
            )],
        }],
        ..Fixture::default()
    }
}

#[tokio::test]
async fn a_seeded_overlay_is_enabled_and_stored_with_the_fixture_config_under_its_minted_identity()
{
    let (dir, report) = seed_fresh(&alert_fixture()).await;
    let seeded = &report.overlays[0];
    let backend = reopen(dir.path()).await;
    let stored = backend
        .overlay_repo()
        .get(&forge_storage::OverlayId::new(&seeded.id))
        .await
        .unwrap()
        .expect("the overlay row is stored");
    backend.shutdown().await;

    assert_eq!(seeded.display_name, "Alert Box");
    assert_eq!(stored.kind_id, "overlay.alert");
    assert!(
        stored.enabled,
        "a disabled overlay refuses its own page credential, so nothing could connect"
    );
    assert_eq!(
        stored.config.get("headline"),
        Some(&Variant::String("stored headline".to_owned()))
    );
}

#[tokio::test]
async fn the_page_credential_the_report_names_is_the_one_the_overlay_row_answers_to() {
    let (dir, report) = seed_fresh(&alert_fixture()).await;
    let seeded = &report.overlays[0];
    let backend = reopen(dir.path()).await;
    let found = backend
        .overlay_repo()
        .get_by_credential(&forge_storage::OverlayCredential::new(&seeded.credential))
        .await
        .unwrap()
        .expect("forge resolves a page by the credential the report carries");
    backend.shutdown().await;

    assert_eq!(found.id.as_str(), seeded.id);
}

#[tokio::test]
async fn an_overlay_send_step_reaches_storage_addressed_by_identity_rather_than_display_name() {
    let (dir, report) = seed_fresh(&alert_fixture()).await;
    let seeded = report.overlays[0].clone();
    let backend = reopen(dir.path()).await;
    let action = backend
        .action_repo()
        .get(report.chat_commands[0].action_id)
        .await
        .unwrap()
        .expect("action is stored");
    backend.shutdown().await;

    assert_eq!(
        action.sub_actions[0].config.get("overlay_id"),
        Some(&Variant::String(seeded.id.clone())),
        "forge answers to the minted slug; a display name would leave the step addressing nothing"
    );
    assert_ne!(
        seeded.id, seeded.display_name,
        "this assertion is only meaningful while the two differ"
    );
    assert_eq!(
        action.sub_actions[0].config.get("headline"),
        Some(&Variant::String("%user_login% raised an alert".to_owned())),
        "rewriting the target must leave every other config value untouched"
    );
}

#[tokio::test]
async fn an_overlay_type_this_build_does_not_carry_is_refused_rather_than_stored() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Fixture {
        overlays: vec![forge_emulator::fixture::OverlayFixture {
            display_name: "Alert Box".to_owned(),
            kind_id: "vendor.unshipped".to_owned(),
            config: BTreeMap::new(),
        }],
        ..Fixture::default()
    };

    let refusal = seed(Path::new(EMULATOR), dir.path(), &fixture).await;

    assert!(
        matches!(&refusal, Err(EmulatorError::SeederProcess { reason }) if reason.contains("vendor.unshipped")),
        "got {refusal:?}"
    );
}

fn subscriber_trigger(steps: Vec<SubActionStep>) -> EventTrigger {
    EventTrigger {
        trigger_kind: "twitch.support.subscriber".to_owned(),
        action_name: "Announce Subscriber".to_owned(),
        config: BTreeMap::from([("tier".to_owned(), Variant::String("1000".to_owned()))]),
        steps,
    }
}

#[tokio::test]
async fn seeded_event_trigger_is_the_named_descriptor_carrying_its_config_and_bound_to_its_action()
{
    let fixture = Fixture {
        event_triggers: vec![subscriber_trigger(vec![log_step("a sub")])],
        ..Fixture::default()
    };
    let (dir, report) = seed_fresh(&fixture).await;
    let seeded = &report.event_triggers[0];
    let backend = reopen(dir.path()).await;
    let linked = backend
        .trigger_instance_repo()
        .list_for_action(seeded.action_id)
        .await
        .unwrap();
    backend.shutdown().await;

    let [instance] = linked.as_slice() else {
        panic!("expected exactly one linked trigger, got {linked:?}");
    };
    assert_eq!(instance.id, seeded.trigger_instance_id);
    assert!(instance.enabled);
    assert_eq!(
        instance.overrides.get("tier"),
        Some(&Variant::String("1000".to_owned()))
    );

    let mut registry = TriggerRegistry::new();
    forge_platform_twitch::register_twitch_triggers(&mut registry).unwrap();
    assert!(
        registry.get(&instance.kind_id).is_some(),
        "`{}` is no trigger descriptor id; trigger_instances never store event kinds",
        instance.kind_id
    );
}

#[tokio::test]
async fn an_event_triggers_overlay_send_step_reaches_storage_addressed_by_the_minted_identity() {
    let fixture = Fixture {
        overlays: vec![forge_emulator::fixture::OverlayFixture {
            display_name: "Alert Box".to_owned(),
            kind_id: "overlay.alert".to_owned(),
            config: BTreeMap::new(),
        }],
        event_triggers: vec![subscriber_trigger(vec![overlay_send_step(
            "Alert Box",
            "%user_name% just subscribed",
        )])],
        ..Fixture::default()
    };
    let (dir, report) = seed_fresh(&fixture).await;
    let minted = report.overlays[0].id.clone();
    let backend = reopen(dir.path()).await;
    let action = backend
        .action_repo()
        .get(report.event_triggers[0].action_id)
        .await
        .unwrap()
        .expect("action is stored");
    backend.shutdown().await;

    assert_eq!(
        action.sub_actions[0].config.get("overlay_id"),
        Some(&Variant::String(minted.clone())),
        "forge answers to the minted slug; a display name would leave the step addressing nothing"
    );
    assert_ne!(minted, "Alert Box");
}
