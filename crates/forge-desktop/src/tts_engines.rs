use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::FONT_MICRO;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use forge_components::{
    BORDER_THIN, DISABLED_OPACITY, Density, FONT_MD, FONT_XS, FONT_XXS, ForgePalette, Icon,
    PushToast, Radius, Spacing, ToastKind, avatar_tile, body_family, card, empty_state,
    error_row_with_retry, hash_accent, icon, mono_family, radius, section_label, slider, spacing,
    status_dot, toggle, tr,
};
use forge_speak_queue::{
    EmoteTokenSet, Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest,
};
use forge_speech_types::SynthesisDefaults;
use forge_speech_types::{EngineId, TtsVoice, VoiceGender, VoiceId};
use forge_storage::{CredentialId, CredentialsRepo, EngineParams, SettingsRepo, StorageError};
use forge_tts_cloud::CloudGateways;
use forge_tts_core::{TtsRegistry, engine_display_name};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::async_bridge::{
    self, ErrorSink, Generation, Loadable, SLIDER_PERSIST_DEBOUNCE, SnapshotWriter, WriteOutcome,
};
use crate::cloud_credentials::{CloudCredentialsView, CloudEngineKind, CloudEngineRegistered};
use forge_components::ActivePresentation;

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

struct EngineEntry {
    id: String,
    name: String,
    kind: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    None,
    Engine(usize),
    Adding(CloudEngineKind),
}

pub struct TtsEnginesView {
    registry: Option<Arc<RwLock<TtsRegistry>>>,
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    engines: Vec<EngineEntry>,
    selected: Selection,
    disabled: Loadable<HashSet<String>>,
    disabled_writer: SnapshotWriter,
    params: Loadable<EngineParams>,
    params_saves: HashMap<String, Generation>,
    params_writers: HashMap<String, SnapshotWriter>,
    add_open: bool,
    regions: HashMap<String, String>,
    cloud: Entity<CloudCredentialsView>,
    _subs: Vec<Subscription>,
}

impl TtsEnginesView {
    pub fn new(
        registry: Option<Arc<RwLock<TtsRegistry>>>,
        cloud_gateways: CloudGateways,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let cloud = cx.new(|cx| {
            CloudCredentialsView::new(
                registry.clone(),
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

        let mut view = Self {
            engines: load_roster(registry.as_ref()),
            registry,
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
            _subs: vec![sub],
        };
        view.load_disabled(cx);
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
                    reason = error.as_str()
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
                        .map_err(|e| e.to_string())?;
                    if let Some(speak) = speak {
                        let defaults = SynthesisDefaults {
                            pitch_semitones: params.pitch_semitones,
                            rate_multiplier: params.rate_multiplier,
                        };
                        speak
                            .send(SpeakCommand::SetEngineParams(eid, defaults, params.gain))
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                    Ok::<(), String>(())
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
                tr!("tts_engines_persist_params_failed", reason = error.as_str()),
            );
            if self.selected_engine_id().as_deref() == Some(engine_id) {
                self.load_params(engine_id.to_owned(), cx);
            }
        }
    }

    fn on_engine_registered(&mut self, engine_id: &EngineId, cx: &mut Context<Self>) {
        self.engines = load_roster(self.registry.as_ref());
        if let Some(index) = self.engines.iter().position(|e| e.id == engine_id.0) {
            self.selected = Selection::Engine(index);
            self.load_params(engine_id.0.clone(), cx);
        }
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
        let cred_id = kind.credential_id();
        let repo = Arc::clone(&self.credentials);
        let id = engine_id.to_owned();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                match repo.load(&CredentialId::new(cred_id)).await {
                    Ok(Some(json)) => serde_json::from_str::<serde_json::Value>(&json)
                        .ok()
                        .and_then(|v| {
                            v.get("region")
                                .and_then(|r| r.as_str())
                                .map(|s| s.to_owned())
                        }),
                    _ => None,
                }
            },
            move |this, region: Option<String>, cx| {
                if let Some(region) = region {
                    this.regions.insert(id, region);
                    cx.notify();
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
        let request = SpeakRequest {
            request_id: RequestId::new(),
            viewer_id: String::new(),
            viewer_name: String::new(),
            text: tr!("tts_engines_voice_preview_sample"),
            priority: Priority::Normal,
            engine_override: Some(EngineId(engine_id)),
            voice_override: Some(VoiceId(voice_id)),
            source_event_id: None,
            is_reward: false,
            target: None,
            message_emotes: EmoteTokenSet::default(),
        };
        async_bridge::report_failure(
            &self.rt_handle,
            async move { speak.send(SpeakCommand::Enqueue(request)).await },
            ErrorSink::Silent,
            "voice preview enqueue",
            cx,
        );
    }

    fn configured_cloud_kinds(&self) -> HashSet<&'static str> {
        self.engines
            .iter()
            .filter_map(|e| CloudEngineKind::from_engine_id(&e.id).map(|k| k.key()))
            .collect()
    }

    fn engine_list(
        &self,
        catalog: &[TtsVoice],
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(spacing(Spacing::Xs, density))
            .pb(spacing(Spacing::Sm, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("tts_engines_header_prefix")),
            )
            .child(
                div()
                    .text_size(FONT_MICRO)
                    .text_color(palette.text_faint)
                    .child(self.engines.len().to_string()),
            );

        let mut entries = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density));
        for (index, engine) in self.engines.iter().enumerate() {
            let count = engine_voice_count(catalog, &engine.id);
            let disabled = self
                .disabled
                .loaded()
                .is_some_and(|set| set.contains(&engine.id));
            entries = entries
                .child(self.engine_entry(index, engine, count, disabled, palette, density, cx));
        }

        let failure = self.disabled.failure().map(|reason| {
            div()
                .w_full()
                .pb(spacing(Spacing::Sm, density))
                .child(error_row_with_retry(
                    "tts-engines-disabled-retry",
                    tr!("tts_engines_disabled_load_failed", reason = reason),
                    palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.load_disabled(cx)),
                    density,
                ))
        });
        let column = div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .children(failure)
            .child(entries)
            .child(self.add_engine_button(palette, density, cx))
            .child(self.add_picker(palette, density, cx));

        div()
            .id("tts-engines-list")
            .w(RAIL_W)
            .flex_shrink_0()
            .h_full()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Xs, density))
            .overflow_y_scroll()
            .child(column)
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "render helper takes each displayed value and handler separately"
    )]
    fn engine_entry(
        &self,
        index: usize,
        engine: &EngineEntry,
        voice_count: usize,
        disabled: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let selected = self.selected == Selection::Engine(index);
        let name_color = if disabled {
            palette.text_faint
        } else if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };
        let dot_color = if disabled {
            palette.text_faint
        } else {
            engine_status_color(engine.kind, palette)
        };

        let identity = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(name_color)
                    .child(engine.name.clone()),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_MICRO)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "tts_engines_rail_sub",
                        kind = engine.kind,
                        count = voice_count as i64
                    )),
            );

        div()
            .id((gpui::ElementId::from("tts-engine"), engine.id.clone()))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .when(selected, |d| d.bg(palette.surface_overlay))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_engine(index, cx)))
            .child(status_dot(dot_color, STATUS_DOT))
            .child(identity)
            .into_any_element()
    }

    fn add_engine_button(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id("tts-add-engine")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(spacing(Spacing::Xxs, density))
            .mt(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_dashed()
            .border_color(palette.border_regular)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_add_picker(cx)))
            .child(icon(Icon::Plus, PLUS_GLYPH, palette.brand))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_LABEL)
                    .text_color(palette.brand)
                    .child(tr!("tts_engines_add_engine")),
            )
            .into_any_element()
    }

    fn add_picker(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        if !self.add_open {
            return div().into_any_element();
        }
        let configured = self.configured_cloud_kinds();
        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .mt(spacing(Spacing::Xxs, density));
        let mut any = false;
        for kind in CloudEngineKind::ALL {
            if configured.contains(kind.key()) {
                continue;
            }
            any = true;
            list = list.child(
                div()
                    .id(SharedString::from(format!("tts-add-pick-{}", kind.key())))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Sm, density))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(|s| s.bg(palette.surface_overlay))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.choose_add(kind, cx)),
                    )
                    .child(icon(Icon::Cloud, PLUS_GLYPH, palette.brand))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_secondary)
                            .child(kind.display_name()),
                    ),
            );
        }
        if !any {
            list = list.child(
                div()
                    .px(spacing(Spacing::Sm, density))
                    .py(spacing(Spacing::Xs, density))
                    .font_family(body_family())
                    .text_size(FONT_META)
                    .text_color(palette.text_faint)
                    .child(tr!("tts_engines_add_none_left")),
            );
        }
        list.into_any_element()
    }

    fn detail_pane(
        &self,
        catalog: &[TtsVoice],
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let inner: AnyElement = match self.selected {
            Selection::Engine(index) => match self.engines.get(index) {
                Some(engine) => self.engine_detail(engine, catalog, palette, density, cx),
                None => self.detail_hint(palette),
            },
            Selection::Adding(kind) => self.adding_detail(kind, palette, density, cx),
            Selection::None => self.detail_hint(palette),
        };

        div()
            .id("tts-engine-detail-scroll")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .bg(palette.base)
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .overflow_y_scroll()
            .child(inner)
            .into_any_element()
    }

    fn detail_hint(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("tts_engines_select_hint")),
            )
            .into_any_element()
    }

    fn engine_detail(
        &self,
        engine: &EngineEntry,
        catalog: &[TtsVoice],
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let voices: Vec<&TtsVoice> = catalog
            .iter()
            .filter(|v| v.engine_id.0 == engine.id)
            .collect();
        let is_cloud = CloudEngineKind::from_engine_id(&engine.id).is_some();
        let region = self.regions.get(&engine.id).map(|s| s.as_str());

        let mut col = div().w_full().flex().flex_col().child(self.detail_header(
            &engine.name,
            engine.kind,
            region,
            voices.len(),
            Some(&engine.id),
            palette,
            density,
            cx,
        ));
        if is_cloud {
            col = col.child(self.cloud.clone());
        }
        col.child(self.params_section(palette, density, cx))
            .child(self.voices_section(engine, &voices, palette, density, cx))
            .into_any_element()
    }

    fn adding_detail(
        &self,
        kind: CloudEngineKind,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(self.detail_header(
                kind.display_name(),
                "cloud",
                None,
                0,
                None,
                palette,
                density,
                cx,
            ))
            .child(self.cloud.clone())
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "render helper takes each displayed value and handler separately"
    )]
    fn detail_header(
        &self,
        name: &str,
        kind: &str,
        region: Option<&str>,
        voice_count: usize,
        engine_id: Option<&str>,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let tile = div()
            .w(TILE)
            .h(TILE)
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Md))
            .bg(palette.surface_overlay)
            .child(icon(engine_glyph(kind), TILE_GLYPH, palette.brand));

        let subtitle = match region {
            Some(region) => tr!(
                "tts_engines_detail_sub_region",
                kind = kind,
                region = region,
                count = voice_count as i64
            ),
            None => tr!(
                "tts_engines_detail_sub",
                kind = kind,
                count = voice_count as i64
            ),
        };

        let identity = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child(name.to_owned()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_LABEL)
                    .text_color(palette.text_muted)
                    .child(subtitle),
            );

        let right: AnyElement = match engine_id {
            Some(id) => match self.disabled.loaded() {
                Some(disabled) => {
                    let id_owned = id.to_owned();
                    toggle(!disabled.contains(id), palette)
                        .on_click(
                            SharedString::from(format!("engine-toggle-{id}")),
                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.toggle_engine(id_owned.clone(), cx)
                            }),
                        )
                        .into_any_element()
                }
                None => div()
                    .opacity(DISABLED_OPACITY)
                    .child(toggle(false, palette))
                    .into_any_element(),
            },
            None => div().into_any_element(),
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .mb(spacing(Spacing::Md, density))
            .child(tile)
            .child(identity)
            .child(right)
            .into_any_element()
    }

    fn voices_section(
        &self,
        engine: &EngineEntry,
        voices: &[&TtsVoice],
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(section_label(
                tr!("tts_engines_voices_header_prefix"),
                palette,
            ))
            .child(
                div()
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "tts_engines_voices_available",
                        count = voices.len() as i64
                    )),
            );

        let body: AnyElement = if voices.is_empty() {
            empty_state(tr!("tts_engines_voices_empty"), palette)
                .density(density)
                .into_any_element()
        } else {
            let mut grid = div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density));
            for row in voices.chunks(GRID_COLS) {
                let mut line = div().w_full().flex().gap(spacing(Spacing::Xs, density));
                for voice in row {
                    line = line.child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.voice_cell(&engine.id, voice, palette, density, cx)),
                    );
                }
                for _ in row.len()..GRID_COLS {
                    line = line.child(div().flex_1());
                }
                grid = grid.child(line);
            }
            grid.into_any_element()
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn voice_cell(
        &self,
        engine_id: &str,
        voice: &TtsVoice,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let avatar = avatar_tile(
            voice_initial(&voice.name),
            hash_accent(&voice.name, palette),
            palette,
        )
        .size(AVATAR)
        .font(FONT_META);

        let body = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(voice.name.clone()),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_MICRO)
                    .text_color(palette.text_faint)
                    .child(format!("{} · {}", voice.locale, voice_descriptor(voice))),
            );

        let engine_id = engine_id.to_owned();
        let voice_id = voice.id.0.clone();
        let play = div()
            .id(SharedString::from(format!("tts-voice-play-{voice_id}")))
            .flex_shrink_0()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.preview_voice(engine_id.clone(), voice_id.clone(), cx)
            }))
            .child(icon(Icon::PlayerPlay, PLAY_GLYPH, palette.success));

        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(avatar)
            .child(body)
            .child(play);

        div()
            .child(
                card(row, palette)
                    .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Sm, density))
                    .full_width(),
            )
            .into_any_element()
    }

    fn params_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let params = self.params.loaded().copied().unwrap_or_default();
        let failure = self.params.failure().map(|reason| {
            div()
                .w_full()
                .pb(spacing(Spacing::Sm, density))
                .child(error_row_with_retry(
                    "tts-engines-params-retry",
                    tr!("tts_engines_params_load_failed", reason = reason),
                    palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.retry_params(cx)),
                    density,
                ))
        });
        let rows = div()
            .w_full()
            .flex()
            .flex_col()
            .when(!self.params.is_loaded(), |d| d.opacity(DISABLED_OPACITY))
            .child(param_row(
                tr!("tts_engines_param_pitch"),
                format!("{:+.0} st", params.pitch_semitones),
                params.pitch_semitones,
                PITCH_MIN_SEMITONES,
                PITCH_MAX_SEMITONES,
                "tts-engines-pitch",
                cx.listener(|this, v: &f32, _, cx| this.set_pitch(*v, cx)),
                palette,
                density,
            ))
            .child(param_row(
                tr!("tts_engines_param_speed"),
                format!("{:.1}x", params.rate_multiplier),
                params.rate_multiplier,
                SPEED_MIN,
                SPEED_MAX,
                "tts-engines-speed",
                cx.listener(|this, v: &f32, _, cx| this.set_speed(*v, cx)),
                palette,
                density,
            ))
            .child(param_row(
                tr!("tts_engines_param_volume"),
                format!("{}%", (params.gain * 100.0).round() as i64),
                params.gain,
                GAIN_MIN,
                GAIN_MAX,
                "tts-engines-volume",
                cx.listener(|this, v: &f32, _, cx| this.set_engine_volume(*v, cx)),
                palette,
                density,
            ));

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .mb(spacing(Spacing::Md, density))
            .child(section_label(tr!("tts_engines_section_params"), palette))
            .children(failure)
            .child(
                card(rows, palette)
                    .radius(Radius::Md)
                    .padding(spacing(Spacing::Md, density))
                    .full_width(),
            )
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
                .font_family(body_family())
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
                .font_family(mono_family())
                .text_size(FONT_LABEL)
                .text_color(palette.text_primary)
                .child(value_text.into()),
        )
}

fn engine_status_color(kind: &str, palette: &ForgePalette) -> Rgba {
    if kind == "system" {
        palette.info
    } else {
        palette.success
    }
}

fn engine_glyph(kind: &str) -> Icon {
    if kind == "cloud" {
        Icon::Cloud
    } else {
        Icon::Cpu
    }
}

fn engine_voice_count(catalog: &[TtsVoice], engine_id: &str) -> usize {
    catalog
        .iter()
        .filter(|v| v.engine_id.0 == engine_id)
        .count()
}

fn voice_descriptor(voice: &TtsVoice) -> &'static str {
    match voice.gender {
        VoiceGender::Male => "male",
        VoiceGender::Female => "female",
        VoiceGender::Neutral => {
            if voice.is_neural {
                "neural"
            } else {
                "standard"
            }
        }
    }
}

fn voice_initial(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
}

fn load_roster(registry: Option<&Arc<RwLock<TtsRegistry>>>) -> Vec<EngineEntry> {
    let Some(registry) = registry else {
        return Vec::new();
    };
    let ids = registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .engine_ids();
    ids.into_iter()
        .map(|id| EngineEntry {
            kind: engine_kind(&id.0),
            name: engine_display_name(&id.0).to_owned(),
            id: id.0,
        })
        .collect()
}

fn engine_kind(id: &str) -> &'static str {
    match id {
        forge_tts_core::ENGINE_ID_PIPER | forge_tts_core::ENGINE_ID_ESPEAK_NG => "local",
        forge_tts_core::ENGINE_ID_SAPI | forge_tts_core::ENGINE_ID_NSSPEECH => "system",
        _ => "cloud",
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use forge_components::Toasts;
    use forge_storage::{Language, reserved_keys};
    use forge_tts_core::{
        ENGINE_ID_ESPEAK_NG, ENGINE_ID_PIPER, TtsEngine, TtsEngineFactory, TtsError,
    };
    use gpui::{Entity, TestAppContext};
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{
        ReadFault, SETTINGS_READ_FAILED, SettingWrite, TestBackend, error_toasts,
        install_presentation, pump, spawn_speak_queue, test_backend, writes_to,
    };

    const ESPEAK: usize = 0;
    const PIPER: usize = 1;
    const SETTLE_ROUNDS: usize = 3;
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

    struct NeverCreated;

    impl TtsEngineFactory for NeverCreated {
        fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
            Err(TtsError::AuthFailed {
                reason: "the engines pane never builds an engine".to_owned(),
            })
        }
    }

    fn params_key(engine_id: &str) -> String {
        format!("{}{engine_id}", reserved_keys::TTS_ENGINE_PARAMS_PREFIX)
    }

    fn undecodable(key: &str) -> String {
        format!("stored setting '{key}' cannot be decoded")
    }

    fn unreachable() -> String {
        StorageError::Connection {
            reason: SETTINGS_READ_FAILED.to_owned(),
        }
        .to_string()
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
            backend.seed(
                &params_key(ENGINE_ID_PIPER),
                &serde_json::to_string(&STORED_PIPER).unwrap(),
            );
            backend.seed(
                &params_key(ENGINE_ID_ESPEAK_NG),
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
            let mut registry = TtsRegistry::new();
            for id in [ENGINE_ID_PIPER, ENGINE_ID_ESPEAK_NG] {
                registry.register(EngineId(id.to_owned()), Arc::new(NeverCreated));
            }
            let registry = Some(Arc::new(RwLock::new(registry)));
            let credentials = Arc::clone(&self.backend) as Arc<dyn CredentialsRepo>;
            let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
            let handle = self.rt.handle().clone();
            let view = cx.update(|cx| {
                cx.new(|cx| {
                    TtsEnginesView::new(
                        registry,
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
            for _ in 0..SETTLE_ROUNDS {
                pump(&self.rt);
                cx.run_until_parked();
            }
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
                if written == reserved_keys::TTS_DISABLED_ENGINES {
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

    fn apply_fault(rig: &EnginesRig, key: &str, fault: &Fault) -> Option<String> {
        match fault {
            Fault::Unreachable => {
                rig.backend.fault_reads(key, ReadFault::Fails);
                Some(unreachable())
            }
            Fault::Undecodable => {
                rig.backend.seed(key, "not json");
                Some(undecodable(key))
            }
            Fault::Pending => {
                rig.backend
                    .fault_reads(key, ReadFault::Held(Arc::new(tokio::sync::Notify::new())));
                None
            }
        }
    }

    #[gpui::test]
    fn no_engine_toggle_writes_until_the_disabled_engines_load(cx: &mut TestAppContext) {
        for fault in [Fault::Unreachable, Fault::Undecodable, Fault::Pending] {
            let rig = EnginesRig::new(cx);
            let reason = apply_fault(&rig, reserved_keys::TTS_DISABLED_ENGINES, &fault);
            let view = rig.open(None, cx);

            view.update(cx, |v, cx| v.toggle_engine(ENGINE_ID_PIPER.to_owned(), cx));
            rig.settle(cx);

            let failure = view.read_with(cx, |v, _| v.disabled.failure().map(str::to_owned));
            assert_eq!(
                (rig.writes_to(reserved_keys::TTS_DISABLED_ENGINES), failure),
                (Vec::<String>::new(), reason.clone()),
                "{reason:?}"
            );
        }
    }

    #[gpui::test]
    fn no_slider_drag_writes_until_the_selected_engine_params_load(cx: &mut TestAppContext) {
        let key = params_key(ENGINE_ID_PIPER);
        for fault in [Fault::Unreachable, Fault::Undecodable, Fault::Pending] {
            for (case, edit) in slider_edits() {
                let rig = EnginesRig::new(cx);
                let _runtime = rig.rt.enter();
                let reason = apply_fault(&rig, &key, &fault);
                let view = rig.open(None, cx);
                select(&view, PIPER, &rig, cx);

                view.update(cx, edit);
                rig.past_the_save_debounce(cx);

                let failure = view.read_with(cx, |v, _| v.params.failure().map(str::to_owned));
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
            .fault_reads(reserved_keys::TTS_DISABLED_ENGINES, ReadFault::Fails);
        rig.backend.fault_reads(&key, ReadFault::Fails);
        let view = rig.open(None, cx);
        select(&view, PIPER, &rig, cx);
        rig.backend.heal_reads(reserved_keys::TTS_DISABLED_ENGINES);
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

    #[gpui::test]
    fn a_failed_disabled_write_toasts_reloads_and_resyncs_the_live_queue(cx: &mut TestAppContext) {
        let rig = EnginesRig::new(cx);
        let (speak, _events, _aliases) = {
            let _runtime = rig.rt.enter();
            spawn_speak_queue(Vec::new())
        };
        rig.backend.fail_writes(reserved_keys::TTS_DISABLED_ENGINES);
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
