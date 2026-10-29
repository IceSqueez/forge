use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_speech_types::{AliasId, AssignmentStrategy, IgnoreProfile, VoiceAlias};
use forge_speech_types::{AliasState, EngineId, VoiceId};
use forge_storage::settings::reserved_keys as key;
use forge_storage::{
    BanLedgerEntry, BanLedgerKey, BanOrigin, CredentialId, FilterRule, FilterRuleKind, GlobalsRepo,
    LatestRecord, MediaBlobId, OverlayId, ScriptRepo, SettingsRepo, StoredClip, StoredDonation,
    TtsPipelineSettings, Viewer, clip_source_referrer,
};
use forge_storage::{SettingPortability, SettingSpec, SettingsCatalog};
use forge_storage_sqlite::SqliteDatabaseFileSchema;
use forge_transfer::{
    SetupExportRequest, SetupImportOutcome, SetupImportRequest, SetupLivePart, SetupQueueChanges,
    StageRequest, StagedSetup, TransferError, apply_setup_import, stage_full_restore, stage_setup,
    write_setup,
};
use forge_types::{
    Action, ActionId, CurrencyCode, Donation, DonationOrigin, Donor, EventId, ExecutionMode,
    IntegrationId, ModerationMarks, MoneyAmount, PlatformId, Queue, TriggerInstance,
    UnifiedChatRow, Variant,
};
use time::OffsetDateTime;
use ulid::Ulid;

use crate::settings_catalog::registered_settings_catalog;

use super::pack_transfer::{
    Fixture, GLOBAL_NAME, Registries, SECRET_TOKEN, Store, files_under, seed,
};

const COUNTER: &str = "counter";
const SCRATCH: &str = "scratch";
const CLIP_HOTKEY: &str = "Ctrl+F1";
const SOURCE_THEME: &str = "source-theme";
const TARGET_THEME: &str = "target-theme";
const ENGINE_PARAMS_KEY: &str = "tts.engine_params.piper";
const UNCLASSIFIED_KEY: &str = "ui.last_note";
const BAN_CHANNEL: &str = "channel";
const LATEST_SLOT: &str = "latest_donation";
const HISTORY_LIMIT: usize = 100;

type Settings = BTreeMap<String, String>;

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_790_000_000 + seconds).unwrap()
}

async fn set_settings(store: &Store, pairs: &[(&str, &str)]) {
    for (k, v) in pairs {
        SettingsRepo::set_string(store.provider.as_ref(), k, v)
            .await
            .unwrap();
    }
}

async fn settings_of(store: &Store) -> Settings {
    SettingsRepo::load_all(store.provider.as_ref())
        .await
        .unwrap()
        .into_iter()
        .collect()
}

fn alias(viewer: &str, voice: &str) -> VoiceAlias {
    VoiceAlias {
        id: AliasId::new(),
        viewer_id: viewer.to_owned(),
        viewer_name: format!("{viewer} name"),
        engine_id: EngineId("piper".to_owned()),
        voice_id: VoiceId(voice.to_owned()),
        pitch_semitones: Some(1.5),
        rate_multiplier: None,
        state: AliasState::Active,
    }
}

fn filter_rule(id: &str, pattern: &str) -> FilterRule {
    FilterRule {
        id: id.to_owned(),
        name: format!("{id} rule"),
        enabled: true,
        position: 0,
        kind: FilterRuleKind::Literal {
            pattern: pattern.to_owned(),
            replacement: "*".to_owned(),
            match_inside_words: false,
        },
    }
}

async fn seed_source_extras(store: &Store, f: &Fixture) {
    let p = store.provider.as_ref();
    GlobalsRepo::set(p, COUNTER, Variant::Int(42), true)
        .await
        .unwrap();
    GlobalsRepo::set(p, SCRATCH, Variant::String("tmp".to_owned()), false)
        .await
        .unwrap();
    let clips = p.soundboard_clips_repo();
    let clip = clips.get(f.clip).await.unwrap().unwrap();
    clips
        .save(&StoredClip {
            hotkey: Some(CLIP_HOTKEY.to_owned()),
            ..clip
        })
        .await
        .unwrap();
    let tts = p.tts_filters_repo();
    tts.set_pipeline_settings(&TtsPipelineSettings {
        max_length: Some(120),
        skip_starts_with_bang: true,
        ..TtsPipelineSettings::default()
    })
    .await
    .unwrap();
    tts.replace_rules(&[filter_rule("source", "darn")])
        .await
        .unwrap();
    let voices = p.voice_alias_repo();
    voices
        .upsert(&alias("viewer-a", "en_US-amy"))
        .await
        .unwrap();
    voices
        .set_strategy(&AssignmentStrategy::Random)
        .await
        .unwrap();
    voices
        .set_ignore_profile(&IgnoreProfile {
            excluded_voice_ids: Vec::new(),
            excluded_locales: vec!["de-DE".to_owned()],
        })
        .await
        .unwrap();
    set_settings(
        store,
        &[
            (key::THEME, SOURCE_THEME),
            (ENGINE_PARAMS_KEY, "{\"speed\":1.2}"),
            (key::HOTKEY_HOLD_CEILING_SECS, "9"),
            (key::WINDOW_PLACEMENT, "source-window"),
            (key::AUDIO_OUTPUT_DEVICE_ID, "source-speakers"),
        ],
    )
    .await;
}

struct TargetExtras {
    archived_action: ActionId,
    overlay: OverlayId,
}

async fn seed_target_extras(store: &Store, f: &Fixture) -> TargetExtras {
    let p = store.provider.as_ref();
    let archived = Action {
        id: ActionId::new(),
        name: "Target archived".to_owned(),
        group: None,
        queue_id: f.queue,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::default(),
        description: None,
        sub_actions: Vec::new(),
    };
    let actions = p.action_repo();
    actions.save(&archived).await.unwrap();
    assert!(actions.archive(archived.id).await.unwrap());
    p.tts_filters_repo()
        .replace_rules(&[filter_rule("target", "heck")])
        .await
        .unwrap();
    p.voice_alias_repo()
        .upsert(&alias("viewer-b", "en_GB-alan"))
        .await
        .unwrap();
    set_settings(
        store,
        &[
            (key::THEME, TARGET_THEME),
            (key::DENSITY, "compact"),
            (key::WINDOW_PLACEMENT, "target-window"),
            (key::AUDIO_OUTPUT_DEVICE_ID, "target-speakers"),
            (key::STAY_AWAKE, "true"),
            (UNCLASSIFIED_KEY, "target-note"),
        ],
    )
    .await;
    let overlay = p
        .overlay_repo()
        .create("Target only", "overlay.alert", 1)
        .await
        .unwrap();
    let overlay_dir = store.overlay_root().join(overlay.id.as_str());
    std::fs::create_dir_all(&overlay_dir).unwrap();
    std::fs::write(overlay_dir.join("target.css"), b"body {}").unwrap();
    TargetExtras {
        archived_action: archived.id,
        overlay: overlay.id,
    }
}

fn chat_row(marker: &str) -> UnifiedChatRow {
    UnifiedChatRow {
        id: format!("{marker}-chat"),
        event_id: EventId::new(),
        source: PlatformId::Twitch,
        received_at: at(1),
        author: format!("{marker}-author"),
        author_id: Some(format!("{marker}-author-id")),
        author_color: None,
        body_segments: Vec::new(),
        badges: Vec::new(),
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    }
}

fn donation(marker: &str) -> Donation {
    Donation {
        provider: IntegrationId::new("donatello"),
        donation_id: format!("{marker}-donation"),
        donor: Donor::Named(format!("{marker}-donor")),
        message: Some(format!("{marker}-message")),
        amount: MoneyAmount::from_micros(5_000_000, CurrencyCode::parse("UAH").unwrap()),
        occurred_at: at(2),
        origin: DonationOrigin::Live,
    }
}

fn ban(marker: &str) -> BanLedgerEntry {
    BanLedgerEntry {
        key: BanLedgerKey {
            platform: PlatformId::Kick,
            channel_id: BAN_CHANNEL.to_owned(),
            viewer_id: format!("{marker}-banned"),
        },
        viewer_name: format!("{marker}-banned-name"),
        reason: Some("spam".to_owned()),
        moderator: None,
        banned_at: at(3),
        expires_at: None,
        platform_ban_id: None,
        origin: BanOrigin::IssuedByForge,
    }
}

async fn seed_history(store: &Store, marker: &str) {
    let p = store.provider.as_ref();
    p.store(
        &CredentialId::new("kick:broadcaster"),
        &format!("{marker}-kick-token"),
    )
    .await
    .unwrap();
    p.event_log_repo()
        .insert(&Event {
            id: EventId::new(),
            source: EventSource::Twitch,
            kind: "twitch.chat.message".to_owned(),
            timestamp: at(0),
            payload: serde_json::json!({ "marker": format!("{marker}-event") }),
            caused_by: None,
            replay: false,
            causation_depth: 0,
        })
        .await
        .unwrap();
    p.chat_history_repo()
        .append(&chat_row(marker))
        .await
        .unwrap();
    p.viewer_repo()
        .record_message(
            PlatformId::Twitch,
            &format!("{marker}-viewer"),
            &format!("{marker}-viewer-name"),
        )
        .await
        .unwrap();
    p.donation_repo()
        .insert_if_new(&donation(marker), at(2))
        .await
        .unwrap();
    p.ban_ledger_repo()
        .upsert(&ban(marker), at(3))
        .await
        .unwrap();
    p.latest_value_repo()
        .upsert_if_newer(&LatestRecord {
            slot: LATEST_SLOT.to_owned(),
            platform: "twitch".to_owned(),
            payload: serde_json::json!({ "who": format!("{marker}-latest") }),
            occurred_at: at(4),
            updated_at: at(4),
        })
        .await
        .unwrap();
}

#[derive(PartialEq)]
struct History {
    credentials: Vec<(String, Option<String>)>,
    events: serde_json::Value,
    chat: serde_json::Value,
    viewers: Vec<Viewer>,
    donations: Vec<StoredDonation>,
    bans: Vec<BanLedgerEntry>,
    latest: Vec<LatestRecord>,
}

async fn history_of(store: &Store) -> History {
    let p = store.provider.as_ref();
    let mut credentials = Vec::new();
    let mut ids = p.list_ids().await.unwrap();
    ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    for id in ids {
        credentials.push((id.as_str().to_owned(), p.load(&id).await.unwrap()));
    }
    History {
        credentials,
        events: serde_json::to_value(p.event_log_repo().recent(HISTORY_LIMIT).await.unwrap())
            .unwrap(),
        chat: serde_json::to_value(
            p.chat_history_repo()
                .list_recent(HISTORY_LIMIT)
                .await
                .unwrap(),
        )
        .unwrap(),
        viewers: p.viewer_repo().list().await.unwrap(),
        donations: p.donation_repo().list_recent(HISTORY_LIMIT).await.unwrap(),
        bans: p
            .ban_ledger_repo()
            .list_active(PlatformId::Kick, BAN_CHANNEL, at(10), HISTORY_LIMIT)
            .await
            .unwrap(),
        latest: p.latest_value_repo().list_slot(LATEST_SLOT).await.unwrap(),
    }
}

type ClipView = (
    String,
    String,
    u32,
    Option<String>,
    String,
    bool,
    Option<MediaBlobId>,
);
type OverlayView = (String, String, bool, serde_json::Value, Vec<String>);

#[derive(Debug, PartialEq)]
struct ConfigView {
    actions: Vec<Action>,
    links: Vec<(ActionId, Vec<forge_types::TriggerInstanceId>)>,
    triggers: Vec<TriggerInstance>,
    queues: Vec<Queue>,
    scripts: Vec<(String, String, String, bool)>,
    persisted_globals: Vec<(String, Variant)>,
    clips: Vec<ClipView>,
    overlays: Vec<OverlayView>,
    overlay_files: Vec<(String, PathBuf, Vec<u8>)>,
    media: Vec<String>,
    tts_pipeline: TtsPipelineSettings,
    filter_rules: Vec<FilterRule>,
    voice_aliases: serde_json::Value,
    voice_strategy: AssignmentStrategy,
    ignore_profile: serde_json::Value,
    portable_settings: Settings,
}

async fn config_of(store: &Store) -> ConfigView {
    let p = store.provider.as_ref();
    let overlay_rows = p.overlay_repo().list().await.unwrap();
    let overlay_name: BTreeMap<String, String> = overlay_rows
        .iter()
        .map(|o| (o.id.as_str().to_owned(), o.display_name.clone()))
        .collect();
    let mut actions = p.action_repo().list().await.unwrap();
    actions.sort_by_key(|a| a.id);
    for action in &mut actions {
        for step in &mut action.sub_actions {
            if step.kind_id == forge_runtime::OVERLAY_SEND_KIND_ID
                && let Some(Variant::String(target)) =
                    step.config.get_mut(forge_runtime::OVERLAY_TARGET_KEY)
                && let Some(name) = overlay_name.get(target.as_str())
            {
                *target = name.clone();
            }
        }
    }
    let mut links = Vec::new();
    for action in &actions {
        links.push((action.id, store.linked(action.id).await));
    }
    let mut triggers = p.trigger_instance_repo().list_all().await.unwrap();
    triggers.sort_by_key(|t| t.id);
    let mut queues = p.queue_repo().list().await.unwrap();
    queues.sort_by_key(|q| q.id);
    let mut scripts: Vec<_> = forge_storage::ScriptRepo::list(p)
        .await
        .unwrap()
        .into_iter()
        .map(|s| (s.id.to_string(), s.name, s.body, s.enabled))
        .collect();
    scripts.sort();
    let mut persisted_globals: Vec<_> = GlobalsRepo::list(p)
        .await
        .unwrap()
        .into_iter()
        .filter(|g| g.persisted)
        .map(|g| (g.name, g.value))
        .collect();
    persisted_globals.sort_by(|a, b| a.0.cmp(&b.0));
    let media_repo = p.media_repo();
    let mut clips = Vec::new();
    for clip in p.soundboard_clips_repo().list().await.unwrap() {
        clips.push((
            clip.id.to_string(),
            clip.name,
            clip.volume.to_bits(),
            clip.hotkey,
            clip.category,
            clip.loop_playback,
            media_repo
                .blob_of(&clip_source_referrer(clip.id))
                .await
                .unwrap(),
        ));
    }
    clips.sort();
    let mut overlays: Vec<_> = overlay_rows
        .into_iter()
        .map(|o| {
            (
                o.display_name,
                o.kind_id,
                o.enabled,
                serde_json::to_value(o.config).unwrap(),
                o.source_overrides,
            )
        })
        .collect();
    overlays.sort_by(|a, b| a.0.cmp(&b.0));
    let mut overlay_files: Vec<_> = files_under(&store.overlay_root())
        .into_iter()
        .map(|(path, bytes)| {
            let mut parts = path.components();
            let id = parts
                .next()
                .unwrap()
                .as_os_str()
                .to_string_lossy()
                .into_owned();
            let name = overlay_name.get(&id).cloned().unwrap_or(id);
            (name, parts.as_path().to_path_buf(), bytes)
        })
        .collect();
    overlay_files.sort();
    let mut media: Vec<_> = media_repo
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|blob| blob.id.as_str().to_owned())
        .collect();
    media.sort();
    let tts = p.tts_filters_repo();
    let voices = p.voice_alias_repo();
    let mut aliases = voices.list().await.unwrap();
    aliases.sort_by(|a, b| a.id.0.cmp(&b.id.0));
    ConfigView {
        actions,
        links,
        triggers,
        queues,
        scripts,
        persisted_globals,
        clips,
        overlays,
        overlay_files,
        media,
        tts_pipeline: tts.get_pipeline_settings().await.unwrap(),
        filter_rules: tts.list_rules().await.unwrap(),
        voice_aliases: serde_json::to_value(aliases).unwrap(),
        voice_strategy: voices.get_strategy().await.unwrap(),
        ignore_profile: serde_json::to_value(voices.get_ignore_profile().await.unwrap()).unwrap(),
        portable_settings: settings_of(store)
            .await
            .into_iter()
            .filter(|(k, _)| registered_settings_catalog().is_portable(k))
            .collect(),
    }
}

async fn export_setup(store: &Store) -> PathBuf {
    export_setup_with(store, registered_settings_catalog()).await
}

async fn export_setup_with(store: &Store, settings_catalog: SettingsCatalog) -> PathBuf {
    write_setup(SetupExportRequest {
        provider: Arc::clone(&store.provider),
        settings_catalog: Arc::new(settings_catalog),
        overlay_root: store.overlay_root(),
        destination: store
            .data_dir()
            .join(format!("{}.forgebundle", Ulid::generate())),
    })
    .await
    .unwrap()
}

async fn stage(store: &Store, bundle: &Path) -> StagedSetup {
    stage_setup(bundle.to_path_buf(), store.data_dir().to_path_buf())
        .await
        .unwrap()
}

async fn import_into(
    store: &Store,
    registries: &Registries,
    bundle: &Path,
    overlay_root: PathBuf,
) -> Result<SetupImportOutcome, TransferError> {
    let staged = stage(store, bundle).await;
    apply_setup_import(SetupImportRequest {
        provider: Arc::clone(&store.provider),
        settings_catalog: &registered_settings_catalog(),
        registries: registries.closure(),
        staged: &staged,
        data_dir: store.data_dir().to_path_buf(),
        overlay_root,
    })
    .await
}

async fn import(
    store: &Store,
    registries: &Registries,
    bundle: &Path,
) -> Result<SetupImportOutcome, TransferError> {
    import_into(store, registries, bundle, store.overlay_root()).await
}

struct Migration {
    registries: Registries,
    source: Store,
    source_fixture: Fixture,
    target: Store,
    target_fixture: Fixture,
    target_extras: TargetExtras,
    bundle: PathBuf,
}

async fn source_and_target() -> Migration {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let source_fixture = seed(&source).await;
    seed_source_extras(&source, &source_fixture).await;
    seed_history(&source, "source").await;
    let bundle = export_setup(&source).await;
    let target = Store::open().await;
    let target_fixture = seed(&target).await;
    let target_extras = seed_target_extras(&target, &target_fixture).await;
    seed_history(&target, "target").await;
    Migration {
        registries,
        source,
        source_fixture,
        target,
        target_fixture,
        target_extras,
        bundle,
    }
}

impl Migration {
    async fn import(&self) -> Result<SetupImportOutcome, TransferError> {
        import(&self.target, &self.registries, &self.bundle).await
    }
}

#[tokio::test]
async fn importing_a_setup_makes_the_target_configuration_equal_the_source() {
    let m = source_and_target().await;

    m.import().await.unwrap();

    assert_eq!(config_of(&m.target).await, config_of(&m.source).await);
}

#[tokio::test]
async fn importing_a_setup_deletes_the_targets_own_live_and_archived_items() {
    let m = source_and_target().await;

    m.import().await.unwrap();

    let actions = m.target.provider.action_repo();
    let f = &m.target_fixture;
    let mut survivors = Vec::new();
    for id in [
        f.root,
        f.helper,
        f.unrelated,
        m.target_extras.archived_action,
    ] {
        if actions.get(id).await.unwrap().is_some() || actions.restore(id).await.unwrap() {
            survivors.push(id);
        }
    }
    let queue_survived = m
        .target
        .provider
        .queue_repo()
        .list()
        .await
        .unwrap()
        .iter()
        .any(|q| q.id == f.queue);
    let only = &m.target_extras.overlay;
    let overlay_survived = m
        .target
        .provider
        .overlay_repo()
        .get(only)
        .await
        .unwrap()
        .is_some()
        || m.target.overlay_root().join(only.as_str()).exists();
    assert_eq!(
        (survivors, queue_survived, overlay_survived),
        (Vec::new(), false, false)
    );
}

#[tokio::test]
async fn a_setup_carries_current_values_of_persisted_globals_only() {
    let m = source_and_target().await;
    let staged = stage(&m.target, &m.bundle).await;

    m.import().await.unwrap();

    let documents = staged.documents();
    let carried: BTreeSet<&str> = documents
        .configuration
        .globals
        .iter()
        .map(|g| g.name.as_str())
        .chain(documents.global_values.keys().map(String::as_str))
        .collect();
    let p = m.target.provider.as_ref();
    assert_eq!(
        (
            carried,
            GlobalsRepo::get(p, COUNTER).await.unwrap(),
            GlobalsRepo::get(p, SCRATCH).await.unwrap(),
        ),
        (
            BTreeSet::from([COUNTER, GLOBAL_NAME]),
            Some(Variant::Int(42)),
            None
        )
    );
}

#[tokio::test]
async fn importing_a_setup_leaves_credentials_and_every_history_kind_untouched() {
    let m = source_and_target().await;
    let before = history_of(&m.target).await;

    m.import().await.unwrap();

    assert!(history_of(&m.target).await == before);
}

#[tokio::test]
async fn importing_a_setup_leaves_the_targets_machine_local_settings_untouched() {
    let m = source_and_target().await;
    let machine_local = |settings: Settings| -> Settings {
        settings
            .into_iter()
            .filter(|(k, _)| {
                !registered_settings_catalog().is_portable(k) && k != key::VOICE_ASSIGNMENT_STRATEGY
            })
            .collect()
    };
    let before = machine_local(settings_of(&m.target).await);

    m.import().await.unwrap();

    assert_eq!(machine_local(settings_of(&m.target).await), before);
}

#[tokio::test]
async fn a_setup_file_carries_no_credentials_history_or_machine_local_settings() {
    let m = source_and_target().await;
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&m.bundle).unwrap())
        .read_to_end(&mut raw)
        .unwrap();
    let raw = String::from_utf8_lossy(&raw);

    for leaked in [
        SECRET_TOKEN,
        "source-kick-token",
        "kick:broadcaster",
        "source-event",
        "source-author",
        "source-viewer",
        "source-donor",
        "source-banned",
        "source-latest",
        "source-window",
        "source-speakers",
        super::pack_transfer::SECRET_SETTING,
    ] {
        assert!(!raw.contains(leaked), "setup bytes carry {leaked:?}");
    }
}

#[tokio::test]
async fn a_setup_carries_owner_declared_portable_settings_and_withholds_owner_machine_local_ones() {
    let (_registries, store, _f) = source_only().await;
    let bundle = export_setup(&store).await;

    let staged = stage(&store, &bundle).await;

    let carried = &staged.documents().settings;
    assert_eq!(
        (
            carried.get(key::HOTKEY_HOLD_CEILING_SECS).cloned(),
            carried.contains_key(key::WINDOW_PLACEMENT),
        ),
        (Some("9".to_owned()), false)
    );
}

fn every_owner_spec_made_portable() -> SettingsCatalog {
    let mut catalog = SettingsCatalog::core();
    for specs in [
        forge_server::settings::SETTING_SPECS,
        forge_script::settings::SETTING_SPECS,
        forge_runtime::settings::SETTING_SPECS,
    ] {
        let portable: Vec<SettingSpec> = specs
            .iter()
            .map(|spec| SettingSpec {
                portability: SettingPortability::Portable,
                ..*spec
            })
            .collect();
        catalog.register(&portable).unwrap();
    }
    catalog
}

#[tokio::test]
async fn a_setup_exported_with_every_owner_key_portable_cannot_loosen_network_exposure_or_http_trust()
 {
    let (registries, store, _f) = source_only().await;
    let guarded = [
        (key::SERVER_BIND_ADDRESS, "127.0.0.1", "0.0.0.0"),
        (key::SERVER_LAN_BIND_ENABLED, "false", "true"),
        (key::SERVER_AUTH_REQUIRED_FOR_READS, "true", "false"),
        (key::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN, "true", "false"),
        (key::SERVER_OVERLAY_CORS_ANY_ORIGIN, "false", "true"),
        (
            key::SERVER_ADDITIONAL_ORIGINS,
            "[]",
            "[\"https://attacker.example\"]",
        ),
        (key::SERVER_OVERLAY_ROOT, "/home/local/overlays", "/etc"),
        (
            key::SCRIPT_HTTP_ALLOWED_DOMAINS,
            "[\"api.example.com\"]",
            "[\"*\"]",
        ),
        (key::SCRIPT_HTTP_ALLOW_LOCAL, "false", "true"),
        (key::CORE_HTTP_ALLOW_LOCAL, "false", "true"),
    ];
    let loosened: Vec<(&str, &str)> = guarded.iter().map(|(k, _, loose)| (*k, *loose)).collect();
    let local: Vec<(&str, &str)> = guarded.iter().map(|(k, safe, _)| (*k, *safe)).collect();
    set_settings(&store, &loosened).await;
    let bundle = export_setup_with(&store, every_owner_spec_made_portable()).await;
    set_settings(&store, &local).await;
    let staged = stage(&store, &bundle).await;
    let all_carried = guarded
        .iter()
        .all(|(k, _, _)| staged.documents().settings.contains_key(*k));

    import(&store, &registries, &bundle).await.unwrap();

    let settings = settings_of(&store).await;
    let changed: Vec<&str> = local
        .iter()
        .filter(|(k, safe)| settings.get(*k).map(String::as_str) != Some(*safe))
        .map(|(k, _)| *k)
        .collect();
    assert_eq!((all_carried, changed), (true, Vec::<&str>::new()));
}

#[tokio::test]
async fn a_failure_midway_through_the_import_leaves_the_target_as_it_was() {
    let m = source_and_target().await;
    let blocked_root = m.target.data_dir().join("overlays-blocked");
    std::fs::write(&blocked_root, b"not a directory").unwrap();
    let config_before = config_of(&m.target).await;
    let history_before = history_of(&m.target).await;
    let settings_before = settings_of(&m.target).await;

    let failed = import_into(&m.target, &m.registries, &m.bundle, blocked_root).await;

    assert!(matches!(failed, Err(TransferError::Io(_))), "{failed:?}");
    assert_eq!(config_of(&m.target).await, config_before);
    assert!(history_of(&m.target).await == history_before);
    assert_eq!(settings_of(&m.target).await, settings_before);
}

#[tokio::test]
async fn a_failed_import_keeps_the_targets_archived_action_archived() {
    let m = source_and_target().await;
    let blocked_root = m.target.data_dir().join("overlays-blocked");
    std::fs::write(&blocked_root, b"not a directory").unwrap();

    let failed = import_into(&m.target, &m.registries, &m.bundle, blocked_root).await;

    let actions = m.target.provider.action_repo();
    let archived = m.target_extras.archived_action;
    assert!(failed.is_err());
    assert_eq!(actions.get(archived).await.unwrap(), None);
    assert!(
        actions.restore(archived).await.unwrap(),
        "the archived action was lost or left live by the rollback"
    );
}

#[tokio::test]
async fn importing_a_setup_writes_a_pre_setup_safety_snapshot_that_restore_can_stage() {
    let m = source_and_target().await;

    let outcome = m.import().await.unwrap();
    let snapshot = outcome.safety_snapshot;
    let staged = stage_full_restore(StageRequest {
        bundle: snapshot.clone(),
        data_dir: m.target.data_dir().to_path_buf(),
        passphrase: None,
        database_schema: Arc::new(SqliteDatabaseFileSchema),
    })
    .await;

    assert!(
        snapshot
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("forge-pre-setup")
    );
    assert!(staged.is_ok(), "{staged:?}");
}

#[tokio::test]
async fn importing_into_another_install_reports_its_queue_swap_every_live_part_and_a_restart() {
    let m = source_and_target().await;
    let source_queue = m
        .source
        .provider
        .queue_repo()
        .list()
        .await
        .unwrap()
        .into_iter()
        .find(|q| q.id == m.source_fixture.queue)
        .unwrap();

    let outcome = m.import().await.unwrap();

    assert_eq!(
        (outcome.queues, outcome.refresh, outcome.restart_recommended),
        (
            SetupQueueChanges {
                added: vec![source_queue],
                changed: Vec::new(),
                removed: vec![m.target_fixture.queue],
            },
            BTreeSet::from([
                SetupLivePart::Queues,
                SetupLivePart::Scripts,
                SetupLivePart::Overlays,
                SetupLivePart::Hotkeys,
                SetupLivePart::Tts,
            ]),
            true,
        )
    );
}

async fn source_only() -> (Registries, Store, Fixture) {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;
    seed_source_extras(&store, &f).await;
    (registries, store, f)
}

#[tokio::test]
async fn reimporting_an_installs_own_setup_changes_no_queue_and_recommends_no_restart() {
    let (registries, store, _f) = source_only().await;
    let bundle = export_setup(&store).await;

    let outcome = import(&store, &registries, &bundle).await.unwrap();

    assert_eq!(
        (outcome.queues, outcome.restart_recommended),
        (SetupQueueChanges::default(), false)
    );
}

#[tokio::test]
async fn a_queue_edited_since_the_export_is_reported_changed_back_to_the_setup_version() {
    let (registries, store, f) = source_only().await;
    let bundle = export_setup(&store).await;
    let queues = store.provider.queue_repo();
    let exported = queues
        .list()
        .await
        .unwrap()
        .into_iter()
        .find(|q| q.id == f.queue)
        .unwrap();
    queues
        .save(&Queue {
            concurrency: exported.concurrency + 3,
            ..exported.clone()
        })
        .await
        .unwrap();

    let outcome = import(&store, &registries, &bundle).await.unwrap();

    assert_eq!(
        outcome.queues,
        SetupQueueChanges {
            added: Vec::new(),
            changed: vec![exported],
            removed: Vec::new(),
        }
    );
}

#[tokio::test]
async fn a_portable_setting_only_the_target_holds_is_removed_and_recommends_a_restart() {
    let (registries, store, _f) = source_only().await;
    let bundle = export_setup(&store).await;
    set_settings(&store, &[(key::DENSITY, "compact")]).await;

    let outcome = import(&store, &registries, &bundle).await.unwrap();

    assert_eq!(
        (
            outcome.restart_recommended,
            settings_of(&store).await.get(key::DENSITY).cloned()
        ),
        (true, None)
    );
}

async fn archive_greet(store: &Store) -> forge_types::ScriptId {
    let id = ScriptRepo::get_by_name(store.provider.as_ref(), super::pack_transfer::SCRIPT_NAME)
        .await
        .unwrap()
        .unwrap()
        .id;
    assert!(store.provider.archive_script(id).await.unwrap());
    id
}

async fn archived_scripts(store: &Store) -> Vec<forge_types::ScriptId> {
    store
        .provider
        .list_archived_scripts()
        .await
        .unwrap()
        .into_iter()
        .map(|script| script.id)
        .collect()
}

async fn live_greet(store: &Store) -> Option<forge_types::ScriptId> {
    ScriptRepo::get_by_name(store.provider.as_ref(), super::pack_transfer::SCRIPT_NAME)
        .await
        .unwrap()
        .map(|script| script.id)
}

#[tokio::test]
async fn a_setup_replaces_the_targets_archived_same_named_script_with_its_own() {
    let m = source_and_target().await;
    archive_greet(&m.target).await;

    m.import().await.unwrap();

    assert_eq!(
        (
            live_greet(&m.target).await,
            archived_scripts(&m.target).await
        ),
        (live_greet(&m.source).await, Vec::new())
    );
}

#[tokio::test]
async fn a_failed_import_keeps_the_targets_archived_script_archived() {
    let m = source_and_target().await;
    let archived = archive_greet(&m.target).await;
    let blocked_root = m.target.data_dir().join("overlays-blocked");
    std::fs::write(&blocked_root, b"not a directory").unwrap();

    let failed = import_into(&m.target, &m.registries, &m.bundle, blocked_root).await;

    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(
        (
            live_greet(&m.target).await,
            archived_scripts(&m.target).await
        ),
        (None, vec![archived])
    );
}
