use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, ForgePalette,
    InputEvent, Radius, Spacing, TextInput, card, radius, search_input, slider, spacing,
    status_dot,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, Subscription, Window, div,
    prelude::*, px,
};

use crate::presentation::ActivePresentation;

/// Configured-engine list rail width — the parity source pins it at a fixed 220px,
/// off the `Spacing` scale, so it is carried as a named literal.
const ENGINE_LIST_W: Pixels = px(220.0);
/// Voice-filter field width (the source's fixed 90px).
const VOICE_SEARCH_W: Pixels = px(90.0);
/// One voice card's fixed width in the wrapped voices grid (the source's 140px).
const VOICE_CELL_W: Pixels = px(140.0);
/// Parameter-row label column width (the source's fixed 70px).
const PARAM_LABEL_W: Pixels = px(70.0);
/// Parameter-row trailing value column width (the source's fixed 42px).
const PARAM_VALUE_W: Pixels = px(42.0);
/// Engine health-dot diameter (the source's fixed 7px dot).
const STATUS_DOT: Pixels = px(7.0);

/// One available voice of the selected engine. A cached view-model of a registered
/// voice; the live roster is fetched from the engine over the runtime→UI bridge,
/// never owned here.
struct VoiceRow {
    display_name: &'static str,
    locale: &'static str,
    /// `neural` or `standard`, mirroring the engine's neural-voice flag.
    quality: &'static str,
    /// `M`, `F` or `N`, mirroring the engine's declared voice gender.
    gender: &'static str,
}

/// One configured TTS engine. `kind` is `cloud`, `local` or `system` and drives the
/// health-dot hue; `is_default` marks the engine new utterances fall back to.
struct EngineEntry {
    name: &'static str,
    kind: &'static str,
    is_default: bool,
    voices: Vec<VoiceRow>,
}

/// The TTS Engines section view-entity: a configured-engine rail on the left and, for
/// the selected engine, a detail pane stacking an identity header, a credentials
/// notice, the default voice parameters and the searchable voices grid.
///
/// Owns its engine roster as seeded stub state — `forge-desktop` wires no TTS
/// registry yet, so the engines and their voices are seeded representative. The real
/// screen reads the registered engine roster and each engine's live voice list
/// through the TTS pipeline's engine registry over the runtime→UI bridge (the engine
/// factory's async `list_voices`), and selecting an engine dispatches that fetch; the
/// default voice parameters are the engine's persisted defaults. Here selection is a
/// view-state index and the parameters are a static read-only display, matching the
/// parity source (which exposes no parameter-edit control).
pub struct TtsEnginesView {
    engines: Vec<EngineEntry>,
    /// Index into `engines` of the engine whose detail pane is shown. Guarded against
    /// an empty roster at render, so it never dangles.
    selected: usize,
    voice_search: Entity<TextInput>,
    _voice_search_sub: Subscription,
}

impl TtsEnginesView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let voice_search = cx.new(|cx| search_input("Filter voices…", palette, cx));
        let voice_search_sub =
            cx.subscribe(&voice_search, |_this, _input, event: &InputEvent, cx| {
                // The filter reads the field's live content at render; a keystroke just
                // needs to trigger a repaint. Submit/cancel carry no extra behaviour.
                if let InputEvent::Changed(_) = event {
                    cx.notify();
                }
            });

        Self {
            engines: seed_engines(),
            selected: 0,
            voice_search,
            _voice_search_sub: voice_search_sub,
        }
    }

    /// Selects the engine at `index`, swapping the detail pane to it. Real path: this
    /// dispatches the engine's async voice-list fetch through the registry handle; the
    /// bridge lands the loaded voices back on this view.
    fn select_engine(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = index;
        cx.notify();
    }

    // --- engine list rail -------------------------------------------------

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
        let selected = self.selected == index;
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
                    .child(engine.name),
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

    // --- detail pane ------------------------------------------------------

    fn detail_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let inner: AnyElement = match self.engines.get(self.selected) {
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
                    .child(engine.name),
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

    // --- voices -----------------------------------------------------------

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

        let visible: Vec<&VoiceRow> = engine
            .voices
            .iter()
            .filter(|v| voice_matches(v.display_name, search))
            .collect();

        let body: AnyElement = if visible.is_empty() {
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

// ── view-specific fragments ───────────────────────────────────────────────

/// The dashed "more engines" hint pinned below the configured list.
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

/// The `DEFAULT` pill marking the fallback engine in the detail header.
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

/// The credentials block: a section heading over a notice card explaining that
/// credentials are stored encrypted, with a badge stating no credentials are needed
/// for the (local) engine.
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

/// The default voice parameters block: a section heading over three read-only
/// value rows (pitch / speed / volume). The parity source exposes no edit control,
/// so the rails are static value bars.
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

/// One parameter row: a fixed-width label, a read-only value bar filling the middle,
/// and a fixed-width trailing mono value.
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

/// One voice card: the voice name over a mono `locale · quality · gender` meta line.
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
                .child(voice.display_name),
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

/// A detail-pane block heading — an uppercase monospace caption inking `text_muted`.
fn section_label(label: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(label)
}

/// The engine health-dot hue: system engines ink the info hue, local and cloud
/// engines the ready (success) hue.
fn engine_status_color(kind: &str, palette: &ForgePalette) -> Rgba {
    if kind == "system" {
        palette.info
    } else {
        palette.success
    }
}

/// Case-insensitive substring match of a voice's display name against the filter.
/// An empty filter matches every voice.
fn voice_matches(display_name: &str, search: &str) -> bool {
    search.is_empty()
        || display_name
            .to_ascii_lowercase()
            .contains(&search.to_ascii_lowercase())
}

// ── seeded stub state ─────────────────────────────────────────────────────

/// The representative engine roster the section seeds before a TTS registry is
/// wired: a mix of cloud, local and system engines so every health hue and the
/// default marker render, each carrying its own representative voice list.
fn seed_engines() -> Vec<EngineEntry> {
    vec![
        EngineEntry {
            name: "Amazon Polly",
            kind: "cloud",
            is_default: true,
            voices: vec![
                VoiceRow {
                    display_name: "Olena",
                    locale: "uk-UA",
                    quality: "neural",
                    gender: "F",
                },
                VoiceRow {
                    display_name: "Maksym",
                    locale: "uk-UA",
                    quality: "neural",
                    gender: "M",
                },
                VoiceRow {
                    display_name: "Tatiana",
                    locale: "ru-RU",
                    quality: "neural",
                    gender: "F",
                },
                VoiceRow {
                    display_name: "Mathieu",
                    locale: "fr-FR",
                    quality: "neural",
                    gender: "M",
                },
                VoiceRow {
                    display_name: "Joanna",
                    locale: "en-US",
                    quality: "neural",
                    gender: "F",
                },
                VoiceRow {
                    display_name: "Matthew",
                    locale: "en-US",
                    quality: "standard",
                    gender: "M",
                },
            ],
        },
        EngineEntry {
            name: "ElevenLabs",
            kind: "cloud",
            is_default: false,
            voices: vec![
                VoiceRow {
                    display_name: "Rachel",
                    locale: "en-US",
                    quality: "neural",
                    gender: "F",
                },
                VoiceRow {
                    display_name: "Antoni",
                    locale: "en-US",
                    quality: "neural",
                    gender: "M",
                },
                VoiceRow {
                    display_name: "Bella",
                    locale: "en-US",
                    quality: "neural",
                    gender: "F",
                },
            ],
        },
        EngineEntry {
            name: "Piper",
            kind: "local",
            is_default: false,
            voices: vec![
                VoiceRow {
                    display_name: "UA-1",
                    locale: "uk-UA",
                    quality: "standard",
                    gender: "N",
                },
                VoiceRow {
                    display_name: "EN-US-1",
                    locale: "en-US",
                    quality: "standard",
                    gender: "M",
                },
            ],
        },
        EngineEntry {
            name: "eSpeak-NG",
            kind: "local",
            is_default: false,
            voices: vec![
                VoiceRow {
                    display_name: "uk",
                    locale: "uk-UA",
                    quality: "standard",
                    gender: "N",
                },
                VoiceRow {
                    display_name: "en",
                    locale: "en-US",
                    quality: "standard",
                    gender: "N",
                },
            ],
        },
        EngineEntry {
            name: "Microsoft SAPI 5",
            kind: "system",
            is_default: false,
            voices: vec![
                VoiceRow {
                    display_name: "David",
                    locale: "en-US",
                    quality: "standard",
                    gender: "M",
                },
                VoiceRow {
                    display_name: "Zira",
                    locale: "en-US",
                    quality: "standard",
                    gender: "F",
                },
            ],
        },
    ]
}
