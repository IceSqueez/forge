use std::time::{Duration, SystemTime};

use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_platform_core::paths;
use forge_platform_twitch::credentials::{StoredCredential, store_credential};
use forge_server::ServerSettings;
use forge_storage::{CredentialId, DataProvider, SettingsRepo, StorageError};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ExecutionMode, OAuthToken, PermissionRung, PlatformScope, SubActionStep,
    TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};
use rand::Rng as _;

use super::data_dir::{DATA_DIR_VARIABLE, ForgeDataDir, KEY_FILE_VARIABLE};
use super::port::free_loopback_port;
use super::report::{
    SeedReport, SeededCommand, SeededEventTrigger, SeededOverlay, SeededServer, SeededTwitch,
};
use super::spec::{
    ChatCommand, EventTrigger, Fixture, OVERLAY_SEND_KIND, OVERLAY_TARGET_KEY, OverlayFixture,
    TwitchAccount,
};
use crate::EmulatorError;

const DATABASE_FILE: &str = "forge.db";
const SERVER_BEARER_CREDENTIAL: &str = "server:bearer";
const SERVER_BIND_ADDRESS: &str = "127.0.0.1";
const DEFAULT_QUEUE: &str = "Default";
const CHAT_COMMAND_TRIGGER_KIND: &str = "twitch.chat.command";
const TWITCH_TOKEN_LIFETIME: Duration = Duration::from_secs(10 * 365 * 24 * 60 * 60);
const BEARER_TOKEN_BYTES: usize = 32;

/// Closes the storage pool and stops the retention pruner before returning, on failure too.
pub async fn seed_forge_environment(fixture: &Fixture) -> Result<SeedReport, EmulatorError> {
    fixture.validate()?;
    let data_dir = ForgeDataDir::from_forge_environment(
        std::env::var_os(DATA_DIR_VARIABLE),
        std::env::var_os(KEY_FILE_VARIABLE),
    )?;
    let port = free_loopback_port()?;
    let bearer_token = generate_bearer_token();

    let database = paths::data_dir().join(DATABASE_FILE);
    let url = format!("sqlite://{}?mode=rwc", database.display());
    let backend = SqliteBackend::open(&url)
        .await
        .map_err(|e| storage_error(e.into()))?;
    let written = write_fixture(&backend, fixture, port, &bearer_token).await;
    backend.shutdown().await;
    let Written {
        twitch,
        overlays,
        chat_commands,
        event_triggers,
    } = written?;

    Ok(SeedReport {
        data_dir: data_dir.path().to_owned(),
        server: SeededServer { port, bearer_token },
        twitch,
        overlays,
        chat_commands,
        event_triggers,
    })
}

struct Written {
    twitch: Option<SeededTwitch>,
    overlays: Vec<SeededOverlay>,
    chat_commands: Vec<SeededCommand>,
    event_triggers: Vec<SeededEventTrigger>,
}

async fn write_fixture(
    provider: &dyn DataProvider,
    fixture: &Fixture,
    port: u16,
    bearer_token: &str,
) -> Result<Written, EmulatorError> {
    let settings: &dyn SettingsRepo = provider;
    ServerSettings::save_enabled(settings, true)
        .await
        .map_err(storage_error)?;
    ServerSettings::save_bind_address(settings, SERVER_BIND_ADDRESS)
        .await
        .map_err(storage_error)?;
    ServerSettings::save_port(settings, port)
        .await
        .map_err(storage_error)?;
    provider
        .store(&CredentialId::new(SERVER_BEARER_CREDENTIAL), bearer_token)
        .await
        .map_err(storage_error)?;

    let twitch = match &fixture.twitch {
        Some(account) => Some(seed_twitch_account(provider, account).await?),
        None => None,
    };
    let kinds = builtin_overlay_kinds()?;
    let mut overlays = Vec::with_capacity(fixture.overlays.len());
    for overlay in &fixture.overlays {
        overlays.push(seed_overlay(provider, &kinds, overlay).await?);
    }
    let mut chat_commands = Vec::with_capacity(fixture.chat_commands.len());
    for command in &fixture.chat_commands {
        chat_commands.push(seed_chat_command(provider, command, &overlays).await?);
    }
    let mut event_triggers = Vec::with_capacity(fixture.event_triggers.len());
    for trigger in &fixture.event_triggers {
        event_triggers.push(seed_event_trigger(provider, trigger, &overlays).await?);
    }
    Ok(Written {
        twitch,
        overlays,
        chat_commands,
        event_triggers,
    })
}

fn builtin_overlay_kinds() -> Result<OverlayKindRegistry, EmulatorError> {
    let mut kinds = OverlayKindRegistry::new();
    register_builtin_kinds(&mut kinds).map_err(|e| EmulatorError::InvalidFixture {
        reason: format!("overlay types could not be registered: {e}"),
    })?;
    Ok(kinds)
}

/// The repository mints the identity slug and the page credential; the fixture only supplies the
/// name, the type and the stored config.
async fn seed_overlay(
    provider: &dyn DataProvider,
    kinds: &OverlayKindRegistry,
    overlay: &OverlayFixture,
) -> Result<SeededOverlay, EmulatorError> {
    let descriptor = kinds
        .get(&overlay.kind_id)
        .ok_or_else(|| EmulatorError::InvalidFixture {
            reason: format!(
                "overlay `{}` needs overlay type `{}`, which this build does not carry",
                overlay.display_name, overlay.kind_id
            ),
        })?;
    let repo = provider.overlay_repo();
    let mut definition = repo
        .create(
            &overlay.display_name,
            &overlay.kind_id,
            descriptor.config_schema_version(),
        )
        .await
        .map_err(storage_error)?;
    definition.config = overlay.config.clone();
    repo.save(&definition).await.map_err(storage_error)?;

    Ok(SeededOverlay {
        id: definition.id.as_str().to_owned(),
        display_name: overlay.display_name.clone(),
        kind_id: overlay.kind_id.clone(),
        credential: definition.credential.as_str().to_owned(),
    })
}

/// Rewrites every `overlay.send` target from the fixture's display name to the identity the
/// repository minted, which is the only name forge answers to.
fn addressed_steps(steps: &[SubActionStep], overlays: &[SeededOverlay]) -> Vec<SubActionStep> {
    steps
        .iter()
        .cloned()
        .map(|mut step| {
            if step.kind_id != OVERLAY_SEND_KIND {
                return step;
            }
            let identity = step
                .config
                .get(OVERLAY_TARGET_KEY)
                .and_then(Variant::as_str)
                .and_then(|target| {
                    overlays
                        .iter()
                        .find(|overlay| overlay.display_name == target)
                })
                .map(|overlay| overlay.id.clone());
            if let Some(identity) = identity {
                step.config
                    .insert(OVERLAY_TARGET_KEY.to_owned(), Variant::String(identity));
            }
            step
        })
        .collect()
}

async fn seed_twitch_account(
    provider: &dyn DataProvider,
    account: &TwitchAccount,
) -> Result<SeededTwitch, EmulatorError> {
    let credential = StoredCredential {
        access_token: OAuthToken::new(account.access_token.clone()),
        // Why: with no refresh token, no code path can reach the real Twitch token endpoint.
        refresh_token: None,
        user_id: account.user_id.clone(),
        login: account.login.clone(),
        expires_at: Some(SystemTime::now() + TWITCH_TOKEN_LIFETIME),
    };
    store_credential(provider, &credential)
        .await
        .map_err(storage_error)?;
    Ok(SeededTwitch {
        client_id: account.client_id.clone(),
        user_id: account.user_id.clone(),
        login: account.login.clone(),
    })
}

async fn seed_chat_command(
    provider: &dyn DataProvider,
    command: &ChatCommand,
    overlays: &[SeededOverlay],
) -> Result<SeededCommand, EmulatorError> {
    let mut overrides = TriggerConfig::new();
    overrides.insert("phrase".to_owned(), Variant::String(command.phrase.clone()));
    overrides.insert("case_sensitive".to_owned(), Variant::Bool(false));
    let triggered = seed_triggered_action(
        provider,
        TriggeredAction {
            action_name: &command.action_name,
            steps: &command.steps,
            kind_id: CHAT_COMMAND_TRIGGER_KIND,
            instance_name: &command.phrase,
            overrides,
        },
        overlays,
    )
    .await?;

    Ok(SeededCommand {
        phrase: command.phrase.clone(),
        action_name: command.action_name.clone(),
        action_id: triggered.action_id,
        trigger_instance_id: triggered.trigger_instance_id,
    })
}

async fn seed_event_trigger(
    provider: &dyn DataProvider,
    trigger: &EventTrigger,
    overlays: &[SeededOverlay],
) -> Result<SeededEventTrigger, EmulatorError> {
    let triggered = seed_triggered_action(
        provider,
        TriggeredAction {
            action_name: &trigger.action_name,
            steps: &trigger.steps,
            kind_id: &trigger.trigger_kind,
            instance_name: &trigger.trigger_kind,
            overrides: trigger.config.clone(),
        },
        overlays,
    )
    .await?;

    Ok(SeededEventTrigger {
        trigger_kind: trigger.trigger_kind.clone(),
        action_name: trigger.action_name.clone(),
        action_id: triggered.action_id,
        trigger_instance_id: triggered.trigger_instance_id,
    })
}

struct TriggeredAction<'a> {
    action_name: &'a str,
    steps: &'a [SubActionStep],
    kind_id: &'a str,
    instance_name: &'a str,
    overrides: TriggerConfig,
}

struct Triggered {
    action_id: ActionId,
    trigger_instance_id: TriggerInstanceId,
}

async fn seed_triggered_action(
    provider: &dyn DataProvider,
    spec: TriggeredAction<'_>,
    overlays: &[SeededOverlay],
) -> Result<Triggered, EmulatorError> {
    let queue = provider
        .queue_repo()
        .get_by_name(DEFAULT_QUEUE)
        .await
        .map_err(storage_error)?
        .ok_or_else(|| EmulatorError::Storage {
            reason: format!("the `{DEFAULT_QUEUE}` queue is missing"),
        })?;
    let action = Action {
        id: ActionId::new(),
        name: spec.action_name.to_owned(),
        group: None,
        queue_id: queue.id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::default(),
        description: None,
        sub_actions: addressed_steps(spec.steps, overlays),
    };
    provider
        .action_repo()
        .save(&action)
        .await
        .map_err(storage_error)?;

    let instance = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: spec.kind_id.to_owned(),
        name: spec.instance_name.to_owned(),
        overrides: spec.overrides,
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    };
    let triggers = provider.trigger_instance_repo();
    triggers.save(&instance).await.map_err(storage_error)?;
    triggers
        .link_action(action.id, instance.id, 0)
        .await
        .map_err(storage_error)?;

    Ok(Triggered {
        action_id: action.id,
        trigger_instance_id: instance.id,
    })
}

fn generate_bearer_token() -> String {
    let mut bytes = [0u8; BEARER_TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn storage_error(e: StorageError) -> EmulatorError {
    EmulatorError::Storage {
        reason: e.to_string(),
    }
}
