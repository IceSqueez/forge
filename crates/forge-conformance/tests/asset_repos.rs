#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::{Store, at, store};
use forge_speech_types::{AliasId, AliasState, AssignmentStrategy, EngineId, VoiceAlias, VoiceId};
use forge_storage::{
    DataProvider, FilterRule, FilterRuleKind, MediaBlobId, MediaReferrer, MediaReferrerKind,
    MediaRepo, SoundboardClipsRepo, StorageError, StoredClip, TtsFiltersRepo, TtsPipelineSettings,
    VoiceAliasRepo,
};
use forge_types::{ClipId, OutputDevice};

const TINY_GIF: &[u8] = b"GIF89a";
const UNKNOWN_BLOB: &str =
    "sha256-0000000000000000000000000000000000000000000000000000000000000000";

fn media(store: &Store) -> Arc<dyn MediaRepo> {
    store.backend.media_repo()
}

fn clips(store: &Store) -> Arc<dyn SoundboardClipsRepo> {
    store.backend.soundboard_clips_repo()
}

fn aliases(store: &Store) -> Arc<dyn VoiceAliasRepo> {
    store.backend.voice_alias_repo()
}

fn filters(store: &Store) -> Arc<dyn TtsFiltersRepo> {
    store.backend.tts_filters_repo()
}

fn unknown_blob() -> MediaBlobId {
    MediaBlobId::from_stored(UNKNOWN_BLOB.to_owned()).unwrap()
}

fn referrer() -> MediaReferrer {
    MediaReferrer::new(MediaReferrerKind::Overlay, "alert", "icon")
}

fn clip(name: &str) -> StoredClip {
    StoredClip {
        id: ClipId::new(),
        name: name.to_owned(),
        file_path: PathBuf::from(format!("{name}.wav")),
        volume: 1.0,
        output_device: OutputDevice::Default,
        hotkey: None,
        created_at: at(0),
        category: String::new(),
        loop_playback: false,
        duration_secs: None,
        builtin_id: None,
    }
}

fn alias(key: &str, viewer_name: &str) -> VoiceAlias {
    VoiceAlias {
        id: AliasId::generate(),
        alias_key: key.to_owned(),
        viewer_name: viewer_name.to_owned(),
        engine_id: EngineId("piper".to_owned()),
        voice_id: VoiceId("amy".to_owned()),
        pitch_semitones: None,
        rate_multiplier: None,
        state: AliasState::Active,
    }
}

fn rule(id: &str, position: u32) -> FilterRule {
    FilterRule {
        id: id.to_owned(),
        name: id.to_owned(),
        enabled: true,
        position,
        kind: FilterRuleKind::Literal {
            pattern: "pattern".to_owned(),
            replacement: "replacement".to_owned(),
            match_inside_words: false,
        },
    }
}

async fn rule_ids(store: &Store) -> Vec<String> {
    filters(store)
        .list_rules()
        .await
        .unwrap()
        .into_iter()
        .map(|rule| rule.id)
        .collect()
}

#[tokio::test]
async fn reads_and_writes_on_an_unknown_blob_report_it_missing() {
    let store = store().await;
    let repo = media(&store);
    let unknown = unknown_blob();

    assert!(repo.get(&unknown).await.unwrap().is_none());
    assert!(!repo.delete(&unknown).await.unwrap());
    assert!(repo.referrers(&unknown).await.unwrap().is_empty());
    assert!(matches!(
        repo.read(&unknown).await,
        Err(StorageError::NotFound { .. })
    ));
    assert!(matches!(
        repo.resolve(&unknown).await,
        Err(StorageError::NotFound { .. })
    ));
}

#[tokio::test]
async fn a_slot_nobody_holds_has_nothing_to_release() {
    let store = store().await;
    let repo = media(&store);

    assert_eq!(repo.blob_of(&referrer()).await.unwrap(), None);
    assert!(!repo.release(&referrer()).await.unwrap());
    assert_eq!(
        repo.release_all(MediaReferrerKind::Overlay, "alert")
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn a_held_blob_cannot_be_deleted_until_its_slot_is_released() {
    let store = store().await;
    let repo = media(&store);
    let blob = repo.store("icon.gif", TINY_GIF.to_vec()).await.unwrap();
    repo.retain(&referrer(), &blob.id).await.unwrap();

    assert!(repo.delete(&blob.id).await.is_err());

    assert!(repo.release(&referrer()).await.unwrap());
    assert!(repo.delete(&blob.id).await.unwrap());
}

#[tokio::test]
async fn an_unknown_clip_reads_as_nothing_and_deletes_nothing() {
    let store = store().await;
    let repo = clips(&store);
    let missing = ClipId::new();

    assert!(repo.get(missing).await.unwrap().is_none());
    assert!(!repo.delete(missing).await.unwrap());
}

#[tokio::test]
async fn clips_list_by_name_ignoring_case_and_a_second_save_replaces_the_row() {
    let store = store().await;
    let repo = clips(&store);
    let mut zebra = clip("zebra");
    repo.save(&zebra).await.unwrap();
    repo.save(&clip("Apple")).await.unwrap();
    repo.save(&clip("mango")).await.unwrap();
    zebra.volume = 0.5;
    repo.save(&zebra).await.unwrap();

    let listed = repo.list().await.unwrap();

    assert_eq!(
        listed
            .iter()
            .map(|clip| clip.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Apple", "mango", "zebra"]
    );
    assert_eq!(
        repo.get(zebra.id).await.unwrap().map(|c| c.volume),
        Some(0.5)
    );
}

#[tokio::test]
async fn deleting_an_absent_alias_is_not_an_error_and_an_unknown_key_reads_as_nothing() {
    let store = store().await;
    let repo = aliases(&store);

    repo.delete(&AliasId::generate()).await.unwrap();

    assert!(repo.find_by_alias_key("absent").await.unwrap().is_none());
}

#[tokio::test]
async fn aliases_are_listed_by_viewer_name_ignoring_case() {
    let store = store().await;
    let repo = aliases(&store);
    repo.upsert(&alias("1", "zed")).await.unwrap();
    repo.upsert(&alias("2", "Amy")).await.unwrap();

    let names: Vec<String> = repo
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|alias| alias.viewer_name)
        .collect();

    assert_eq!(names, vec!["Amy", "zed"]);
}

#[tokio::test]
async fn a_stored_assignment_strategy_reads_back() {
    let store = store().await;
    let repo = aliases(&store);
    let default = repo.get_strategy().await.unwrap();
    assert_eq!(default, AssignmentStrategy::default());

    repo.set_strategy(&default).await.unwrap();

    assert_eq!(repo.get_strategy().await.unwrap(), default);
}

#[tokio::test]
async fn replacing_filter_rules_keeps_only_the_new_set_in_position_order() {
    let store = store().await;
    let repo = filters(&store);
    repo.replace_rules(&[rule("gone", 0)]).await.unwrap();

    repo.replace_rules(&[rule("second", 1), rule("first", 0)])
        .await
        .unwrap();

    assert_eq!(rule_ids(&store).await, vec!["first", "second"]);
}

#[tokio::test]
async fn replacing_filter_rules_with_nothing_empties_the_list() {
    let store = store().await;
    let repo = filters(&store);
    repo.replace_rules(&[rule("gone", 0)]).await.unwrap();

    repo.replace_rules(&[]).await.unwrap();

    assert!(rule_ids(&store).await.is_empty());
}

#[tokio::test]
async fn pipeline_settings_read_their_defaults_until_written_then_read_back() {
    let store = store().await;
    let repo = filters(&store);
    let defaults = TtsPipelineSettings::default();
    assert_eq!(repo.get_pipeline_settings().await.unwrap(), defaults);

    let changed = TtsPipelineSettings {
        strip_emoji: !defaults.strip_emoji,
        ..defaults
    };
    repo.set_pipeline_settings(&changed).await.unwrap();

    assert_eq!(repo.get_pipeline_settings().await.unwrap(), changed);
}
