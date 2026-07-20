use std::sync::{Arc, RwLock};

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_MD, FONT_XS, FONT_XXS,
    ForgePalette, Icon, Radius, Spacing, card, icon, radius, slider, spacing, status_dot, tr,
};
use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest};
use forge_tts_core::{EngineId, TtsRegistry, TtsVoice, VoiceGender, VoiceId};
use forge_types::EventId;
use gpui::{
    AnyElement, ClickEvent, Context, EventEmitter, FontWeight, Pixels, Rgba, SharedString, Window,
    div, prelude::*, px,
};

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

pub struct AddEngineRequested;

struct EngineEntry {
    id: String,
    name: String,
    kind: &'static str,
}

pub struct TtsEnginesView {
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    engines: Vec<EngineEntry>,
    selected: Option<usize>,
}

impl EventEmitter<AddEngineRequested> for TtsEnginesView {}

impl TtsEnginesView {
    pub fn new(
        registry: Option<Arc<RwLock<TtsRegistry>>>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            engines: load_roster(registry.as_ref()),
            speak,
            rt_handle,
            selected: None,
        }
    }

    fn catalog(&self) -> Arc<Vec<TtsVoice>> {
        self.speak
            .as_ref()
            .map(|h| h.available_voices())
            .unwrap_or_default()
    }

    fn select_engine(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = Some(index);
        cx.notify();
    }

    fn request_add_engine(&mut self, cx: &mut Context<Self>) {
        cx.emit(AddEngineRequested);
    }

    fn preview_voice(&self, engine_id: String, voice_id: String) {
        let Some(speak) = self.speak.clone() else {
            eprintln!("forge-desktop: voice preview dropped - speak queue unavailable");
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
        self.rt_handle.spawn(async move {
            if let Err(err) = speak.send(SpeakCommand::Enqueue(request)).await {
                eprintln!("forge-desktop: voice preview enqueue failed: {err}");
            }
        });
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
                    .font_family(DEFAULT_MONO_FAMILY)
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
            entries = entries.child(self.engine_entry(index, engine, count, palette, density, cx));
        }

        let column = div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .child(entries)
            .child(self.add_engine_button(palette, density, cx));

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

    fn engine_entry(
        &self,
        index: usize,
        engine: &EngineEntry,
        voice_count: usize,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let selected = self.selected == Some(index);
        let name_color = if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };

        let identity = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(name_color)
                    .child(engine.name.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FS_10)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "tts_engines_rail_sub",
                        kind = engine.kind,
                        count = voice_count as i64
                    )),
            );

        div()
            .id(("tts-engine", index))
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
            .child(status_dot(
                engine_status_color(engine.kind, palette),
                STATUS_DOT,
            ))
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
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.request_add_engine(cx)))
            .child(icon(Icon::Plus, PLUS_GLYPH, palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FS_11_5)
                    .text_color(palette.brand)
                    .child(tr!("tts_engines_add_engine")),
            )
            .into_any_element()
    }

    fn detail_pane(
        &self,
        catalog: &[TtsVoice],
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let inner: AnyElement = match self.selected.and_then(|i| self.engines.get(i)) {
            Some(engine) => self.engine_detail(engine, catalog, palette, density, cx),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(tr!("tts_engines_select_hint")),
                )
                .into_any_element(),
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

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(self.detail_header(engine, voices.len(), palette, density))
            .child(params_section(palette, density))
            .child(self.voices_section(engine, &voices, palette, density, cx))
            .into_any_element()
    }

    fn detail_header(
        &self,
        engine: &EngineEntry,
        voice_count: usize,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let status_color = engine_status_color(engine.kind, palette);

        let tile = div()
            .w(TILE)
            .h(TILE)
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Md))
            .bg(palette.surface_overlay)
            .child(icon(engine_glyph(engine.kind), TILE_GLYPH, palette.brand));

        let identity = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child(engine.name.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FS_11_5)
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "tts_engines_detail_sub",
                        kind = engine.kind,
                        count = voice_count as i64
                    )),
            );

        let ready = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(status_color, STATUS_DOT))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(status_color)
                    .child(tr!("tts_engines_status_ready")),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .mb(spacing(Spacing::Md, density))
            .child(tile)
            .child(identity)
            .child(ready)
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
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("tts_engines_voices_empty"))
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
        let avatar = div()
            .w(AVATAR)
            .h(AVATAR)
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Sm))
            .bg(avatar_color(&voice.name, palette))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FS_11)
                    .text_color(palette.shell)
                    .child(voice_initial(&voice.name)),
            );

        let body = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(voice.name.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
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
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.preview_voice(engine_id.clone(), voice_id.clone())
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

fn params_section(palette: &ForgePalette, density: Density) -> impl IntoElement {
    let rows = div()
        .w_full()
        .flex()
        .flex_col()
        .child(param_row(
            tr!("tts_engines_param_pitch"),
            "+0 st",
            0.5,
            palette,
            density,
        ))
        .child(param_row(
            tr!("tts_engines_param_speed"),
            "1.0x",
            0.5,
            palette,
            density,
        ))
        .child(param_row(
            tr!("tts_engines_param_volume"),
            "100%",
            1.0,
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

fn param_row(
    label: impl Into<SharedString>,
    value: &'static str,
    fraction: f32,
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
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(slider(fraction, 0.0, 1.0, palette)),
        )
        .child(
            div()
                .w(PARAM_VALUE_W)
                .flex_shrink_0()
                .text_right()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FS_11_5)
                .text_color(palette.text_primary)
                .child(value),
        )
}

fn section_label(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label)
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

fn avatar_color(name: &str, palette: &ForgePalette) -> Rgba {
    let accents = [
        palette.brand,
        palette.info,
        palette.success,
        palette.warning,
        palette.bits,
        palette.accent_teal,
        palette.accent_pink_light,
    ];
    let hash = name
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    accents[hash % accents.len()]
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
