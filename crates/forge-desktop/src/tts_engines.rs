use std::sync::{Arc, RwLock};

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, ForgePalette,
    InputEvent, Radius, Spacing, TextInput, card, radius, search_input, slider, spacing,
    status_dot,
};
use forge_tts_core::{EngineId, TtsRegistry, TtsVoice, VoiceGender};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, Subscription, Window, div,
    prelude::*, px,
};

use crate::presentation::ActivePresentation;

const ENGINE_LIST_W: Pixels = px(220.0);
const VOICE_SEARCH_W: Pixels = px(90.0);
const VOICE_CELL_W: Pixels = px(140.0);
const PARAM_LABEL_W: Pixels = px(70.0);
const PARAM_VALUE_W: Pixels = px(42.0);
const STATUS_DOT: Pixels = px(7.0);

struct VoiceRow {
    display_name: String,
    locale: String,
    quality: &'static str,
    gender: &'static str,
}

struct EngineEntry {
    id: String,
    name: String,
    kind: &'static str,
    is_default: bool,
    voices: Vec<VoiceRow>,
}

pub struct TtsEnginesView {
    registry: Option<Arc<RwLock<TtsRegistry>>>,
    rt_handle: tokio::runtime::Handle,
    engines: Vec<EngineEntry>,
    selected: Option<usize>,
    voices_loading: bool,
    voice_search: Entity<TextInput>,
    _voice_search_sub: Subscription,
}

impl TtsEnginesView {
    pub fn new(
        registry: Option<Arc<RwLock<TtsRegistry>>>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let voice_search = cx.new(|cx| search_input("Filter voices…", palette, cx));
        let voice_search_sub =
            cx.subscribe(&voice_search, |_this, _input, event: &InputEvent, cx| {
                if let InputEvent::Changed(_) = event {
                    cx.notify();
                }
            });

        Self {
            engines: load_roster(registry.as_ref()),
            registry,
            rt_handle,
            selected: None,
            voices_loading: false,
            voice_search,
            _voice_search_sub: voice_search_sub,
        }
    }

    fn select_engine(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = Some(index);
        if let Some(engine) = self.engines.get_mut(index) {
            engine.voices.clear();
        }
        let Some(registry) = self.registry.clone() else {
            self.voices_loading = false;
            cx.notify();
            return;
        };
        let Some(engine_id) = self.engines.get(index).map(|e| EngineId(e.id.clone())) else {
            cx.notify();
            return;
        };
        self.voices_loading = true;
        cx.notify();

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(fetch_engine_voices(registry, engine_id).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.on_voices_loaded(index, result, cx));
            }
        })
        .detach();
    }

    fn on_voices_loaded(
        &mut self,
        index: usize,
        result: Result<Vec<VoiceRow>, String>,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(index) {
            return;
        }
        self.voices_loading = false;
        match result {
            Ok(voices) => {
                if let Some(engine) = self.engines.get_mut(index) {
                    engine.voices = voices;
                }
            }
            Err(err) => {
                let engine = self
                    .engines
                    .get(index)
                    .map(|e| e.name.as_str())
                    .unwrap_or("");
                tracing::warn!(error = %err, engine, "failed to list engine voices");
            }
        }
        cx.notify();
    }

    fn engine_list(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(format!("CONFIGURED · {}", self.engines.len())),
            );

        let mut col = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(header);
        for (index, engine) in self.engines.iter().enumerate() {
            col = col.child(self.engine_card(index, engine, palette, density, cx));
        }
        col = col.child(engine_list_placeholder(palette, density));

        div()
            .id("tts-engines-list")
            .w(ENGINE_LIST_W)
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .child(col)
            .into_any_element()
    }

    fn engine_card(
        &self,
        index: usize,
        engine: &EngineEntry,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected == Some(index);
        let (border_color, border_w) = if selected {
            (palette.brand, px(1.0))
        } else {
            (palette.border_regular, BORDER_THIN)
        };

        let name_row = div()
            .w_full()
            .flex()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(engine.name.clone()),
            )
            .child(status_dot(
                engine_status_color(engine.kind, palette),
                STATUS_DOT,
            ));

        let meta = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(engine.kind);

        div()
            .id(("tts-engine", index))
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .bg(palette.elevated)
            .border(border_w)
            .border_color(border_color)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_engine(index, cx)))
            .child(name_row)
            .child(meta)
            .into_any_element()
    }

    fn detail_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let inner: AnyElement = match self.selected.and_then(|i| self.engines.get(i)) {
            Some(engine) => self.engine_detail(engine, palette, density, cx),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_muted)
                        .child("Select an engine to configure"),
                )
                .into_any_element(),
        };

        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .rounded(radius(Radius::Lg))
            .child(inner)
            .into_any_element()
    }

    fn engine_detail(
        &self,
        engine: &EngineEntry,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let voice_count = engine.voices.len();
        let column = div()
            .w_full()
            .flex()
            .flex_col()
            .child(self.detail_header(engine, voice_count, palette, density))
            .child(credentials_section(palette, density))
            .child(params_section(palette, density))
            .child(self.voices_section(engine, palette, density, cx));

        div()
            .id("tts-engine-detail-scroll")
            .size_full()
            .overflow_y_scroll()
            .child(column)
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

        let mut title_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(engine.name.clone()),
            );
        if engine.is_default {
            title_row = title_row.child(default_badge(palette, density));
        }

        let sub = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(format!("{} · {voice_count} voices", engine.kind));

        let identity = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(title_row)
            .child(sub);

        let credentials_status = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(status_dot(status_color, STATUS_DOT))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(status_color)
                    .child("Ready"),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(identity)
            .child(credentials_status)
            .into_any_element()
    }

    fn voices_section(
        &self,
        engine: &EngineEntry,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let search = self.voice_search.read(cx).content();

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(format!("AVAILABLE VOICES · {}", engine.voices.len())),
            )
            .child(div().w(VOICE_SEARCH_W).child(self.voice_search.clone()));

        let body: AnyElement = if self.voices_loading {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("Loading voices…")
                .into_any_element()
        } else {
            let visible: Vec<&VoiceRow> = engine
                .voices
                .iter()
                .filter(|v| voice_matches(&v.display_name, search))
                .collect();

            if visible.is_empty() {
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("No voices found")
                    .into_any_element()
            } else {
                let mut grid = div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .gap(spacing(Spacing::Xs, density));
                for voice in visible {
                    grid = grid.child(voice_cell(voice, palette, density));
                }
                grid.into_any_element()
            }
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(header)
            .child(body)
            .into_any_element()
    }
}

impl Render for TtsEnginesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let list = self.engine_list(&palette, density, cx);
        let detail = self.detail_pane(&palette, density, cx);

        div()
            .size_full()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Md, density))
            .bg(palette.base)
            .child(list)
            .child(detail)
    }
}

fn engine_list_placeholder(palette: &ForgePalette, density: Density) -> impl IntoElement {
    div()
        .w_full()
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("+ More engines in future releases"),
        )
}

fn default_badge(palette: &ForgePalette, density: Density) -> impl IntoElement {
    div()
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Pill))
        .bg(palette.surface_overlay)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_XS)
                .text_color(palette.brand)
                .child("DEFAULT"),
        )
}

fn credentials_section(palette: &ForgePalette, density: Density) -> impl IntoElement {
    let no_credentials = div()
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.surface_overlay)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.success)
                .child("LOCAL — no credentials"),
        );

    let notice_row = div()
        .w_full()
        .flex()
        .items_center()
        .child(
            div()
                .flex_1()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("Credentials stored encrypted in the local database, never in config files"),
        )
        .child(no_credentials);

    let notice = card(notice_row, palette)
        .background(palette.shell)
        .radius(Radius::Sm)
        .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
        .full_width();

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(section_label("ENGINE", palette))
        .child(notice)
}

fn params_section(palette: &ForgePalette, density: Density) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(section_label("DEFAULT VOICE PARAMETERS", palette))
        .child(param_row("Pitch", "0 st", 0.5, palette, density))
        .child(param_row("Speed", "1.0x", 0.5, palette, density))
        .child(param_row("Volume", "0 dB", 1.0, palette, density))
}

fn param_row(
    label: &'static str,
    value: &'static str,
    fraction: f32,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(
            div()
                .w(PARAM_LABEL_W)
                .flex_shrink_0()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
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
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(value),
        )
}

fn voice_cell(voice: &VoiceRow, palette: &ForgePalette, density: Density) -> impl IntoElement {
    let body = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(voice.display_name.clone()),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(format!(
                    "{} · {} · {}",
                    voice.locale, voice.quality, voice.gender
                )),
        );

    div().w(VOICE_CELL_W).child(
        card(body, palette)
            .background(palette.shell)
            .radius(Radius::Sm)
            .padding(spacing(Spacing::Xs, density))
            .full_width(),
    )
}

fn section_label(label: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
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

fn voice_matches(display_name: &str, search: &str) -> bool {
    search.is_empty()
        || display_name
            .to_ascii_lowercase()
            .contains(&search.to_ascii_lowercase())
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
        .enumerate()
        .map(|(index, id)| EngineEntry {
            kind: engine_kind(&id.0),
            name: engine_label(&id.0),
            is_default: index == 0,
            voices: Vec::new(),
            id: id.0,
        })
        .collect()
}

async fn fetch_engine_voices(
    registry: Arc<RwLock<TtsRegistry>>,
    engine_id: EngineId,
) -> Result<Vec<VoiceRow>, String> {
    let factory = registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&engine_id);
    let Some(factory) = factory else {
        return Err(format!("engine {} is not registered", engine_id.0));
    };
    let engine = factory.create().map_err(|e| e.to_string())?;
    let voices = engine.list_voices().await.map_err(|e| e.to_string())?;
    Ok(voices.into_iter().map(voice_row_from).collect())
}

fn voice_row_from(voice: TtsVoice) -> VoiceRow {
    VoiceRow {
        display_name: voice.name,
        locale: voice.locale,
        quality: if voice.is_neural {
            "neural"
        } else {
            "standard"
        },
        gender: match voice.gender {
            VoiceGender::Male => "M",
            VoiceGender::Female => "F",
            VoiceGender::Neutral => "N",
        },
    }
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
