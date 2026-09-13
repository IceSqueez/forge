use std::time::{Duration, SystemTime};

use forge_platform_core::paths;
use forge_platform_twitch::credentials::{StoredCredential, store_credential};
use forge_server::ServerSettings;
use forge_storage::{CredentialId, DataProvider, SettingsRepo, StorageError};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ExecutionMode, OAuthToken, PermissionRung, PlatformScope, TriggerConfig,
    TriggerInstance, TriggerInstanceId, Variant,
};
use rand::Rng as _;

use super::data_dir::{DATA_DIR_VARIABLE, ForgeDataDir, KEY_FILE_VARIABLE};
use super::port::free_loopback_port;
use super::report::{SeedReport, SeededCommand, SeededServer, SeededTwitch};
use super::spec::{ChatCommand, Fixture, TwitchAccount};
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
    let (twitch, chat_commands) = written?;

    Ok(SeedReport {
        data_dir: data_dir.path().to_owned(),
        server: SeededServer { port, bearer_token },
        twitch,
        chat_commands,
    })
}

async fn write_fixture(
    provider: &dyn DataProvider,
    fixture: &Fixture,
    port: u16,
    bearer_token: &str,
) -> Result<(Option<SeededTwitch>, Vec<SeededCommand>), EmulatorError> {
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
    let mut chat_commands = Vec::with_capacity(fixture.chat_commands.len());
    for command in &fixture.chat_commands {
        chat_commands.push(seed_chat_command(provider, command).await?);
    }
    Ok((twitch, chat_commands))
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
) -> Result<SeededCommand, EmulatorError> {
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
        name: command.action_name.clone(),
        group: None,
        queue_id: queue.id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::default(),
        description: None,
        sub_actions: command.steps.clone(),
    };
    provider
        .action_repo()
        .save(&action)
        .await
        .map_err(storage_error)?;

    let mut overrides = TriggerConfig::new();
    overrides.insert("phrase".to_owned(), Variant::String(command.phrase.clone()));
    overrides.insert("case_sensitive".to_owned(), Variant::Bool(false));
    let instance = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: CHAT_COMMAND_TRIGGER_KIND.to_owned(),
        name: command.phrase.clone(),
        overrides,
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

    Ok(SeededCommand {
        phrase: command.phrase.clone(),
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
