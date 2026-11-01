mod panes;
mod test_text;

use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::FONT_MICRO;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use forge_components::OverflowRevealingFocus;
use forge_components::{
    ActivePresentation, BORDER_THIN, DISABLED_OPACITY, Density, FONT_MD, FONT_XS, FONT_XXS,
    ForgePalette, Icon, InputEvent, KeyActivated, PushToast, Radius, Spacing, TextArea, ToastKind,
    avatar_tile, card, empty_state, error_row_with_retry, hash_accent, icon, radius, section_label,
    slider, spacing, status_dot, toggle, tr,
};
use forge_speak_queue::{SpeakCommand, SpeakQueueHandle};
use forge_speech_types::SynthesisDefaults;
use forge_speech_types::{EngineId, TtsVoice, VoiceId};
use forge_storage::{CredentialsRepo, EngineParams, SettingsRepo, StorageError};
use forge_tts_cloud::CloudGateways;
use forge_tts_core::EngineClass;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::app::async_bridge::{
    self, Debounced, ErrorSink, Generation, Loadable, SLIDER_PERSIST_DEBOUNCE, SnapshotWriter,
    WriteOutcome,
};
use crate::audio::cloud_credentials::{
    CloudCredentialsView, CloudEngineKind, CloudEngineRegistered, stored_region,
};
use crate::audio::tts_engine_roster::{
    RosterEngine, engine_class_label, load_engine_roster, voice_descriptor_label,
};
use crate::audio::voice_test_text::{
    default_voice_test_text, spoken_voice_test_text, voice_preview_request,
};
use crate::shared::repo_failure::RepoFailure;

const RAIL_W: Pixels = px(240.0);
const TILE: Pixels = px(36.0);
const AVATAR: Pixels = px(26.0);
const PARAM_LABEL_W: Pixels = px(60.0);
const PARAM_VALUE_W: Pixels = px(60.0);
const STATUS_DOT: Pixels = px(7.0);
const PLUS_GLYPH: Pixels = px(12.0);
const PITCH_MIN_SEMITONES: f32 = -12.0;
const PITCH_MAX_SEMITONES: f32 = 12.0;
const SPEED_MIN: f32 = 0.5;
const SPEED_MAX: f32 = 2.0;
const GAIN_MIN: f32 = 0.0;
const GAIN_MAX: f32 = 1.0;
const TILE_GLYPH: Pixels = px(18.0);
const PLAY_GLYPH: Pixels = px(14.0);
const GRID_COLS: usize = 3;

struct ParamsSave {
    engine_id: String,
    ticket: u64,
    params: EngineParams,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    None,
    Engine(usize),
    Adding(CloudEngineKind),
}

pub(crate) struct TtsEnginesView {
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    engines: Vec<RosterEngine>,
    selected: Selection,
    disabled: Loadable<HashSet<String>>,
    disabled_writer: SnapshotWriter,
    params: Loadable<EngineParams>,
    params_saves: HashMap<String, Generation>,
    params_writers: HashMap<String, SnapshotWriter>,
    add_open: bool,
    regions: HashMap<String, String>,
    cloud: Entity<CloudCredentialsView>,
    test_text: Entity<TextArea>,
    test_text_load: Loadable<()>,
    test_text_debounce: Debounced,
    _subs: Vec<Subscription>,
}

impl TtsEnginesView {
    pub(crate) fn new(
        cloud_gateways: CloudGateways,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let cloud = cx.new(|cx| {
            CloudCredentialsView::new(
                cloud_gateways,
                Arc::clone(&credentials),
                rt_handle.clone(),
                speak.clone(),
                cx,
            )
        });
        let sub = cx.subscribe(
            &cloud,
            |this, _entity, event: &CloudEngineRegistered, cx| {
                this.on_engine_registered(&event.0, cx);
            },
        );
        let test_text = test_text::new_input(cx);
        let test_text_sub = cx.subscribe(&test_text, |this, _input, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                this.save_test_text(cx);
            }
        });

        let mut view = Self {
            engines: load_engine_roster(speak.as_ref()),
            credentials,
            settings,
            speak,
            disabled_writer: SnapshotWriter::new(&rt_handle),
            rt_handle,
            selected: Selection::None,
            disabled: Loadable::Loading,
            params: Loadable::Loading,
            params_saves: HashMap::new(),
            params_writers: HashMap::new(),
            add_open: false,
            regions: HashMap::new(),
            cloud,
            test_text,
            test_text_load: Loadable::Loading,
            test_text_debounce: Debounced::new(SLIDER_PERSIST_DEBOUNCE),
            _subs: vec![sub, test_text_sub],
        };
        view.load_disabled(cx);
        view.load_test_text(cx);
        view
    }

    fn load_disabled(&mut self, cx: &mut Context<Self>) {
        self.disabled = Loadable::Loading;
        let settings = Arc::clone(&self.settings);
        async_bridge::run_async(
            &self.rt_handle,
            async move { forge_storage::disabled_tts_engines(settings.as_ref()).await },
            |this, result: Result<Vec<String>, StorageError>, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%error, "failed to load the disabled TTS engines");
                }
                this.disabled = Loadable::settled(result.map(|ids| ids.into_iter().collect()));
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    fn rollback_disabled(&mut self, cx: &mut Context<Self>) {
        self.disabled = Loadable::Loading;
        let settings = Arc::clone(&self.settings);
        async_bridge::run_async(
            &self.rt_handle,
            async move { forge_storage::disabled_tts_engines(settings.as_ref()).await },
            |this, result: Result<Vec<String>, StorageError>, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%error, "failed to reload the disabled TTS engines");
                }
                this.disabled = Loadable::settled(result.map(|ids| ids.into_iter().collect()));
                this.resync_live_engines(cx);
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    fn resync_live_engines(&self, cx: &mut Context<Self>) {
        let (Some(disabled), Some(speak)) = (self.disabled.loaded(), self.speak.clone()) else {
            return;
        };
        let commands: Vec<SpeakCommand> = self
            .engines
            .iter()
            .map(|engine| {
                SpeakCommand::SetEngineEnabled(
                    EngineId(engine.id.clone()),
                    !disabled.contains(&engine.id),
                )
            })
            .collect();
        async_bridge::report_failure(
            &self.rt_handle,
            async move {
                for command in commands {
                    speak.send(command).await?;
                }
                Ok::<(), forge_speak_queue::SpeakError>(())
            },
            ErrorSink::Toast,
            tr!("tts_engines_toggle_failed"),
            cx,
        );
    }

    fn load_params(&mut self, engine_id: String, cx: &mut Context<Self>) {
        self.params = Loadable::Loading;
        let settings = Arc::clone(&self.settings);
        let requested = engine_id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { forge_storage::engine_params(settings.as_ref(), &requested).await },
            move |this, result: Result<EngineParams, StorageError>, cx| {
                if this.selected_engine_id().as_deref() != Some(engine_id.as_str()) {
                    return;
                }
                if let Err(error) = &result {
                    tracing::warn!(%error, engine = %engine_id, "failed to load TTS engine params");
                }
                this.params = Loadable::settled(result);
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    fn retry_params(&mut self, cx: &mut Context<Self>) {
        if let Some(engine_id) = self.selected_engine_id() {
            self.load_params(engine_id, cx);
        }
    }

    fn catalog(&self) -> Arc<Vec<TtsVoice>> {
        self.speak
            .as_ref()
            .map(|h| h.available_voices())
            .unwrap_or_default()
    }

    fn select_engine(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = Selection::Engine(index);
        self.add_open = false;
        let kind = self
            .engines
            .get(index)
            .and_then(|e| CloudEngineKind::from_engine_id(&e.id));
        self.cloud
            .update(cx, |cloud, cx| cloud.set_active(kind, cx));
        if let Some(engine) = self.engines.get(index) {
            let id = engine.id.clone();
            self.ensure_region_loaded(&id, cx);
            self.load_params(id, cx);
        }
        cx.notify();
    }

    fn selected_engine_id(&self) -> Option<String> {
        match self.selected {
            Selection::Engine(index) => self.engines.get(index).map(|e| e.id.clone()),
            _ => None,
        }
    }

    fn toggle_add_picker(&mut self, cx: &mut Context<Self>) {
        self.add_open = !self.add_open;
        cx.notify();
    }

    fn choose_add(&mut self, kind: CloudEngineKind, cx: &mut Context<Self>) {
        self.selected = Selection::Adding(kind);
        self.add_open = false;
        self.cloud
            .update(cx, |cloud, cx| cloud.set_active(Some(kind), cx));
        cx.notify();
    }

    fn toggle_engine(&mut self, engine_id: String, cx: &mut Context<Self>) {
        let Some(disabled) = self.disabled.loaded_mut() else {
            return;
        };
        if !disabled.remove(&engine_id) {
            disabled.insert(engine_id.clone());
        }
        let now_enabled = !disabled.contains(&engine_id);
        let ids: Vec<String> = disabled.iter().cloned().collect();
        if let Some(speak) = self.speak.clone() {
            let eid = EngineId(engine_id);
            async_bridge::report_failure(
                &self.rt_handle,
                async move {
                    speak
                        .send(SpeakCommand::SetEngineEnabled(eid, now_enabled))
                        .await
                },
                ErrorSink::Toast,
                tr!("tts_engines_toggle_failed"),
                cx,
            );
        }
        let settings = Arc::clone(&self.settings);
        self.disabled_writer.write(
            async move { forge_storage::set_disabled_tts_engines(settings.as_ref(), &ids).await },
            Self::disabled_write_settled,
            cx,
        );
        cx.notify();
    }

    fn disabled_write_settled(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        if let WriteOutcome::Failed(error) = outcome {
            cx.push_toast(
                ToastKind::Error,
                tr!(
                    "tts_engines_persist_disabled_failed_reason",
                    reason = error.text()
                ),
            );
            self.rollback_disabled(cx);
        }
    }

    fn edit_params(&mut self, edit: impl FnOnce(&mut EngineParams), cx: &mut Context<Self>) {
        let Some(engine_id) = self.selected_engine_id() else {
            return;
        };
        let Some(params) = self.params.loaded_mut() else {
            return;
        };
        edit(params);
        let params = *params;
        let ticket = self
            .params_saves
            .entry(engine_id.clone())
            .or_default()
            .next();
        let pending = ParamsSave {
            engine_id,
            ticket,
            params,
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move { tokio::time::sleep(SLIDER_PERSIST_DEBOUNCE).await },
            move |this, (), cx| this.save_params(pending, cx),
            cx,
        );
        cx.notify();
    }

    fn set_pitch(&mut self, value: f32, cx: &mut Context<Self>) {
        self.edit_params(
            |p| p.pitch_semitones = value.clamp(PITCH_MIN_SEMITONES, PITCH_MAX_SEMITONES),
            cx,
        );
    }

    fn set_speed(&mut self, value: f32, cx: &mut Context<Self>) {
        self.edit_params(
            |p| p.rate_multiplier = value.clamp(SPEED_MIN, SPEED_MAX),
            cx,
        );
    }

    fn set_engine_volume(&mut self, value: f32, cx: &mut Context<Self>) {
        self.edit_params(|p| p.gain = value.clamp(GAIN_MIN, GAIN_MAX), cx);
    }

    fn save_params(&mut self, pending: ParamsSave, cx: &mut Context<Self>) {
        let ParamsSave {
            engine_id,
            ticket,
            params,
        } = pending;
        if !self
            .params_saves
            .get(&engine_id)
            .is_some_and(|saves| saves.is_current(ticket))
        {
            return;
        }
        let speak = self.speak.clone();
        let settings = Arc::clone(&self.settings);
        let eid = EngineId(engine_id.clone());
        let handle = self.rt_handle.clone();
        self.params_writers
            .entry(engine_id.clone())
            .or_insert_with(|| SnapshotWriter::new(&handle))
            .write(
                async move {
                    forge_storage::set_engine_params(settings.as_ref(), &eid.0, params)
                        .await
                        .map_err(RepoFailure::from)?;
                    if let Some(speak) = speak {
                        let defaults = SynthesisDefaults {
                            pitch_semitones: params.pitch_semitones,
                            rate_multiplier: params.rate_multiplier,
                        };
                        speak
                            .send(SpeakCommand::SetEngineParams(eid, defaults, params.gain))
                            .await
                            .map_err(RepoFailure::from)?;
                    }
                    Ok::<(), RepoFailure>(())
                },
                move |this, outcome, cx| this.params_write_settled(&engine_id, outcome, cx),
                cx,
            );
    }

    fn params_write_settled(
        &mut self,
        engine_id: &str,
        outcome: WriteOutcome,
        cx: &mut Context<Self>,
    ) {
        if let WriteOutcome::Failed(error) = outcome {
            cx.push_toast(
                ToastKind::Error,
                tr!("tts_engines_persist_params_failed", reason = error.text()),
            );
            if self.selected_engine_id().as_deref() == Some(engine_id) {
                self.load_params(engine_id.to_owned(), cx);
            }
        }
    }

    pub(crate) fn refresh_engine_roster(&mut self, cx: &mut Context<Self>) {
        let selected_engine = self.selected_engine_id();
        self.engines = load_engine_roster(self.speak.as_ref());
        if let Selection::Engine(_) = self.selected {
            self.selected = selected_engine
                .and_then(|id| self.engines.iter().position(|engine| engine.id == id))
                .map_or(Selection::None, Selection::Engine);
        }
        cx.notify();
    }

    fn on_engine_registered(&mut self, engine_id: &EngineId, cx: &mut Context<Self>) {
        self.engines = load_engine_roster(self.speak.as_ref());
        if let Some(index) = self.engines.iter().position(|e| e.id == engine_id.0) {
            self.selected = Selection::Engine(index);
            self.load_params(engine_id.0.clone(), cx);
        }
        self.regions.remove(&engine_id.0);
        self.ensure_region_loaded(&engine_id.0.clone(), cx);
        cx.notify();
    }

    fn ensure_region_loaded(&mut self, engine_id: &str, cx: &mut Context<Self>) {
        if self.regions.contains_key(engine_id) {
            return;
        }
        let Some(kind) = CloudEngineKind::from_engine_id(engine_id) else {
            return;
        };
        let repo = Arc::clone(&self.credentials);
        let id = engine_id.to_owned();
        async_bridge::run_async(
            &self.rt_handle,
            async move { stored_region(repo.as_ref(), kind).await },
            move |this, region: Result<Option<String>, RepoFailure>, cx| match region {
                Ok(Some(region)) => {
                    this.regions.insert(id, region);
                    cx.notify();
                }
                Ok(None) => {}
                Err(failure) => {
                    tracing::warn!(engine = %id, error = %failure.text(), "could not read the cloud engine region");
                }
            },
            cx,
        );
    }

    fn preview_voice(&self, engine_id: String, voice_id: String, cx: &mut Context<Self>) {
        let Some(speak) = self.speak.clone() else {
            tracing::warn!("voice preview dropped - speak queue unavailable");
            return;
        };
        let text = spoken_voice_test_text(
            self.test_text.read(cx).content(),
            &default_voice_test_text(),
        );
        let voice = (EngineId(engine_id), VoiceId(voice_id));
        let request = voice_preview_request(text, String::new(), String::new(), Some(voice));
        async_bridge::report_failure(
            &self.rt_handle,
            async move { speak.send(SpeakCommand::Preview(request)).await },
            ErrorSink::Toast,
            tr!("tts_aliases_preview_failed"),
            cx,
        );
    }

    fn configured_cloud_kinds(&self) -> HashSet<&'static str> {
        self.engines
            .iter()
            .filter_map(|e| CloudEngineKind::from_engine_id(&e.id).map(|k| k.key()))
            .collect()
    }
}

impl Render for TtsEnginesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let catalog = self.catalog();

        let list = self.engine_list(&catalog, &palette, density, cx);
        let detail = self.detail_pane(&catalog, &palette, density, cx);

        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(palette.base)
            .child(list)
            .child(detail)
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "render helper takes each displayed value and handler separately"
)]
fn param_row(
    label: impl Into<SharedString>,
    value_text: impl Into<SharedString>,
    value: f32,
    min: f32,
    max: f32,
    id: &'static str,
    on_change: impl Fn(&f32, &mut Window, &mut gpui::App) + 'static,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Md, density))
        .py(spacing(Spacing::Xs, density))
        .child(
            div()
                .w(PARAM_LABEL_W)
                .flex_shrink_0()
                .font_family(cx.body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(slider(value, min, max, palette).on_change(id, on_change)),
        )
        .child(
            div()
                .w(PARAM_VALUE_W)
                .flex_shrink_0()
                .text_right()
                .font_family(cx.mono_family())
                .text_size(FONT_LABEL)
                .text_color(palette.text_primary)
                .child(value_text.into()),
        )
}

fn engine_status_color(class: EngineClass, palette: &ForgePalette) -> Rgba {
    match class {
        EngineClass::System => palette.info,
        EngineClass::Local | EngineClass::Cloud => palette.success,
    }
}

fn engine_glyph(class: EngineClass) -> Icon {
    match class {
        EngineClass::Cloud => Icon::Cloud,
        EngineClass::Local | EngineClass::System => Icon::Cpu,
    }
}

fn engine_voice_count(catalog: &[TtsVoice], engine_id: &str) -> usize {
    catalog
        .iter()
        .filter(|v| v.engine_id.0 == engine_id)
        .count()
}

fn voice_initial(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use forge_types::Secret;
    use std::cell::RefCell;

    use forge_components::Toasts;
    use forge_storage::Language;
    use forge_storage::settings::core_settings::{
        TTS_DISABLED_ENGINES, TTS_ENGINE_PARAMS, TTS_VOICE_TEST_TEXT,
    };
    use forge_tts_core::{
        ENGINE_ID_ESPEAK_NG, ENGINE_ID_PIPER, TtsEngine, TtsEngineFactory, TtsError, TtsRegistry,
    };
    use gpui::{Entity, TestAppContext};
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::*;
    use crate::app::i18n::install_language;
    use crate::audio::voice_test_text::VOICE_TEST_TEXT_MAX_CHARS;
    use crate::test_support::{
        ReadFault, SETTINGS_READ_FAILED, SettingWrite, TestBackend, error_toasts,
        install_presentation, runtime, spawn_speak_queue, spawn_speak_queue_with_registry,
        test_backend, writes_to,
    };

    const ESPEAK: usize = 0;
    const PIPER: usize = 1;
    const STORED_PIPER: EngineParams = EngineParams {
        pitch_semitones: 4.0,
        rate_multiplier: 1.5,
        gain: 0.5,
    };
    const STORED_ESPEAK: EngineParams = EngineParams {
        pitch_semitones: -6.0,
        rate_multiplier: 0.75,
        gain: 0.25,
    };

    struct NeverCreated(&'static str);

    impl TtsEngineFactory for NeverCreated {
        fn engine_id(&self) -> EngineId {
            EngineId(self.0.to_owned())
        }

        fn class(&self) -> forge_tts_core::EngineClass {
            forge_tts_core::EngineClass::Local
        }

        fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
            Err(TtsError::AuthFailed {
                reason: "the engines pane never builds an engine".to_owned(),
            })
        }
    }

    fn engines_registry() -> TtsRegistry {
        let mut registry = TtsRegistry::new();
        for id in [ENGINE_ID_PIPER, ENGINE_ID_ESPEAK_NG] {
            registry.register(Arc::new(NeverCreated(id)));
        }
        registry
    }

    fn params_key(engine_id: &str) -> String {
        TTS_ENGINE_PARAMS.member_key(engine_id).unwrap()
    }

    fn undecodable(key: &str) -> String {
        crate::test_support::shown_in_english(StorageError::UndecodableSetting {
            key: key.to_owned(),
        })
    }

    fn unreachable() -> String {
        crate::test_support::shown_in_english(StorageError::Connection {
            reason: SETTINGS_READ_FAILED.to_owned(),
        })
    }

    struct EnginesRig {
        rt: tokio::runtime::Runtime,
        backend: Arc<TestBackend>,
        writes: RefCell<UnboundedReceiver<SettingWrite>>,
    }

    impl EnginesRig {
        fn new(cx: &mut TestAppContext) -> Self {
            install_language(Language::En);
            install_presentation(cx);
            cx.update(|cx| cx.set_global(Toasts::new()));
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .start_paused(true)
                .build()
                .unwrap();
            let (backend, writes) = test_backend();
            backend.seed_raw_family(
                &TTS_ENGINE_PARAMS,
                ENGINE_ID_PIPER,
                &serde_json::to_string(&STORED_PIPER).unwrap(),
            );
            backend.seed_raw_family(
                &TTS_ENGINE_PARAMS,
                ENGINE_ID_ESPEAK_NG,
                &serde_json::to_string(&STORED_ESPEAK).unwrap(),
            );
            Self {
                rt,
                backend,
                writes: RefCell::new(writes),
            }
        }

        fn open(
            &self,
            speak: Option<SpeakQueueHandle>,
            cx: &mut TestAppContext,
        ) -> Entity<TtsEnginesView> {
            let speak = speak.or_else(|| {
                let _runtime = self.rt.enter();
                Some(spawn_speak_queue_with_registry(engines_registry()))
            });
            let credentials = self.backend.credentials_repo();
            let settings = self.backend.settings_repo();
            let handle = self.rt.handle().clone();
            let view = cx.update(|cx| {
                cx.new(|cx| {
                    TtsEnginesView::new(
                        CloudGateways::new(),
                        credentials,
                        settings,
                        speak,
                        handle,
                        cx,
                    )
                })
            });
            self.settle(cx);
            view
        }

        fn settle(&self, cx: &mut TestAppContext) {
            crate::test_support::settle_paused_runtime_work(cx, &self.rt);
        }

        fn past_the_save_debounce(&self, cx: &mut TestAppContext) {
            self.settle(cx);
            self.rt
                .block_on(tokio::time::advance(SLIDER_PERSIST_DEBOUNCE * 2));
            self.settle(cx);
        }

        fn writes_to(&self, key: &str) -> Vec<String> {
            writes_to(&mut self.writes.borrow_mut(), key)
        }

        fn written_params(&self, engine_id: &str) -> Vec<EngineParams> {
            self.writes_to(&params_key(engine_id))
                .iter()
                .map(|raw| serde_json::from_str(raw).unwrap())
                .collect()
        }

        fn written_disabled_and_params(&self, engine_id: &str) -> (Vec<String>, Vec<EngineParams>) {
            let key = params_key(engine_id);
            let mut disabled = Vec::new();
            let mut params = Vec::new();
            while let Ok((written, value)) = self.writes.borrow_mut().try_recv() {
                if written == TTS_DISABLED_ENGINES.name() {
                    disabled.push(value);
                } else if written == key {
                    params.push(serde_json::from_str(&value).unwrap());
                }
            }
            (disabled, params)
        }
    }

    type Edit = fn(&mut TtsEnginesView, &mut Context<TtsEnginesView>);

    fn slider_edits() -> [(&'static str, Edit); 3] {
        [
            ("drag pitch", |v, cx| v.set_pitch(3.0, cx)),
            ("drag speed", |v, cx| v.set_speed(1.25, cx)),
            ("drag volume", |v, cx| v.set_engine_volume(0.75, cx)),
        ]
    }

    fn select(
        view: &Entity<TtsEnginesView>,
        index: usize,
        rig: &EnginesRig,
        cx: &mut TestAppContext,
    ) {
        view.update(cx, |v, cx| v.select_engine(index, cx));
        rig.settle(cx);
    }

    fn shown_params(
        view: &Entity<TtsEnginesView>,
        cx: &mut TestAppContext,
    ) -> Option<EngineParams> {
        view.read_with(cx, |v, _| v.params.loaded().copied())
    }

    fn shown_disabled(
        view: &Entity<TtsEnginesView>,
        cx: &mut TestAppContext,
    ) -> Option<Vec<String>> {
        view.read_with(cx, |v, _| {
            v.disabled.loaded().map(|set| {
                let mut ids: Vec<String> = set.iter().cloned().collect();
                ids.sort();
                ids
            })
        })
    }

    enum Fault {
        Unreachable,
        Undecodable,
        Pending,
    }

    enum FaultTarget {
        DisabledEngines,
        Params(&'static str),
    }

    fn apply_fault(rig: &EnginesRig, target: &FaultTarget, fault: &Fault) -> Option<String> {
        let key = &match target {
            FaultTarget::DisabledEngines => TTS_DISABLED_ENGINES.name().to_owned(),
            FaultTarget::Params(engine_id) => params_key(engine_id),
        };
        match fault {
            Fault::Unreachable => {
                rig.backend.fault_reads(key, ReadFault::Fails);
                Some(unreachable())
            }
            Fault::Undecodable => {
                match target {
                    FaultTarget::DisabledEngines => {
                        rig.backend.seed_raw(&TTS_DISABLED_ENGINES, "not json")
                    }
                    FaultTarget::Params(engine_id) => {
                        rig.backend
                            .seed_raw_family(&TTS_ENGINE_PARAMS, engine_id, "not json")
                    }
                }
                Some(undecodable(key))
            }
            Fault::Pending => {
                rig.backend
                    .fault_reads(key, ReadFault::Held(Arc::new(tokio::sync::Notify::new())));
                None
            }
        }
    }

    #[test]
    fn the_roster_shows_each_shipped_engine_under_the_class_its_factory_declares() {
        let gateways = forge_tts_cloud::CloudGateways::new();
        let factories: [Arc<dyn TtsEngineFactory>; 8] = [
            Arc::new(forge_tts_piper::PiperEngineFactory {
                piper_binary: "piper".into(),
                voices_dir: "voices".into(),
                timeout: std::time::Duration::from_secs(1),
            }),
            Arc::new(forge_tts_espeak::EspeakEngineFactory::default()),
            Arc::new(forge_tts_sapi::SapiEngineFactory),
            Arc::new(forge_tts_nsspeech::NsSpeechEngineFactory),
            Arc::new(forge_tts_cloud::azure::AzureEngineFactory::new(
                forge_tts_cloud::credentials::AzureCredentials {
                    api_key: Secret::new("key"),
                    region: "westeurope".into(),
                    base_url: None,
                },
                &gateways,
            )),
            Arc::new(forge_tts_cloud::elevenlabs::ElevenLabsEngineFactory::new(
                forge_tts_cloud::credentials::ElevenLabsCredentials {
                    api_key: Secret::new("key"),
                    base_url: None,
                },
                &gateways,
            )),
            Arc::new(forge_tts_cloud::openai::OpenAiEngineFactory::new(
                forge_tts_cloud::credentials::OpenAiCredentials {
                    api_key: Secret::new("key"),
                    base_url: None,
                },
                &gateways,
            )),
            Arc::new(forge_tts_cloud::polly::PollyEngineFactory::new(
                forge_tts_cloud::credentials::PollyCredentials {
                    access_key_id: Secret::new("id"),
                    secret_access_key: Secret::new("secret"),
                    region: "eu-west-1".into(),
                    base_url: None,
                },
                &gateways,
            )),
        ];
        let mut registry = TtsRegistry::new();
        for factory in factories {
            registry.register(factory);
        }
        let rt = runtime();
        let speak = {
            let _runtime = rt.enter();
            spawn_speak_queue_with_registry(registry)
        };

        let roster: Vec<(String, String, EngineClass)> = load_engine_roster(Some(&speak))
            .into_iter()
            .map(|engine| (engine.id, engine.name, engine.class))
            .collect();

        let expected = [
            ("azure", "Azure Speech", EngineClass::Cloud),
            ("elevenlabs", "ElevenLabs", EngineClass::Cloud),
            ("espeak-ng", "eSpeak-NG", EngineClass::Local),
            ("nsspeech", "Apple AVSpeech", EngineClass::System),
            ("openai", "OpenAI TTS", EngineClass::Cloud),
            ("piper", "Piper", EngineClass::Local),
            ("polly", "Amazon Polly", EngineClass::Cloud),
            ("sapi", "Microsoft SAPI 5", EngineClass::System),
        ]
        .map(|(id, name, class)| (id.to_owned(), name.to_owned(), class));
        assert_eq!(roster, expected);
    }

    #[gpui::test]
    fn no_engine_toggle_writes_until_the_disabled_engines_load(cx: &mut TestAppContext) {
        for fault in [Fault::Unreachable, Fault::Undecodable, Fault::Pending] {
            let rig = EnginesRig::new(cx);
            let reason = apply_fault(&rig, &FaultTarget::DisabledEngines, &fault);
            let view = rig.open(None, cx);

            view.update(cx, |v, cx| v.toggle_engine(ENGINE_ID_PIPER.to_owned(), cx));
            rig.settle(cx);

            let failure = view.read_with(cx, |v, _| v.disabled.failure());
            assert_eq!(
                (rig.writes_to(TTS_DISABLED_ENGINES.name()), failure),
                (Vec::<String>::new(), reason.clone()),
                "{reason:?}"
            );
        }
    }

    #[gpui::test]
    fn no_slider_drag_writes_until_the_selected_engine_params_load(cx: &mut TestAppContext) {
        let target = FaultTarget::Params(ENGINE_ID_PIPER);
        for fault in [Fault::Unreachable, Fault::Undecodable, Fault::Pending] {
            for (case, edit) in slider_edits() {
                let rig = EnginesRig::new(cx);
                let _runtime = rig.rt.enter();
                let reason = apply_fault(&rig, &target, &fault);
                let view = rig.open(None, cx);
                select(&view, PIPER, &rig, cx);

                view.update(cx, edit);
                rig.past_the_save_debounce(cx);

                let failure = view.read_with(cx, |v, _| v.params.failure());
                assert_eq!(
                    (rig.written_params(ENGINE_ID_PIPER), failure),
                    (Vec::new(), reason.clone()),
                    "{reason:?}: {case}"
                );
            }
        }
    }

    #[gpui::test]
    fn retrying_failed_loads_lets_a_toggle_and_a_slider_drag_persist(cx: &mut TestAppContext) {
        let rig = EnginesRig::new(cx);
        let _runtime = rig.rt.enter();
        let key = params_key(ENGINE_ID_PIPER);
        rig.backend
            .fault_reads(TTS_DISABLED_ENGINES.name(), ReadFault::Fails);
        rig.backend.fault_reads(&key, ReadFault::Fails);
        let view = rig.open(None, cx);
        select(&view, PIPER, &rig, cx);
        rig.backend.heal_reads(TTS_DISABLED_ENGINES.name());
        rig.backend.heal_reads(&key);

        view.update(cx, |v, cx| {
            v.load_disabled(cx);
            v.retry_params(cx);
        });
        rig.settle(cx);
        view.update(cx, |v, cx| {
            v.toggle_engine(ENGINE_ID_PIPER.to_owned(), cx);
            v.set_pitch(-2.0, cx);
        });
        rig.past_the_save_debounce(cx);

        assert_eq!(
            rig.written_disabled_and_params(ENGINE_ID_PIPER),
            (
                vec![serde_json::to_string(&[ENGINE_ID_PIPER]).unwrap()],
                vec![EngineParams {
                    pitch_semitones: -2.0,
                    ..STORED_PIPER
                }],
            )
        );
    }

    #[gpui::test]
    fn a_params_load_landing_after_another_engine_was_selected_is_dropped(cx: &mut TestAppContext) {
        let rig = EnginesRig::new(cx);
        let gate = Arc::new(tokio::sync::Notify::new());
        let key = params_key(ENGINE_ID_PIPER);
        rig.backend
            .fault_reads(&key, ReadFault::Held(Arc::clone(&gate)));
        let view = rig.open(None, cx);

        select(&view, PIPER, &rig, cx);
        select(&view, ESPEAK, &rig, cx);
        rig.backend.heal_reads(&key);
        gate.notify_one();
        rig.settle(cx);

        assert_eq!(shown_params(&view, cx), Some(STORED_ESPEAK));
    }

    #[gpui::test]
    fn a_failed_params_write_toasts_and_reloads_the_stored_params(cx: &mut TestAppContext) {
        for (case, edit) in slider_edits() {
            let rig = EnginesRig::new(cx);
            let _runtime = rig.rt.enter();
            rig.backend.fail_writes(&params_key(ENGINE_ID_PIPER));
            let view = rig.open(None, cx);
            select(&view, PIPER, &rig, cx);

            view.update(cx, edit);
            rig.past_the_save_debounce(cx);

            assert_eq!(
                (cx.update(|cx| error_toasts(cx)), shown_params(&view, cx)),
                (1, Some(STORED_PIPER)),
                "{case}"
            );
        }
    }

    fn test_text(view: &Entity<TtsEnginesView>, cx: &mut TestAppContext) -> String {
        view.read_with(cx, |v, cx| v.test_text.read(cx).content().to_owned())
    }

    fn stored_test_text(rig: &EnginesRig) -> Option<String> {
        let settings = rig.backend.settings_repo();
        rig.rt
            .block_on(forge_storage::voice_test_text(settings.as_ref()))
            .unwrap()
    }

    fn type_test_text(view: &Entity<TtsEnginesView>, text: &str, cx: &mut TestAppContext) {
        view.update(cx, |v, cx| {
            v.test_text.update(cx, |input, cx| {
                input.clear(cx);
                input.insert_at_caret(text, cx);
            });
        });
    }

    #[gpui::test]
    fn the_test_text_shows_the_default_until_one_is_stored(cx: &mut TestAppContext) {
        let rig = EnginesRig::new(cx);
        assert_eq!(
            test_text(&rig.open(None, cx), cx),
            default_voice_test_text()
        );

        rig.backend
            .seed_raw(&TTS_VOICE_TEST_TEXT, "Стара збережена фраза.");

        assert_eq!(test_text(&rig.open(None, cx), cx), "Стара збережена фраза.");
    }

    #[gpui::test]
    fn an_edit_is_stored_once_after_the_debounce_and_a_cleared_box_forgets_it(
        cx: &mut TestAppContext,
    ) {
        let rig = EnginesRig::new(cx);
        let _runtime = rig.rt.enter();
        let view = rig.open(None, cx);

        type_test_text(&view, "Пер", cx);
        type_test_text(&view, "Перша фраза.", cx);
        rig.past_the_save_debounce(cx);

        assert_eq!(
            rig.writes_to(TTS_VOICE_TEST_TEXT.name()),
            vec!["Перша фраза."]
        );
        assert_eq!(stored_test_text(&rig).as_deref(), Some("Перша фраза."));

        type_test_text(&view, "   ", cx);
        rig.past_the_save_debounce(cx);

        assert_eq!(stored_test_text(&rig), None);
    }

    #[gpui::test]
    fn a_failed_test_text_write_toasts_and_keeps_the_typed_text(cx: &mut TestAppContext) {
        let rig = EnginesRig::new(cx);
        let _runtime = rig.rt.enter();
        rig.backend.fail_writes(TTS_VOICE_TEST_TEXT.name());
        let view = rig.open(None, cx);

        type_test_text(&view, "Не збережеться.", cx);
        rig.past_the_save_debounce(cx);

        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
        assert_eq!(test_text(&view, cx), "Не збережеться.");
    }

    #[gpui::test]
    fn no_test_text_edit_writes_until_the_stored_text_loads(cx: &mut TestAppContext) {
        let rig = EnginesRig::new(cx);
        let _runtime = rig.rt.enter();
        rig.backend
            .fault_reads(TTS_VOICE_TEST_TEXT.name(), ReadFault::Fails);
        let view = rig.open(None, cx);

        type_test_text(&view, "Затре збережене.", cx);
        rig.past_the_save_debounce(cx);

        let failure = view.read_with(cx, |v, _| v.test_text_load.failure());
        assert_eq!(
            (rig.writes_to(TTS_VOICE_TEST_TEXT.name()), failure),
            (Vec::<String>::new(), Some(unreachable()))
        );
    }

    #[gpui::test]
    fn a_voice_preview_sends_the_box_text_bounded_by_the_limit(cx: &mut TestAppContext) {
        let rig = EnginesRig::new(cx);
        let _runtime = rig.rt.enter();
        let (speak, mut events, _aliases) = spawn_speak_queue(Vec::new());
        let view = rig.open(Some(speak), cx);
        let at_limit = "є".repeat(VOICE_TEST_TEXT_MAX_CHARS);

        type_test_text(&view, &format!("{at_limit}є"), cx);
        view.update(cx, |v, cx| {
            v.preview_voice(ENGINE_ID_PIPER.to_owned(), "any-voice".to_owned(), cx);
        });
        rig.settle(cx);

        let enqueued = rig.rt.block_on(async {
            loop {
                if let forge_speak_queue::SpeakEvent::Enqueued { text, .. } =
                    events.recv().await.unwrap()
                {
                    return text;
                }
            }
        });
        assert_eq!(enqueued, at_limit);
    }

    #[gpui::test]
    fn a_failed_disabled_write_toasts_reloads_and_resyncs_the_live_queue(cx: &mut TestAppContext) {
        let rig = EnginesRig::new(cx);
        let speak = {
            let _runtime = rig.rt.enter();
            spawn_speak_queue_with_registry(engines_registry())
        };
        rig.backend.fail_writes(TTS_DISABLED_ENGINES.name());
        let view = rig.open(Some(speak.clone()), cx);

        view.update(cx, |v, cx| v.toggle_engine(ENGINE_ID_PIPER.to_owned(), cx));
        rig.settle(cx);
        rig.settle(cx);

        assert_eq!(
            (
                cx.update(|cx| error_toasts(cx)),
                shown_disabled(&view, cx),
                speak.disabled_engines().is_empty(),
            ),
            (1, Some(Vec::new()), true)
        );
    }
}
