use super::*;
use crate::async_bridge;
use crate::donation_services::donation_provider_options;
use crate::tts_engines::engine_label;
use forge_components::tr;
use forge_registry::FormSchemaSource;
use forge_runtime::triggers::DONATION_PROVIDER_OPTIONS_KEY;
use forge_runtime::{ENGINE_VOICE_OPTIONS_KEY, EngineVoiceRef};
use forge_tts_core::TtsVoice;
use gpui::Context;
use std::collections::HashMap;

const CLIP_OPTIONS_KEY: &str = "soundboard.clip_ids";

struct SelectOptionsFetch {
    options: HashMap<String, Vec<(String, String)>>,
    overlay_kind_by_identity: HashMap<String, String>,
    concurrent_queue_ids: HashSet<String>,
    unavailable_clip_ids: HashSet<String>,
}

fn mark_unavailable_clips(
    clips: &mut [(String, String)],
    unavailable: &HashSet<String>,
    suffix: &str,
) {
    for (id, label) in clips.iter_mut() {
        if unavailable.contains(id) {
            *label = format!("{label} - {suffix}");
        }
    }
}

fn engine_voice_options(voices: &[TtsVoice]) -> Vec<(String, String)> {
    let mut options: Vec<(String, String)> = voices
        .iter()
        .map(|voice| {
            let encoded = EngineVoiceRef::new(voice.engine_id.0.clone(), voice.id.0.clone());
            let engine = engine_label(&voice.engine_id.0);
            let label = if voice.locale.is_empty() {
                format!("{engine} - {}", voice.name)
            } else {
                format!("{engine} - {} ({})", voice.name, voice.locale)
            };
            (encoded.to_string(), label)
        })
        .collect();
    options.sort_by(|a, b| a.1.cmp(&b.1));
    options
}

impl ScreenActionsView {
    pub(super) fn fetch_select_options(&self, cx: &mut Context<Self>) {
        let action_repo = Arc::clone(&self.action_repo);
        let queue_repo = Arc::clone(&self.queue_repo);
        let ti_repo = Arc::clone(&self.trigger_instance_repo);
        let script_repo = Arc::clone(&self.script_repo);
        let clip_library = Arc::clone(&self.clip_library);
        let globals_repo = Arc::clone(&self.globals_repo);
        let overlay_repo = Arc::clone(&self.overlay_repo);
        let overlay_schema = Arc::clone(&self.overlay_schema);
        let tts_registry = self.tts_registry.clone();
        let speak = self.speak.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
                map.insert(
                    DONATION_PROVIDER_OPTIONS_KEY.to_owned(),
                    donation_provider_options(),
                );
                let mut overlay_kind_by_identity: HashMap<String, String> = HashMap::new();
                let mut concurrent_queue_ids: HashSet<String> = HashSet::new();
                if let Ok(actions) = action_repo.list().await {
                    map.insert(
                        "action.ids".to_owned(),
                        actions
                            .into_iter()
                            .map(|a| (a.id.to_string(), a.name))
                            .collect(),
                    );
                }
                if let Ok(queues) = queue_repo.list().await {
                    concurrent_queue_ids = queues
                        .iter()
                        .filter(|q| !q.is_serial())
                        .map(|q| q.id.to_string())
                        .collect();
                    map.insert(
                        "queue.ids".to_owned(),
                        queues
                            .into_iter()
                            .map(|q| (q.id.to_string(), q.name))
                            .collect(),
                    );
                }
                if let Ok(instances) = ti_repo.list_all().await {
                    map.insert(
                        "trigger_instance.ids".to_owned(),
                        instances
                            .into_iter()
                            .map(|ti| (ti.id.to_string(), ti.name))
                            .collect(),
                    );
                }
                if let Ok(scripts) = script_repo.list().await {
                    map.insert(
                        "script.names".to_owned(),
                        scripts
                            .into_iter()
                            .map(|s| (s.name.clone(), s.name))
                            .collect(),
                    );
                }
                let mut unavailable_clip_ids: HashSet<String> = HashSet::new();
                if let Ok(clips) = clip_library.list().await {
                    unavailable_clip_ids = clip_library
                        .availability_of(&clips)
                        .await
                        .into_iter()
                        .filter(|(_, availability)| !availability.is_playable())
                        .map(|(id, _)| id.to_string())
                        .collect();
                    map.insert(
                        CLIP_OPTIONS_KEY.to_owned(),
                        clips
                            .into_iter()
                            .map(|c| (c.id.to_string(), c.name))
                            .collect(),
                    );
                }
                if let Ok(globals) = globals_repo.list().await {
                    map.insert(
                        "global.names".to_owned(),
                        globals
                            .into_iter()
                            .map(|g| (g.name.clone(), g.name))
                            .collect(),
                    );
                }
                if let Ok(overlays) = overlay_repo.list().await {
                    map.insert(
                        "overlay.ids".to_owned(),
                        overlays
                            .iter()
                            .filter(|o| overlay_schema.takes_sends(&o.kind_id))
                            .map(|o| (o.id.to_string(), o.display_name.clone()))
                            .collect(),
                    );
                    overlay_kind_by_identity = overlays
                        .into_iter()
                        .map(|o| (o.id.to_string(), o.kind_id))
                        .collect();
                }
                if let Some(registry) = tts_registry {
                    let ids = registry
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .engine_ids();
                    map.insert(
                        "tts.engine_ids".to_owned(),
                        ids.into_iter().map(|id| (id.0.clone(), id.0)).collect(),
                    );
                }
                if let Some(speak) = speak {
                    let voices = speak.available_voices();
                    for voice in voices.iter() {
                        map.entry(format!("tts.voices.{}", voice.engine_id.0))
                            .or_default()
                            .push((voice.id.0.clone(), voice.name.clone()));
                    }
                    map.insert(
                        ENGINE_VOICE_OPTIONS_KEY.to_owned(),
                        engine_voice_options(&voices),
                    );
                }
                SelectOptionsFetch {
                    options: map,
                    overlay_kind_by_identity,
                    concurrent_queue_ids,
                    unavailable_clip_ids,
                }
            },
            |this, fetch, cx| this.on_select_options_fetched(fetch, cx),
            cx,
        );
    }

    fn on_select_options_fetched(&mut self, fetch: SelectOptionsFetch, cx: &mut Context<Self>) {
        let SelectOptionsFetch {
            mut options,
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
        self.overlay_schema = Arc::new(
            self.overlay_schema
                .with_identities(overlay_kind_by_identity),
        );
        self.concurrent_queue_ids = concurrent_queue_ids;
        self.select_options = options;
        self.select_options.extend(self.collection_options.clone());
        if let Some(form) = self.sub_form.clone() {
            let schema = Arc::clone(&self.overlay_schema) as Arc<dyn FormSchemaSource>;
            let merged = self.select_options.clone();
            form.update(cx, |form, cx| {
                form.apply_options(&merged, cx);
                form.set_schema(schema, cx);
            });
        }
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_storage::{
        MEDIA_CONTENT_DIGEST_BYTES, MediaBlobId, MockMediaRepo, StoredClip, clip_source_referrer,
    };
    use forge_types::{ClipId, OutputDevice};
    use time::OffsetDateTime;

    use super::super::tests::view_with;
    use super::*;
    use crate::test_support::{StubActions, pump, runtime};

    const MISSING: &str = "file missing";
    const CLIP_PROBE_ROUNDS: usize = 1_000;

    fn options(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(id, label)| ((*id).to_owned(), (*label).to_owned()))
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
    ) -> Entity<ScreenActionsView> {
        view_with(
            cx,
            rt,
            clips,
            media,
            SubActionRegistry::new(),
            Arc::new(StubActions),
        )
    }

    #[gpui::test]
    fn the_clip_picker_marks_a_clip_whose_file_is_gone(cx: &mut gpui::TestAppContext) {
        crate::i18n::install_language(forge_storage::Language::En);
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
        let view = view_over(cx, &rt, vec![ghost, real], media);

        view.update(cx, |view, cx| view.fetch_select_options(cx));
        let mut listed = None;
        for _ in 0..CLIP_PROBE_ROUNDS {
            pump(&rt);
            cx.run_until_parked();
            listed = view.read_with(cx, |view, _| {
                view.select_options.get(CLIP_OPTIONS_KEY).cloned()
            });
            if listed.is_some() {
                break;
            }
        }

        assert_eq!(listed, Some(expected));
    }
}
