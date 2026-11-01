use super::*;
use crate::app::async_bridge;
use crate::audio::engine_voice_choices::engine_voice_options;
use crate::integrations::donation_services::donation_provider_options;
use crate::shared::config_form::dependent_options_key;
use crate::shared::option_choices::ChoiceOption;
use forge_components::tr;
use forge_registry::FormSchemaSource;
pub(super) use forge_registry::SOUNDBOARD_CLIP_OPTIONS_KEY as CLIP_OPTIONS_KEY;
use forge_registry::{
    ACTION_OPTIONS_KEY, GLOBAL_OPTIONS_KEY, OVERLAY_OPTIONS_KEY, QUEUE_OPTIONS_KEY,
    SCRIPT_OPTIONS_KEY, TRIGGER_INSTANCE_OPTIONS_KEY,
};
use forge_runtime::DONATION_PROVIDER_OPTIONS_KEY;
use forge_runtime::{ENGINE_OPTIONS_KEY, VOICE_OPTIONS_PREFIX};
use forge_soundboard::ClipLibrary;
use forge_speak_queue::SpeakQueueHandle;
use forge_speech_types::ENGINE_VOICE_OPTIONS_KEY;
use forge_storage::{GlobalsRepo, OverlayRepo, QueueRepo, ScriptRepo};
use gpui::Context;
use std::collections::HashMap;

struct SelectOptionsFetch {
    options: HashMap<String, Vec<ChoiceOption>>,
    failed_keys: Vec<&'static str>,
    overlay_kind_by_identity: HashMap<String, String>,
    concurrent_queue_ids: HashSet<String>,
    unavailable_clip_ids: HashSet<String>,
}

fn loaded_options<T, E: std::fmt::Display>(
    result: Result<T, E>,
    key: &'static str,
    failed_keys: &mut Vec<&'static str>,
) -> Option<T> {
    match result {
        Ok(listed) => Some(listed),
        Err(error) => {
            tracing::warn!(%error, options = key, "picker options failed to load");
            failed_keys.push(key);
            None
        }
    }
}

fn mark_unavailable_clips(clips: &mut [ChoiceOption], unavailable: &HashSet<String>, suffix: &str) {
    for ChoiceOption { value, label } in clips.iter_mut() {
        if unavailable.contains(value) {
            *label = format!("{label} - {suffix}");
        }
    }
}

struct SelectOptionSources {
    action_repo: Arc<dyn ActionRepo>,
    queue_repo: Arc<dyn QueueRepo>,
    trigger_instance_repo: Arc<dyn TriggerInstanceRepo>,
    script_repo: Arc<dyn ScriptRepo>,
    clip_library: Arc<ClipLibrary>,
    globals_repo: Arc<dyn GlobalsRepo>,
    overlay_repo: Arc<dyn OverlayRepo>,
    overlay_schema: Arc<overlay_schema::OverlayContentSchema>,
    speak: Option<SpeakQueueHandle>,
}

impl SelectOptionsFetch {
    fn seeded() -> Self {
        Self {
            options: HashMap::from([(
                DONATION_PROVIDER_OPTIONS_KEY.to_owned(),
                donation_provider_options(),
            )]),
            failed_keys: Vec::new(),
            overlay_kind_by_identity: HashMap::new(),
            concurrent_queue_ids: HashSet::new(),
            unavailable_clip_ids: HashSet::new(),
        }
    }

    fn listed<T, E: std::fmt::Display>(
        &mut self,
        result: Result<T, E>,
        key: &'static str,
    ) -> Option<T> {
        loaded_options(result, key, &mut self.failed_keys)
    }

    fn put(&mut self, key: &str, entries: impl Iterator<Item = ChoiceOption>) {
        self.options.insert(key.to_owned(), entries.collect());
    }
}

impl SelectOptionSources {
    async fn fetch(self) -> SelectOptionsFetch {
        let mut fetch = SelectOptionsFetch::seeded();
        self.add_actions(&mut fetch).await;
        self.add_queues(&mut fetch).await;
        self.add_trigger_instances(&mut fetch).await;
        self.add_scripts(&mut fetch).await;
        self.add_clips(&mut fetch).await;
        self.add_globals(&mut fetch).await;
        self.add_overlays(&mut fetch).await;
        self.add_speech(&mut fetch);
        fetch
    }

    async fn add_actions(&self, fetch: &mut SelectOptionsFetch) {
        if let Some(actions) = fetch.listed(self.action_repo.list().await, ACTION_OPTIONS_KEY) {
            fetch.put(
                ACTION_OPTIONS_KEY,
                actions
                    .into_iter()
                    .map(|a| ChoiceOption::new(a.id.to_string(), a.name)),
            );
        }
    }

    async fn add_queues(&self, fetch: &mut SelectOptionsFetch) {
        if let Some(queues) = fetch.listed(self.queue_repo.list().await, QUEUE_OPTIONS_KEY) {
            fetch.concurrent_queue_ids = queues
                .iter()
                .filter(|q| !q.is_serial())
                .map(|q| q.id.to_string())
                .collect();
            fetch.put(
                QUEUE_OPTIONS_KEY,
                queues
                    .into_iter()
                    .map(|q| ChoiceOption::new(q.id.to_string(), q.name)),
            );
        }
    }

    async fn add_trigger_instances(&self, fetch: &mut SelectOptionsFetch) {
        if let Some(instances) = fetch.listed(
            self.trigger_instance_repo.list_all().await,
            TRIGGER_INSTANCE_OPTIONS_KEY,
        ) {
            fetch.put(
                TRIGGER_INSTANCE_OPTIONS_KEY,
                instances
                    .into_iter()
                    .map(|ti| ChoiceOption::new(ti.id.to_string(), ti.name)),
            );
        }
    }

    async fn add_scripts(&self, fetch: &mut SelectOptionsFetch) {
        if let Some(scripts) = fetch.listed(self.script_repo.list().await, SCRIPT_OPTIONS_KEY) {
            fetch.put(
                SCRIPT_OPTIONS_KEY,
                scripts
                    .into_iter()
                    .map(|s| ChoiceOption::new(s.name.clone(), s.name)),
            );
        }
    }

    async fn add_clips(&self, fetch: &mut SelectOptionsFetch) {
        if let Some(clips) = fetch.listed(self.clip_library.list().await, CLIP_OPTIONS_KEY) {
            fetch.unavailable_clip_ids = self
                .clip_library
                .availability_of(&clips)
                .await
                .into_iter()
                .filter(|(_, availability)| !availability.is_playable())
                .map(|(id, _)| id.to_string())
                .collect();
            fetch.put(
                CLIP_OPTIONS_KEY,
                clips
                    .into_iter()
                    .map(|c| ChoiceOption::new(c.id.to_string(), c.name)),
            );
        }
    }

    async fn add_globals(&self, fetch: &mut SelectOptionsFetch) {
        if let Some(globals) = fetch.listed(self.globals_repo.list().await, GLOBAL_OPTIONS_KEY) {
            fetch.put(
                GLOBAL_OPTIONS_KEY,
                globals
                    .into_iter()
                    .map(|g| ChoiceOption::new(g.name.clone(), g.name)),
            );
        }
    }

    async fn add_overlays(&self, fetch: &mut SelectOptionsFetch) {
        if let Some(overlays) = fetch.listed(self.overlay_repo.list().await, OVERLAY_OPTIONS_KEY) {
            fetch.put(
                OVERLAY_OPTIONS_KEY,
                overlays
                    .iter()
                    .filter(|o| self.overlay_schema.takes_sends(&o.kind_id))
                    .map(|o| ChoiceOption::new(o.id.to_string(), o.display_name.clone())),
            );
            fetch.overlay_kind_by_identity = overlays
                .into_iter()
                .map(|o| (o.id.to_string(), o.kind_id))
                .collect();
        }
    }

    fn add_speech(&self, fetch: &mut SelectOptionsFetch) {
        let Some(speak) = &self.speak else {
            return;
        };
        fetch.put(
            ENGINE_OPTIONS_KEY,
            speak
                .registered_engine_ids()
                .into_iter()
                .map(|id| ChoiceOption::new(id.0.clone(), id.0)),
        );
        let voices = speak.available_voices();
        for voice in voices.iter() {
            fetch
                .options
                .entry(dependent_options_key(
                    VOICE_OPTIONS_PREFIX,
                    &voice.engine_id.0,
                ))
                .or_default()
                .push(ChoiceOption::new(voice.id.0.clone(), voice.name.clone()));
        }
        fetch.options.insert(
            ENGINE_VOICE_OPTIONS_KEY.to_owned(),
            engine_voice_options(&voices),
        );
    }
}

impl ActionEditor {
    pub(in crate::automation::actions_screen) fn fetch_select_options(
        &self,
        cx: &mut Context<Self>,
    ) {
        let sources = SelectOptionSources {
            action_repo: Arc::clone(&self.deps.action_repo),
            queue_repo: Arc::clone(&self.deps.queue_repo),
            trigger_instance_repo: Arc::clone(&self.deps.trigger_instance_repo),
            script_repo: Arc::clone(&self.deps.script_repo),
            clip_library: Arc::clone(&self.deps.clip_library),
            globals_repo: Arc::clone(&self.deps.globals_repo),
            overlay_repo: Arc::clone(&self.deps.overlay_repo),
            overlay_schema: Arc::clone(&self.overlay_schema),
            speak: self.deps.speak.clone(),
        };
        async_bridge::run_async(
            &self.deps.rt_handle,
            sources.fetch(),
            |this, fetch, cx| this.on_select_options_fetched(fetch, cx),
            cx,
        );
    }

    fn on_select_options_fetched(&mut self, fetch: SelectOptionsFetch, cx: &mut Context<Self>) {
        let SelectOptionsFetch {
            mut options,
            failed_keys,
            overlay_kind_by_identity,
            concurrent_queue_ids,
            unavailable_clip_ids,
        } = fetch;
        if let Some(clips) = options.get_mut(CLIP_OPTIONS_KEY) {
            mark_unavailable_clips(
                clips,
                &unavailable_clip_ids,
                &tr!("soundboard_pad_source_missing"),
            );
        }
        if !failed_keys.contains(&OVERLAY_OPTIONS_KEY) {
            self.overlay_schema = Arc::new(
                self.overlay_schema
                    .with_identities(overlay_kind_by_identity),
            );
        }
        if !failed_keys.contains(&QUEUE_OPTIONS_KEY) {
            self.concurrent_queue_ids = concurrent_queue_ids;
        }
        for key in &failed_keys {
            if let Some(previous) = self.choices.select_options.remove(*key) {
                options.insert((*key).to_owned(), previous);
            }
        }
        let failed_anew = !failed_keys.is_empty() && !self.choices.select_options_failed;
        self.choices.select_options_failed = !failed_keys.is_empty();
        if failed_anew {
            cx.push_toast(ToastKind::Error, tr!("action_editor_options_load_failed"));
        }
        self.choices.select_options = options;
        self.choices
            .select_options
            .extend(self.choices.integration_options.clone());
        if let Some(form) = self.modals.sub_form.clone() {
            let schema = Arc::clone(&self.overlay_schema) as Arc<dyn FormSchemaSource>;
            let merged = self.choices.select_options.clone();
            form.update(cx, |form, cx| {
                form.apply_options(&merged, cx);
                form.set_schema(schema, cx);
            });
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::{
        MEDIA_CONTENT_DIGEST_BYTES, MediaBlobId, MockMediaRepo, StoredClip, clip_source_referrer,
    };
    use forge_types::{ClipId, OutputDevice};
    use time::OffsetDateTime;

    use super::super::tests::{screen_parts, view_with};
    use super::*;
    use crate::test_support::{StubActions, error_toasts, pump_until, runtime};
    use forge_storage::MockActionRepo;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    const MISSING: &str = "file missing";

    fn options(entries: &[(&str, &str)]) -> Vec<ChoiceOption> {
        entries
            .iter()
            .map(|(id, label)| ChoiceOption::new(*id, *label))
            .collect()
    }

    #[test]
    fn only_clips_in_the_unavailable_set_get_the_missing_suffix() {
        let mut clips = options(&[("a", "Airhorn"), ("b", "Bell"), ("c", "Cheer")]);
        let unavailable: HashSet<String> = ["b", "zz"].into_iter().map(str::to_owned).collect();

        mark_unavailable_clips(&mut clips, &unavailable, MISSING);

        assert_eq!(
            clips,
            options(&[
                ("a", "Airhorn"),
                ("b", "Bell - file missing"),
                ("c", "Cheer")
            ])
        );
    }

    fn stored_clip(name: &str, file_path: std::path::PathBuf) -> StoredClip {
        StoredClip {
            id: ClipId::new(),
            name: name.to_owned(),
            file_path,
            volume: 1.0,
            output_device: OutputDevice::Default,
            hotkey: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            category: String::new(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
        }
    }

    fn media_holding(clip: ClipId, file: std::path::PathBuf) -> MockMediaRepo {
        let blob = MediaBlobId::from_digest(&[7; MEDIA_CONTENT_DIGEST_BYTES]);
        let held = clip_source_referrer(clip);
        let mut media = MockMediaRepo::new();
        media
            .expect_blob_of()
            .returning(move |referrer| Ok((*referrer == held).then(|| blob.clone())));
        media.expect_resolve().returning(move |_| Ok(file.clone()));
        media
    }

    fn view_over(
        cx: &mut gpui::TestAppContext,
        rt: &tokio::runtime::Runtime,
        clips: Vec<StoredClip>,
        media: MockMediaRepo,
    ) -> (Entity<ActionEditor>, Entity<ScreenActionsView>) {
        let host = view_with(
            cx,
            rt,
            clips,
            media,
            SubActionRegistry::new(),
            Arc::new(StubActions),
        );
        (screen_parts(cx, &host).0, host)
    }

    #[gpui::test]
    fn the_clip_picker_marks_a_clip_whose_file_is_gone(cx: &mut gpui::TestAppContext) {
        crate::app::i18n::install_language(forge_storage::Language::En);
        let rt = runtime();
        let managed_file = tempfile::NamedTempFile::new().unwrap();
        let gone = std::path::PathBuf::from("/nonexistent/forge/ghost.wav");
        let ghost = stored_clip("Ghost clip", gone.clone());
        let real = stored_clip("Real clip", gone);
        let media = media_holding(real.id, managed_file.path().to_owned());
        let expected = options(&[
            (&ghost.id.to_string(), "Ghost clip - file missing"),
            (&real.id.to_string(), "Real clip"),
        ]);
        let (view, _host) = view_over(cx, &rt, vec![ghost, real], media);

        view.update(cx, |view, cx| view.fetch_select_options(cx));
        let mut listed = None;
        pump_until(&rt, || {
            cx.run_until_parked();
            listed = view.read_with(cx, |view, _| {
                view.choices.select_options.get(CLIP_OPTIONS_KEY).cloned()
            });
            listed.is_some()
        });

        assert_eq!(listed, Some(expected));
    }

    fn clip_detail(
        value: String,
        options: &std::collections::HashMap<
            String,
            Vec<crate::shared::option_choices::ChoiceOption>,
        >,
    ) -> String {
        super::super::step_presentation::shown_card_detail(
            forge_registry::StepCardDetail::OptionLabel {
                options_key: CLIP_OPTIONS_KEY,
                value,
            },
            options,
        )
    }

    #[gpui::test]
    fn a_freshly_mounted_editor_names_play_sound_clips_without_opening_a_step_form(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::app::i18n::install_language(forge_storage::Language::En);
        let rt = runtime();
        let managed_file = tempfile::NamedTempFile::new().unwrap();
        let clip = stored_clip(
            "Airhorn",
            std::path::PathBuf::from("/nonexistent/forge/airhorn.wav"),
        );
        let known = clip.id.to_string();
        let dangling = ClipId::new().to_string();
        let media = media_holding(clip.id, managed_file.path().to_owned());
        let (view, _host) = view_over(cx, &rt, vec![clip], media);

        let mut details = None;
        pump_until(&rt, || {
            cx.run_until_parked();
            details = view.read_with(cx, |view, _| {
                view.choices
                    .select_options
                    .contains_key(CLIP_OPTIONS_KEY)
                    .then(|| {
                        (
                            clip_detail(known.clone(), &view.choices.select_options),
                            clip_detail(dangling.clone(), &view.choices.select_options),
                        )
                    })
            });
            details.is_some()
        });

        assert_eq!(
            details,
            Some(("Airhorn".to_owned(), "Missing clip".to_owned()))
        );
    }

    struct FlakyListing {
        view: Entity<ActionEditor>,
        _host: Entity<ScreenActionsView>,
        lists_fail: Arc<AtomicBool>,
        lists: Arc<AtomicUsize>,
        rt: tokio::runtime::Runtime,
    }

    fn flaky_action_listing(cx: &mut gpui::TestAppContext) -> FlakyListing {
        crate::app::i18n::install_language(forge_storage::Language::En);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let rt = runtime();
        let lists_fail = Arc::new(AtomicBool::new(false));
        let lists = Arc::new(AtomicUsize::new(0));
        let listed = crate::test_support::action_running("Alpha", Vec::new());
        let (failing, counter) = (Arc::clone(&lists_fail), Arc::clone(&lists));
        let mut repo = MockActionRepo::new();
        repo.expect_list().returning(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            if failing.load(Ordering::SeqCst) {
                Err(forge_storage::StorageError::Connection {
                    reason: "database is locked".to_owned(),
                })
            } else {
                Ok(vec![listed.clone()])
            }
        });
        let host = view_with(
            cx,
            &rt,
            Vec::new(),
            MockMediaRepo::new(),
            SubActionRegistry::new(),
            Arc::new(repo),
        );
        let listing = FlakyListing {
            view: screen_parts(cx, &host).0,
            _host: host,
            lists_fail,
            lists,
            rt,
        };
        pump_until(&listing.rt, || {
            cx.run_until_parked();
            listing.action_options(cx).is_some()
        });
        listing
    }

    impl FlakyListing {
        fn action_options(&self, cx: &mut gpui::TestAppContext) -> Option<Vec<ChoiceOption>> {
            self.view.read_with(cx, |view, _| {
                view.choices.select_options.get(ACTION_OPTIONS_KEY).cloned()
            })
        }

        fn failed(&self, cx: &mut gpui::TestAppContext) -> bool {
            self.view
                .read_with(cx, |view, _| view.choices.select_options_failed)
        }

        fn refetch(&self, cx: &mut gpui::TestAppContext, fail: bool) {
            self.lists_fail.store(fail, Ordering::SeqCst);
            let listed_before = self.lists.load(Ordering::SeqCst);
            self.view
                .update(cx, |view, cx| view.fetch_select_options(cx));
            pump_until(&self.rt, || {
                cx.run_until_parked();
                self.lists.load(Ordering::SeqCst) > listed_before && self.failed(cx) == fail
            });
        }
    }

    #[gpui::test]
    fn a_picker_whose_list_failed_to_load_keeps_the_options_it_had(cx: &mut gpui::TestAppContext) {
        let listing = flaky_action_listing(cx);
        let before = listing.action_options(cx);

        listing.refetch(cx, true);

        assert_eq!(listing.action_options(cx), before);
    }

    #[gpui::test]
    fn a_picker_list_failing_on_every_fetch_toasts_once(cx: &mut gpui::TestAppContext) {
        let listing = flaky_action_listing(cx);

        listing.refetch(cx, true);
        listing.refetch(cx, true);
        listing.refetch(cx, false);

        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
    }

    #[gpui::test]
    fn a_picker_list_failing_again_after_recovering_toasts_again(cx: &mut gpui::TestAppContext) {
        let listing = flaky_action_listing(cx);

        listing.refetch(cx, true);
        listing.refetch(cx, false);
        listing.refetch(cx, true);

        assert_eq!(cx.update(|cx| error_toasts(cx)), 2);
    }

    #[gpui::test]
    fn a_failed_queue_list_keeps_what_the_screen_knew_about_concurrent_queues(
        cx: &mut gpui::TestAppContext,
    ) {
        let listing = flaky_action_listing(cx);
        let known: HashSet<String> = ["queue-1".to_owned()].into_iter().collect();
        listing
            .view
            .update(cx, |view, _| view.concurrent_queue_ids = known.clone());

        listing.view.update(cx, |view, cx| {
            view.on_select_options_fetched(
                SelectOptionsFetch {
                    options: HashMap::new(),
                    failed_keys: vec![QUEUE_OPTIONS_KEY],
                    overlay_kind_by_identity: HashMap::new(),
                    concurrent_queue_ids: HashSet::new(),
                    unavailable_clip_ids: HashSet::new(),
                },
                cx,
            );
        });

        let kept = listing
            .view
            .read_with(cx, |view, _| view.concurrent_queue_ids.clone());
        assert_eq!(kept, known);
    }

    #[gpui::test]
    fn a_failed_overlay_list_keeps_the_identities_the_content_form_reads(
        cx: &mut gpui::TestAppContext,
    ) {
        const IDENTITY: &str = "overlay-1";
        let listing = flaky_action_listing(cx);
        let mut kinds = forge_overlay::OverlayKindRegistry::new();
        forge_overlay::register_builtin_kinds(&mut kinds).unwrap();
        let known: HashMap<String, String> =
            [(IDENTITY.to_owned(), "overlay.alert".to_owned())].into();
        let content_fields = |view: &ActionEditor| {
            view.overlay_schema
                .fields_for(forge_runtime::CONTENT_SCHEMA_KEY, IDENTITY)
                .len()
        };
        let before = listing.view.update(cx, |view, _| {
            view.overlay_schema = Arc::new(
                crate::automation::actions_screen::overlay_schema::OverlayContentSchema::new(
                    Arc::new(kinds),
                )
                .with_identities(known),
            );
            content_fields(view)
        });

        listing.view.update(cx, |view, cx| {
            view.on_select_options_fetched(
                SelectOptionsFetch {
                    options: HashMap::new(),
                    failed_keys: vec![OVERLAY_OPTIONS_KEY],
                    overlay_kind_by_identity: HashMap::new(),
                    concurrent_queue_ids: HashSet::new(),
                    unavailable_clip_ids: HashSet::new(),
                },
                cx,
            );
        });

        let after = listing.view.read_with(cx, |view, _| content_fields(view));
        assert_eq!((before > 0, after), (true, before));
    }
}
