use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use forge_components::{
    BORDER_THIN, Density, FONT_MD, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing,
    avatar_tile, body_family, card, empty_state, hash_accent, icon, mono_family, radius,
    section_label, slider, spacing, status_dot, toggle, tr,
};
use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest};
use forge_storage::{CredentialId, CredentialsRepo, EngineParams, SettingsRepo};
use forge_tts_core::{EngineId, TtsRegistry, TtsVoice, VoiceGender, VoiceId};
use forge_types::EventId;
use forge_voice::SynthesisDefaults;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::async_bridge::{self, ErrorSink};
use crate::cloud_credentials::{CloudCredentialsView, CloudEngineKind, CloudEngineRegistered};
use crate::presentation::ActivePresentation;

const RAIL_W: Pixels = px(240.0);
const TILE: Pixels = px(36.0);
const AVATAR: Pixels = px(26.0);
const PARAM_LABEL_W: Pixels = px(60.0);
const PARAM_VALUE_W: Pixels = px(60.0);
const STATUS_DOT: Pixels = px(7.0);
const PLUS_GLYPH: Pixels = px(12.0);
const TILE_GLYPH: Pixels = px(18.0);
const PLAY_GLYPH: Pixels = px(14.0);
const GRID_COLS: usize = 3;
const FS_10: Pixels = px(10.0);
const FS_11: Pixels = px(11.0);
const FS_11_5: Pixels = px(11.5);

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
    disabled: HashSet<String>,
    pitch_semitones: f32,
    rate_multiplier: f32,
    volume: f32,
    add_open: bool,
    regions: HashMap<String, String>,
    cloud: Entity<CloudCredentialsView>,
    params_debounce: async_bridge::Debounced,
    _subs: Vec<Subscription>,
}

impl TtsEnginesView {
    pub fn new(
        registry: Option<Arc<RwLock<TtsRegistry>>>,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let disabled = speak
            .as_ref()
            .map(|h| h.disabled_engines().iter().map(|e| e.0.clone()).collect())
            .unwrap_or_default();

        let cloud = cx.new(|cx| {
            CloudCredentialsView::new(
                registry.clone(),
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

        Self {
            engines: load_roster(registry.as_ref()),
            registry,
            credentials,
            settings,
            speak,
            rt_handle,
            selected: Selection::None,
            disabled,
            pitch_semitones: SynthesisDefaults::default().pitch_semitones,
            rate_multiplier: SynthesisDefaults::default().rate_multiplier,
            volume: 1.0,
            add_open: false,
            regions: HashMap::new(),
            cloud,
            params_debounce: async_bridge::Debounced::new(async_bridge::SLIDER_PERSIST_DEBOUNCE),
            _subs: vec![sub],
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
            self.seed_engine_params(&id);
        }
        cx.notify();
    }

    fn seed_engine_params(&mut self, engine_id: &str) {
        let Some(speak) = self.speak.as_ref() else {
            return;
        };
        let id = EngineId(engine_id.to_owned());
        let defaults = speak.engine_synthesis_defaults(&id);
        self.pitch_semitones = defaults.pitch_semitones;
        self.rate_multiplier = defaults.rate_multiplier;
        self.volume = speak.engine_gain(&id);
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
        if self.disabled.contains(&engine_id) {
            self.disabled.remove(&engine_id);
        } else {
            self.disabled.insert(engine_id.clone());
        }
        let now_enabled = !self.disabled.contains(&engine_id);
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
        self.persist_disabled(cx);
        cx.notify();
    }

    fn persist_disabled(&self, cx: &mut Context<Self>) {
        let settings = Arc::clone(&self.settings);
        let ids: Vec<String> = self.disabled.iter().cloned().collect();
        async_bridge::report_failure(
            &self.rt_handle,
            async move { forge_storage::set_disabled_tts_engines(settings.as_ref(), &ids).await },
            ErrorSink::Toast,
            tr!("tts_engines_persist_disabled_failed"),
            cx,
        );
    }

    fn set_pitch(&mut self, value: f32, cx: &mut Context<Self>) {
        self.pitch_semitones = value.clamp(-12.0, 12.0);
        self.push_engine_params();
        cx.notify();
    }

    fn set_speed(&mut self, value: f32, cx: &mut Context<Self>) {
        self.rate_multiplier = value.clamp(0.5, 2.0);
        self.push_engine_params();
        cx.notify();
    }

    fn set_engine_volume(&mut self, value: f32, cx: &mut Context<Self>) {
        self.volume = value.clamp(0.0, 1.0);
        self.push_engine_params();
        cx.notify();
    }

    fn push_engine_params(&mut self) {
        let Some(engine_id) = self.selected_engine_id() else {
            return;
        };
        let defaults = SynthesisDefaults {
            pitch_semitones: self.pitch_semitones,
            rate_multiplier: self.rate_multiplier,
        };
        let gain = self.volume;
        let speak = self.speak.clone();
        let settings = Arc::clone(&self.settings);
        let params = EngineParams {
            pitch_semitones: defaults.pitch_semitones,
            rate_multiplier: defaults.rate_multiplier,
            gain,
        };
        let persist_id = engine_id.clone();
        let eid = EngineId(engine_id);
        self.params_debounce
            .schedule(&self.rt_handle, "engine params", async move {
                if let Some(speak) = speak {
                    speak
                        .send(SpeakCommand::SetEngineParams(eid, defaults, gain))
                        .await
                        .map_err(|e| e.to_string())?;
                }
                forge_storage::set_engine_params(settings.as_ref(), &persist_id, params)
                    .await
                    .map_err(|e| e.to_string())
            });
    }

    fn on_engine_registered(&mut self, engine_id: &EngineId, cx: &mut Context<Self>) {
        self.engines = load_roster(self.registry.as_ref());
        if let Some(index) = self.engines.iter().position(|e| e.id == engine_id.0) {
            self.selected = Selection::Engine(index);
            self.seed_engine_params(&engine_id.0.clone());
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
            alias_override: None,
            engine_override: Some(EngineId(engine_id)),
            voice_override: Some(VoiceId(voice_id)),
            source_event_id: EventId::new(),
            is_reward: false,
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
                    .text_size(FS_10)
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
            let disabled = self.disabled.contains(&engine.id);
            entries = entries
                .child(self.engine_entry(index, engine, count, disabled, palette, density, cx));
        }

        let column = div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
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

    #[allow(clippy::too_many_arguments)]
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
                    .text_size(FS_10)
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
                    .text_size(FS_11_5)
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
                    .text_size(FS_11)
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

    #[allow(clippy::too_many_arguments)]
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
                    .text_size(FS_11_5)
                    .text_color(palette.text_muted)
                    .child(subtitle),
            );

        let right: AnyElement = match engine_id {
            Some(id) => {
                let on = !self.disabled.contains(id);
                let id_owned = id.to_owned();
                toggle(on, palette)
                    .on_click(
                        SharedString::from(format!("engine-toggle-{id}")),
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.toggle_engine(id_owned.clone(), cx)
                        }),
                    )
                    .into_any_element()
            }
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
        .font(FS_11);

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
                    .text_size(FS_10)
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
        let rows = div()
            .w_full()
            .flex()
            .flex_col()
            .child(param_row(
                tr!("tts_engines_param_pitch"),
                format!("{:+.0} st", self.pitch_semitones),
                self.pitch_semitones,
                -12.0,
                12.0,
                "tts-engines-pitch",
                cx.listener(|this, v: &f32, _, cx| this.set_pitch(*v, cx)),
                palette,
                density,
            ))
            .child(param_row(
                tr!("tts_engines_param_speed"),
                format!("{:.1}x", self.rate_multiplier),
                self.rate_multiplier,
                0.5,
                2.0,
                "tts-engines-speed",
                cx.listener(|this, v: &f32, _, cx| this.set_speed(*v, cx)),
                palette,
                density,
            ))
            .child(param_row(
                tr!("tts_engines_param_volume"),
                format!("{}%", (self.volume * 100.0).round() as i64),
                self.volume,
                0.0,
                1.0,
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

#[allow(clippy::too_many_arguments)]
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
                .text_size(FS_11_5)
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
            name: engine_label(&id.0),
            id: id.0,
        })
        .collect()
}

fn engine_label(id: &str) -> String {
    match id {
        "piper" => "Piper",
        "espeak-ng" => "eSpeak-NG",
        "sapi" => "Microsoft SAPI 5",
        "nsspeech" => "Apple AVSpeech",
        "azure" => "Azure Speech",
        "elevenlabs" => "ElevenLabs",
        "openai" => "OpenAI TTS",
        "polly" => "Amazon Polly",
        other => return other.to_owned(),
    }
    .to_owned()
}

fn engine_kind(id: &str) -> &'static str {
    match id {
        "piper" | "espeak-ng" => "local",
        "sapi" | "nsspeech" => "system",
        _ => "cloud",
    }
}
