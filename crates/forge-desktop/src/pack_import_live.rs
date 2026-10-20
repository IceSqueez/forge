use std::collections::HashSet;
use std::hash::Hash;
use std::sync::Arc;

use forge_hotkey::HotkeyReconciler;
use forge_runtime::{MembershipOutcome, OverlayServiceError};
use forge_speak_queue::{
    PipelineConfigHandle, SpeakCommand, SpeakQueueHandle, build_config_lenient,
};
use forge_storage::{AliasId, DataProvider, OverlayId, StorageError};
use forge_transfer::{
    PackItemKind, PackWrittenItem, SetupImportOutcome, SetupLivePart, SetupQueueChanges,
};
use forge_types::{Queue, Shared};
use forge_voice::IgnoreProfile;

use crate::pack_services::PackServices;
use crate::runtime_handles::RuntimeHandles;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LiveTargets {
    pub queues: Vec<String>,
    pub scripts: bool,
    pub overlays: Vec<OverlayId>,
}

impl LiveTargets {
    pub(crate) fn of(written: &[PackWrittenItem]) -> Self {
        let mut targets = Self::default();
        for item in written {
            match item.key.kind {
                PackItemKind::Queue => targets.queues.push(item.local_id.clone()),
                PackItemKind::Script => targets.scripts = true,
                PackItemKind::Overlay => targets.overlays.push(OverlayId::new(&item.local_id)),
                PackItemKind::Media
                | PackItemKind::Global
                | PackItemKind::Clip
                | PackItemKind::Trigger
                | PackItemKind::Action => {}
            }
        }
        targets
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveRefresh {
    Applied,
    NeedsRestart,
}

pub(crate) async fn refresh_live(services: &PackServices, targets: LiveTargets) -> LiveRefresh {
    let queues = refresh_queues(services, &targets.queues).await;
    let scripts = !targets.scripts || refresh_scripts(services).await;
    let overlays = refresh_overlays(services, &targets.overlays).await;
    if queues && scripts && overlays {
        LiveRefresh::Applied
    } else {
        LiveRefresh::NeedsRestart
    }
}

async fn refresh_queues(services: &PackServices, ids: &[String]) -> bool {
    if ids.is_empty() {
        return true;
    }
    let queues = match services.backend.queue_repo().list().await {
        Ok(queues) => queues,
        Err(error) => {
            tracing::warn!(%error, "imported queues could not be read back");
            return false;
        }
    };
    let mut live = true;
    for id in ids {
        let Some(queue) = queues.iter().find(|queue| queue.id.to_string() == *id) else {
            tracing::warn!(queue = %id, "imported queue is missing after the import");
            live = false;
            continue;
        };
        live &= bring_queue_live(services, queue).await;
    }
    live
}

async fn bring_queue_live(services: &PackServices, queue: &Queue) -> bool {
    let outcome = match services.scheduler.register(queue.clone()).await {
        Ok(MembershipOutcome::AlreadyRegistered) => {
            services.scheduler.reconfigure(queue.clone()).await
        }
        other => other,
    };
    match outcome {
        Ok(MembershipOutcome::Applied | MembershipOutcome::AlreadyRegistered) => true,
        Ok(MembershipOutcome::NotFound) => {
            tracing::warn!(queue = %queue.id, "imported queue is unknown to the scheduler");
            false
        }
        Err(error) => {
            tracing::warn!(queue = %queue.id, %error, "imported queue could not go live");
            false
        }
    }
}

async fn refresh_scripts(services: &PackServices) -> bool {
    match services.scripts.load_all(services.backend.as_ref()).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "scripts could not be reloaded after the pack import");
            false
        }
    }
}

async fn refresh_overlays(services: &PackServices, ids: &[OverlayId]) -> bool {
    let mut live = true;
    for id in ids {
        if let Err(error) = services.overlays.materialize(id).await {
            tracing::warn!(overlay = %id, %error, "imported overlay could not be regenerated");
            live = false;
        }
    }
    live
}

#[derive(Clone, Default)]
pub struct SetupLiveHandles {
    pub hotkeys: Option<Arc<HotkeyReconciler>>,
    pub speak: Option<SpeakQueueHandle>,
    pub pipeline_config: Option<PipelineConfigHandle>,
    pub bot_accounts: Option<Shared<Vec<String>>>,
}

impl SetupLiveHandles {
    pub fn of(handles: &RuntimeHandles) -> Self {
        Self {
            hotkeys: handles.hotkey_reconciler.clone(),
            speak: handles.speak.clone(),
            pipeline_config: handles.pipeline_config.clone(),
            bot_accounts: Some(handles.bot_accounts.clone()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VoiceBaseline {
    pub alias_ids: Vec<AliasId>,
    pub ignore_profile: IgnoreProfile,
}

pub(crate) async fn voice_baseline(provider: &dyn DataProvider) -> Option<VoiceBaseline> {
    let repo = provider.voice_alias_repo();
    let read = async {
        let aliases = repo.list().await?;
        let ignore_profile = repo.get_ignore_profile().await?;
        Ok::<_, StorageError>(VoiceBaseline {
            alias_ids: aliases.into_iter().map(|alias| alias.id).collect(),
            ignore_profile,
        })
    }
    .await;
    match read {
        Ok(baseline) => Some(baseline),
        Err(error) => {
            tracing::warn!(%error, "voice settings could not be read before the setup import");
            None
        }
    }
}

pub(crate) async fn refresh_setup_live(
    services: &PackServices,
    live: &SetupLiveHandles,
    outcome: &SetupImportOutcome,
    baseline: Option<&VoiceBaseline>,
) -> LiveRefresh {
    let mut applied = true;
    for part in &outcome.refresh {
        applied &= match part {
            SetupLivePart::Queues => apply_queue_changes(services, &outcome.queues).await,
            SetupLivePart::Scripts => refresh_scripts(services).await,
            SetupLivePart::Overlays => materialize_stored_overlays(services).await,
            SetupLivePart::Hotkeys => refresh_hotkeys(live).await,
            SetupLivePart::Tts => refresh_tts(services, live, baseline).await,
        };
    }
    if applied {
        LiveRefresh::Applied
    } else {
        LiveRefresh::NeedsRestart
    }
}

async fn apply_queue_changes(services: &PackServices, changes: &SetupQueueChanges) -> bool {
    let mut live = true;
    for queue in changes.added.iter().chain(&changes.changed) {
        live &= bring_queue_live(services, queue).await;
    }
    for id in &changes.removed {
        match services.scheduler.deregister(*id).await {
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(queue = %id, %error, "removed queue could not leave the scheduler");
                live = false;
            }
        }
    }
    live
}

async fn materialize_stored_overlays(services: &PackServices) -> bool {
    let overlays = match services.backend.overlay_repo().list().await {
        Ok(overlays) => overlays,
        Err(error) => {
            tracing::warn!(%error, "imported overlays could not be read back");
            return false;
        }
    };
    let mut live = true;
    for overlay in overlays {
        match services.overlays.materialize(&overlay.id).await {
            Ok(_) => {}
            Err(OverlayServiceError::UnavailableKind { id, kind_id }) => {
                tracing::info!(overlay = %id, %kind_id, "imported overlay type is not in this build");
            }
            Err(error) => {
                tracing::warn!(overlay = %overlay.id, %error, "imported overlay could not be regenerated");
                live = false;
            }
        }
    }
    live
}

async fn refresh_hotkeys(live: &SetupLiveHandles) -> bool {
    if let Some(reconciler) = &live.hotkeys {
        reconciler.reconcile().await;
    }
    true
}

async fn refresh_tts(
    services: &PackServices,
    live: &SetupLiveHandles,
    baseline: Option<&VoiceBaseline>,
) -> bool {
    let Some(baseline) = baseline else {
        return false;
    };
    let filters = services.backend.tts_filters_repo();
    let voices = services.backend.voice_alias_repo();
    let read = async {
        let rules = filters.list_rules().await?;
        let settings = filters.get_pipeline_settings().await?;
        let aliases = voices.list().await?;
        let strategy = voices.get_strategy().await?;
        let ignore_profile = voices.get_ignore_profile().await?;
        Ok::<_, StorageError>((rules, settings, aliases, strategy, ignore_profile))
    }
    .await;
    let (rules, settings, aliases, strategy, ignore_profile) = match read {
        Ok(read) => read,
        Err(error) => {
            tracing::warn!(%error, "imported speech settings could not be read back");
            return false;
        }
    };
    if let Some(bot_accounts) = &live.bot_accounts {
        bot_accounts.store(settings.bot_accounts.clone());
    }
    if let Some(pipeline) = &live.pipeline_config {
        pipeline.swap(build_config_lenient(&rules, &settings));
    }
    let mut applied = same_ignore_profile(&baseline.ignore_profile, &ignore_profile);
    let Some(speak) = &live.speak else {
        return applied;
    };
    let commands = baseline
        .alias_ids
        .iter()
        .cloned()
        .map(SpeakCommand::RemoveAlias)
        .chain(aliases.into_iter().map(SpeakCommand::SetAlias))
        .chain(std::iter::once(SpeakCommand::SetStrategy(strategy)));
    for command in commands {
        if let Err(error) = speak.send(command).await {
            tracing::warn!(%error, "imported voice settings could not reach the speech queue");
            applied = false;
        }
    }
    applied
}

fn same_ignore_profile(before: &IgnoreProfile, after: &IgnoreProfile) -> bool {
    same_members(&before.excluded_voice_ids, &after.excluded_voice_ids)
        && same_members(&before.excluded_locales, &after.excluded_locales)
}

fn same_members<T: Eq + Hash>(before: &[T], after: &[T]) -> bool {
    before.iter().collect::<HashSet<_>>() == after.iter().collect::<HashSet<_>>()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use forge_hotkey::{HotkeyConfig, HotkeyReconciler};
    use forge_speak_queue::PipelineConfigHandle;
    use forge_storage::{
        AliasId, OverlayId, ScriptRecord, ScriptRepo, StoredClip, TtsPipelineSettings, VoiceAlias,
    };
    use forge_transfer::{
        PackItemKey, PackItemKind, PackWrittenItem, SetupImportOutcome, SetupLivePart,
        SetupQueueChanges,
    };
    use forge_types::{ClipId, OutputDevice, ScriptContract, ScriptId, Shared};
    use forge_voice::{AliasState, AssignmentStrategy, EngineId, IgnoreProfile, VoiceId};
    use time::OffsetDateTime;

    use super::{
        LiveRefresh, LiveTargets, SetupLiveHandles, VoiceBaseline, refresh_live,
        refresh_setup_live, same_ignore_profile, voice_baseline,
    };
    use crate::test_support::pack_flow::{PackWorld, pack_world, queue_named};
    use crate::test_support::{LiveAliases, live_alias, runtime, spawn_speak_queue};

    const KNOWN_OVERLAY_KIND: &str = "overlay.blank";
    const RETIRED_OVERLAY_KIND: &str = "overlay.retired";

    fn written(kind: PackItemKind, local_id: &str) -> PackWrittenItem {
        PackWrittenItem {
            key: PackItemKey {
                kind,
                id: format!("pack-{local_id}"),
            },
            local_id: local_id.to_owned(),
            replaced: false,
        }
    }

    fn refresh(
        rt: &tokio::runtime::Runtime,
        world: &PackWorld,
        targets: LiveTargets,
    ) -> LiveRefresh {
        rt.block_on(refresh_live(&world.services, targets))
    }

    fn stored_overlay(rt: &tokio::runtime::Runtime, world: &PackWorld, kind_id: &str) -> OverlayId {
        rt.block_on(
            world
                .provider()
                .overlay_repo()
                .create("Imported", kind_id, 1),
        )
        .unwrap()
        .id
    }

    #[test]
    fn live_targets_route_queues_scripts_and_overlays_and_ignore_the_rest() {
        let items = [
            written(PackItemKind::Queue, "q1"),
            written(PackItemKind::Script, "s1"),
            written(PackItemKind::Overlay, "o1"),
            written(PackItemKind::Action, "a1"),
            written(PackItemKind::Trigger, "t1"),
            written(PackItemKind::Global, "g1"),
            written(PackItemKind::Clip, "c1"),
            written(PackItemKind::Media, "m1"),
        ];

        assert_eq!(
            LiveTargets::of(&items),
            LiveTargets {
                queues: vec!["q1".to_owned()],
                scripts: true,
                overlays: vec![OverlayId::new("o1")],
            }
        );
    }

    #[test]
    fn an_imported_queue_unknown_to_the_scheduler_is_registered() {
        let rt = runtime();
        let world = pack_world(&rt);
        let queue = queue_named("Imported queue");
        rt.block_on(world.provider().queue_repo().save(&queue))
            .unwrap();

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                queues: vec![queue.id.to_string()],
                ..LiveTargets::default()
            },
        );
        let states = rt
            .block_on(world.services.scheduler.queue_states())
            .unwrap();

        assert_eq!(
            (live, states.contains_key(&queue.id)),
            (LiveRefresh::Applied, true)
        );
    }

    #[test]
    fn a_replaced_queue_the_scheduler_already_runs_stays_live() {
        let rt = runtime();
        let world = pack_world(&rt);
        let mut queue = queue_named("Replaced queue");
        rt.block_on(world.services.scheduler.register(queue.clone()))
            .unwrap();
        queue.concurrency = 4;
        rt.block_on(world.provider().queue_repo().save(&queue))
            .unwrap();

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                queues: vec![queue.id.to_string()],
                ..LiveTargets::default()
            },
        );

        assert_eq!(live, LiveRefresh::Applied);
    }

    #[test]
    fn a_written_queue_missing_from_storage_needs_a_restart() {
        let rt = runtime();
        let world = pack_world(&rt);

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                queues: vec![queue_named("Vanished").id.to_string()],
                ..LiveTargets::default()
            },
        );

        assert_eq!(live, LiveRefresh::NeedsRestart);
    }

    #[test]
    fn a_written_script_reloads_the_script_registry() {
        let rt = runtime();
        let world = pack_world(&rt);
        let now = OffsetDateTime::now_utc();
        rt.block_on(ScriptRepo::save(
            world.provider().as_ref(),
            ScriptRecord {
                id: ScriptId::new(),
                name: "imported_greet".to_owned(),
                body: "let greeting = 1;".to_owned(),
                contract: ScriptContract::default(),
                body_hash: String::new(),
                enabled: true,
                created_at: now,
                last_modified: now,
            },
        ))
        .unwrap();

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                scripts: true,
                ..LiveTargets::default()
            },
        );
        let loaded = rt
            .block_on(world.services.scripts.get_by_name("imported_greet"))
            .is_some();

        assert_eq!((live, loaded), (LiveRefresh::Applied, true));
    }

    #[test]
    fn a_written_overlay_is_materialized_under_the_overlay_root() {
        let rt = runtime();
        let world = pack_world(&rt);
        let overlay = stored_overlay(&rt, &world, KNOWN_OVERLAY_KIND);

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                overlays: vec![overlay.clone()],
                ..LiveTargets::default()
            },
        );
        let page_dir = world.overlay_root().join(overlay.as_str());

        assert_eq!((live, page_dir.is_dir()), (LiveRefresh::Applied, true));
    }

    #[test]
    fn one_overlay_that_cannot_be_materialized_needs_a_restart_even_when_the_rest_went_live() {
        let rt = runtime();
        let world = pack_world(&rt);
        let queue = queue_named("Healthy queue");
        rt.block_on(world.provider().queue_repo().save(&queue))
            .unwrap();
        let healthy = stored_overlay(&rt, &world, KNOWN_OVERLAY_KIND);
        let retired = stored_overlay(&rt, &world, RETIRED_OVERLAY_KIND);

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                queues: vec![queue.id.to_string()],
                scripts: true,
                overlays: vec![retired, healthy.clone()],
            },
        );
        let healthy_dir = world.overlay_root().join(healthy.as_str());

        assert_eq!(
            (live, healthy_dir.is_dir()),
            (LiveRefresh::NeedsRestart, true)
        );
    }

    const SPEECH_QUEUE_DEADLINE: Duration = Duration::from_secs(30);
    const IMPORTED_BOT: &str = "imported-bot";

    struct SilentPublisher;

    impl forge_events::EventPublisher for SilentPublisher {
        fn publish(&self, _: forge_events::Event) {}
    }

    fn setup_outcome(parts: &[SetupLivePart], queues: SetupQueueChanges) -> SetupImportOutcome {
        SetupImportOutcome {
            safety_snapshot: PathBuf::from("forge-pre-setup.forgebundle"),
            refresh: parts.iter().copied().collect::<BTreeSet<_>>(),
            queues,
            restart_recommended: false,
            cleanup_failures: Vec::new(),
        }
    }

    fn refresh_setup(
        rt: &tokio::runtime::Runtime,
        world: &PackWorld,
        live: &SetupLiveHandles,
        outcome: &SetupImportOutcome,
        baseline: Option<&VoiceBaseline>,
    ) -> LiveRefresh {
        rt.block_on(refresh_setup_live(&world.services, live, outcome, baseline))
    }

    fn voice_alias(viewer_id: &str) -> VoiceAlias {
        VoiceAlias {
            id: AliasId::new(),
            viewer_id: viewer_id.to_owned(),
            viewer_name: viewer_id.to_owned(),
            engine_id: EngineId("piper".to_owned()),
            voice_id: VoiceId("amy".to_owned()),
            pitch_semitones: None,
            rate_multiplier: None,
            state: AliasState::Active,
        }
    }

    fn excluding(locale: &str) -> IgnoreProfile {
        IgnoreProfile {
            excluded_voice_ids: Vec::new(),
            excluded_locales: vec![locale.to_owned()],
        }
    }

    fn store_ignore_profile(
        rt: &tokio::runtime::Runtime,
        world: &PackWorld,
        profile: &IgnoreProfile,
    ) {
        rt.block_on(
            world
                .provider()
                .voice_alias_repo()
                .set_ignore_profile(profile),
        )
        .unwrap();
    }

    fn strategy_applied(resolver: &LiveAliases, strategy: &AssignmentStrategy) -> bool {
        resolver.read().unwrap().strategy == *strategy
    }

    #[test]
    fn setup_queue_changes_add_change_and_remove_scheduler_queues() {
        let rt = runtime();
        let world = pack_world(&rt);
        let scheduler = &world.services.scheduler;
        let mut changed = queue_named("Changed");
        let removed = queue_named("Removed");
        rt.block_on(scheduler.register(changed.clone())).unwrap();
        rt.block_on(scheduler.register(removed.clone())).unwrap();
        changed.concurrency = 4;
        let added = queue_named("Added");
        let outcome = setup_outcome(
            &[SetupLivePart::Queues],
            SetupQueueChanges {
                added: vec![added.clone()],
                changed: vec![changed.clone()],
                removed: vec![removed.id],
            },
        );

        let live = refresh_setup(&rt, &world, &SetupLiveHandles::default(), &outcome, None);
        let states = rt.block_on(scheduler.queue_states()).unwrap();
        let running: BTreeSet<_> = states.keys().copied().collect();

        assert_eq!(
            (live, running),
            (LiveRefresh::Applied, BTreeSet::from([added.id, changed.id]))
        );
    }

    #[test]
    fn setup_queue_changes_the_scheduler_cannot_take_need_a_restart() {
        let stopped = runtime();
        let world = pack_world(&stopped);
        drop(stopped);
        let rt = runtime();
        let outcome = setup_outcome(
            &[SetupLivePart::Queues],
            SetupQueueChanges {
                removed: vec![queue_named("Removed").id],
                ..SetupQueueChanges::default()
            },
        );

        let live = refresh_setup(&rt, &world, &SetupLiveHandles::default(), &outcome, None);

        assert_eq!(live, LiveRefresh::NeedsRestart);
    }

    #[test]
    fn a_setup_overlay_of_a_kind_missing_from_this_build_does_not_need_a_restart() {
        let rt = runtime();
        let world = pack_world(&rt);
        let healthy = stored_overlay(&rt, &world, KNOWN_OVERLAY_KIND);
        stored_overlay(&rt, &world, RETIRED_OVERLAY_KIND);
        let outcome = setup_outcome(&[SetupLivePart::Overlays], SetupQueueChanges::default());

        let live = refresh_setup(&rt, &world, &SetupLiveHandles::default(), &outcome, None);
        let healthy_dir = world.overlay_root().join(healthy.as_str());

        assert_eq!((live, healthy_dir.is_dir()), (LiveRefresh::Applied, true));
    }

    #[test]
    fn setup_hotkeys_reconcile_the_imported_clip_hotkeys() {
        let rt = runtime();
        let world = pack_world(&rt);
        let clip = ClipId::new();
        rt.block_on(world.provider().soundboard_clips_repo().save(&StoredClip {
            id: clip,
            name: "Airhorn".to_owned(),
            file_path: PathBuf::from("airhorn.ogg"),
            volume: 1.0,
            output_device: OutputDevice::Default,
            hotkey: Some("F9".to_owned()),
            created_at: OffsetDateTime::now_utc(),
            category: String::new(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
        }))
        .unwrap();
        let reconciler = rt.block_on(async {
            let (client, _recorder) = forge_hotkey::testing::test_client(
                HotkeyConfig::default(),
                Arc::new(SilentPublisher),
            );
            HotkeyReconciler::new(
                client,
                world.provider().trigger_instance_repo(),
                world.provider().soundboard_clips_repo(),
            )
        });
        let live = SetupLiveHandles {
            hotkeys: Some(Arc::clone(&reconciler)),
            ..SetupLiveHandles::default()
        };
        let outcome = setup_outcome(&[SetupLivePart::Hotkeys], SetupQueueChanges::default());

        let refreshed = refresh_setup(&rt, &world, &live, &outcome, None);
        let bound: Vec<ClipId> = reconciler
            .clip_bindings()
            .borrow()
            .values()
            .flatten()
            .copied()
            .collect();

        assert_eq!((refreshed, bound), (LiveRefresh::Applied, vec![clip]));
    }

    fn profile(voices: &[&str], locales: &[&str]) -> IgnoreProfile {
        IgnoreProfile {
            excluded_voice_ids: voices.iter().map(|v| VoiceId((*v).to_owned())).collect(),
            excluded_locales: locales.iter().map(|l| (*l).to_owned()).collect(),
        }
    }

    #[test]
    fn ignore_profiles_with_the_same_members_in_any_order_or_multiplicity_are_the_same() {
        for (before, after) in [
            (profile(&["amy", "joe"], &[]), profile(&["joe", "amy"], &[])),
            (
                profile(&[], &["de-DE", "fr-FR"]),
                profile(&[], &["fr-FR", "de-DE"]),
            ),
            (profile(&[], &[]), profile(&[], &[])),
            (profile(&["amy", "amy"], &[]), profile(&["amy"], &[])),
            (profile(&[], &["de-DE", "de-DE"]), profile(&[], &["de-DE"])),
        ] {
            assert!(
                same_ignore_profile(&before, &after) && same_ignore_profile(&after, &before),
                "{before:?} vs {after:?}"
            );
        }
    }

    #[test]
    fn ignore_profiles_with_different_voice_or_locale_members_differ() {
        for (before, after) in [
            (profile(&["amy"], &["de-DE"]), profile(&["joe"], &["de-DE"])),
            (profile(&["amy"], &["de-DE"]), profile(&["amy"], &["fr-FR"])),
            (profile(&[], &[]), profile(&["amy"], &[])),
            (profile(&[], &[]), profile(&[], &["de-DE"])),
            (profile(&["amy", "amy"], &[]), profile(&["amy", "joe"], &[])),
            (
                profile(&[], &["de-DE", "de-DE"]),
                profile(&[], &["de-DE", "fr-FR"]),
            ),
        ] {
            assert!(
                !same_ignore_profile(&before, &after) && !same_ignore_profile(&after, &before),
                "{before:?} vs {after:?}"
            );
        }
    }

    #[test]
    fn setup_speech_needs_a_restart_only_when_the_ignore_profile_changed_or_is_unknown() {
        let rt = runtime();
        let world = pack_world(&rt);
        let stored = excluding("de-DE");
        store_ignore_profile(&rt, &world, &stored);
        let outcome = setup_outcome(&[SetupLivePart::Tts], SetupQueueChanges::default());
        let mut refreshed = Vec::new();

        for before in [
            Some(stored.clone()),
            Some(excluding("fr-FR")),
            Some(IgnoreProfile {
                excluded_voice_ids: vec![VoiceId("amy".to_owned())],
                ..stored.clone()
            }),
            None,
        ] {
            let baseline = before.map(|ignore_profile| VoiceBaseline {
                alias_ids: Vec::new(),
                ignore_profile,
            });
            refreshed.push(refresh_setup(
                &rt,
                &world,
                &SetupLiveHandles::default(),
                &outcome,
                baseline.as_ref(),
            ));
        }

        assert_eq!(
            refreshed,
            vec![
                LiveRefresh::Applied,
                LiveRefresh::NeedsRestart,
                LiveRefresh::NeedsRestart,
                LiveRefresh::NeedsRestart,
            ]
        );
    }

    #[test]
    fn setup_speech_swaps_the_previous_aliases_for_the_imported_ones_and_their_strategy() {
        let rt = runtime();
        let world = pack_world(&rt);
        let previous = voice_alias("twitch:previous");
        let imported = voice_alias("twitch:imported");
        let voices = world.provider().voice_alias_repo();
        rt.block_on(voices.upsert(&imported)).unwrap();
        rt.block_on(voices.set_strategy(&AssignmentStrategy::Random))
            .unwrap();
        let (speak, _events, resolver) =
            rt.block_on(async { spawn_speak_queue(vec![previous.clone()]) });
        let baseline = VoiceBaseline {
            alias_ids: vec![previous.id.clone()],
            ignore_profile: IgnoreProfile::default(),
        };
        let live = SetupLiveHandles {
            speak: Some(speak),
            ..SetupLiveHandles::default()
        };
        let outcome = setup_outcome(&[SetupLivePart::Tts], SetupQueueChanges::default());

        let refreshed = refresh_setup(&rt, &world, &live, &outcome, Some(&baseline));
        rt.block_on(async {
            tokio::time::timeout(SPEECH_QUEUE_DEADLINE, async {
                while !strategy_applied(&resolver, &AssignmentStrategy::Random) {
                    tokio::task::yield_now().await;
                }
            })
            .await
        })
        .expect("the speech queue never applied the imported strategy");

        assert_eq!(
            (
                refreshed,
                live_alias(&resolver, "twitch:previous").is_some(),
                live_alias(&resolver, "twitch:imported").map(|alias| alias.id),
            ),
            (LiveRefresh::Applied, false, Some(imported.id))
        );
    }

    #[test]
    fn setup_speech_publishes_the_imported_bot_accounts_and_filter_pipeline() {
        let rt = runtime();
        let world = pack_world(&rt);
        rt.block_on(world.provider().tts_filters_repo().set_pipeline_settings(
            &TtsPipelineSettings {
                bot_accounts: vec![IMPORTED_BOT.to_owned()],
                ..TtsPipelineSettings::default()
            },
        ))
        .unwrap();
        let baseline = rt
            .block_on(voice_baseline(world.provider().as_ref()))
            .unwrap();
        let bot_accounts = Shared::new(Vec::<String>::new());
        let pipeline = PipelineConfigHandle::new(forge_tts_pipeline::PipelineConfig::default());
        let live = SetupLiveHandles {
            pipeline_config: Some(pipeline.clone()),
            bot_accounts: Some(bot_accounts.clone()),
            ..SetupLiveHandles::default()
        };
        let outcome = setup_outcome(&[SetupLivePart::Tts], SetupQueueChanges::default());

        let refreshed = refresh_setup(&rt, &world, &live, &outcome, Some(&baseline));

        let imported = vec![IMPORTED_BOT.to_owned()];
        assert_eq!(
            (
                refreshed,
                bot_accounts.load().as_ref().clone(),
                pipeline.load().skip_rules.bot_accounts.clone(),
            ),
            (LiveRefresh::Applied, imported.clone(), imported)
        );
    }
}
