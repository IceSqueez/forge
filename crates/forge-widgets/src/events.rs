use forge_events::EventSource;
use iced::{
    Background, Border, Color, Element, Length, Padding,
    widget::{Space, button, column, container, row, scrollable},
};

use crate::{
    icons::{Icon, tabler_icon},
    palette::ForgePalette,
    tokens::{FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf},
};

pub fn color_for_source(source: EventSource, palette: &ForgePalette) -> Color {
    match source {
        EventSource::Twitch => palette.brand,
        EventSource::YouTube => palette.random,
        EventSource::Kick => palette.info,
        EventSource::Core => palette.warning,
        EventSource::Rhai => palette.warning,
        EventSource::Http => palette.random,
        EventSource::Obs => palette.success,
        EventSource::VTube => palette.accent_teal,
        EventSource::Discord => palette.brand,
        EventSource::Midi => palette.bits,
        EventSource::Hotkey => palette.bits,
        EventSource::Timer => palette.warning,
        EventSource::Server => palette.info,
        EventSource::Audio => palette.bits,
    }
}

pub fn source_label(source: EventSource) -> &'static str {
    match source {
        EventSource::Twitch => "TWITCH",
        EventSource::YouTube => "YOUTUBE",
        EventSource::Kick => "KICK",
        EventSource::Core => "CORE",
        EventSource::Rhai => "RHAI",
        EventSource::Http => "HTTP",
        EventSource::Obs => "OBS",
        EventSource::VTube => "VTUBE",
        EventSource::Discord => "DISCORD",
        EventSource::Midi => "MIDI",
        EventSource::Hotkey => "HOTKEY",
        EventSource::Timer => "TIMER",
        EventSource::Server => "SERVER",
        EventSource::Audio => "AUDIO",
    }
}

pub fn source_badge<'a, Msg: 'a>(source: EventSource, palette: &ForgePalette) -> Element<'a, Msg> {
    let fg = color_for_source(source, palette);
    let bg = palette.surface_overlay;
    let label = source_label(source);

    let txt = iced::widget::text(label)
        .size(FONT_XS)
        .color(fg)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..font(FontRole::Monospace)
        });

    container(txt)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub struct EventRowData {
    pub timestamp: String,
    pub source: EventSource,
    pub event_type: String,
    pub summary: String,
    pub result_tag: Option<String>,
    pub is_error: bool,
}

pub fn event_row_observability<'a, Msg: Clone + 'a>(
    event: &EventRowData,
    selected: bool,
    on_click: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let mono = font(FontRole::Monospace);
    let is_error = event.is_error;

    let accent_color = if is_error {
        palette.random
    } else if selected {
        palette.brand
    } else {
        Color::TRANSPARENT
    };

    let bg_selected = palette.elevated;
    let bg_error = Color {
        r: palette.random.r,
        g: palette.random.g,
        b: palette.random.b,
        a: 0.06,
    };
    let bg_hover = Color {
        r: palette.brand.r,
        g: palette.brand.g,
        b: palette.brand.b,
        a: 0.05,
    };
    let sep_color = palette.elevated;

    let accent_bar = container(iced::widget::Space::new().width(2))
        .height(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(accent_color)),
            ..container::Style::default()
        });

    let ts = iced::widget::text(event.timestamp.clone())
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(mono)
        .width(80);

    let badge = source_badge(event.source, palette);

    let etype = container(
        iced::widget::text(event.event_type.clone())
            .size(FONT_XS)
            .color(palette.text_primary)
            .font(mono),
    )
    .width(104);

    let summary = container(
        iced::widget::text(event.summary.clone())
            .size(FONT_XS)
            .color(palette.text_secondary)
            .font(mono),
    )
    .width(Length::Fill)
    .clip(true);

    let result_color = if is_error {
        palette.random
    } else {
        match event.result_tag.as_deref() {
            Some("ok") | Some("sent") => palette.success,
            Some("err") => palette.random,
            _ => palette.text_muted,
        }
    };

    let mut content_row = row![ts, badge, etype, summary]
        .spacing(10)
        .align_y(iced::Alignment::Center);

    if let Some(tag) = &event.result_tag {
        content_row = content_row.push(
            iced::widget::text(tag.clone())
                .size(FONT_XS)
                .color(result_color)
                .font(mono),
        );
    }

    let content = container(content_row)
        .padding(Padding {
            top: spf(Spacing::Xxs),
            right: spf(Spacing::Md),
            bottom: spf(Spacing::Xxs),
            left: spf(Spacing::Sm),
        })
        .width(Length::Fill);

    let full_row = row![accent_bar, content];

    let separator = container(iced::widget::Space::new().width(Length::Fill).height(1)).style(
        move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(sep_color)),
            ..container::Style::default()
        },
    );

    let btn = button(full_row)
        .on_press(on_click)
        .padding(0)
        .width(Length::Fill)
        .style(
            move |_theme: &iced::Theme, status: button::Status| button::Style {
                background: match status {
                    button::Status::Hovered if !selected && !is_error => {
                        Some(Background::Color(bg_hover))
                    }
                    _ if selected => Some(Background::Color(bg_selected)),
                    _ if is_error => Some(Background::Color(bg_error)),
                    _ => None,
                },
                text_color: Color::TRANSPARENT,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        );

    column![btn, separator].into()
}

pub fn causation_chip<'a, Msg: Clone + 'a>(
    label: &'a str,
    meta: &'a str,
    on_click: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let mono = font(FontRole::Monospace);
    let brand = palette.brand;
    let elevated = palette.elevated;
    let border_color = palette.border_regular;
    let text_primary = palette.text_primary;
    let text_faint = palette.text_faint;

    let hover_bg = Color {
        r: brand.r,
        g: brand.g,
        b: brand.b,
        a: 0.10,
    };

    let icon = tabler_icon(Icon::Bolt, FONT_XS, brand);

    let name = iced::widget::text(label)
        .size(FONT_XS)
        .color(text_primary)
        .width(Length::Fill);

    let badge = iced::widget::text(meta)
        .size(FONT_XS)
        .color(text_faint)
        .font(mono);

    let content = row![icon, name, badge]
        .spacing(8)
        .align_y(iced::Alignment::Center);

    button(content)
        .on_press(on_click)
        .padding(Padding {
            top: spf(Spacing::Xs),
            right: spf(Spacing::Sm),
            bottom: spf(Spacing::Xs),
            left: spf(Spacing::Sm),
        })
        .width(Length::Fill)
        .style(
            move |_theme: &iced::Theme, status: button::Status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Some(Background::Color(hover_bg))
                    }
                    _ => Some(Background::Color(elevated)),
                },
                text_color: text_primary,
                border: Border {
                    color: border_color,
                    width: 0.5,
                    radius: radius(Radius::Md).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        )
        .into()
}

struct JsonColors {
    muted: Color,
    key: Color,
    string_val: Color,
    number_val: Color,
    keyword_val: Color,
}

impl JsonColors {
    fn from_palette(p: &ForgePalette) -> Self {
        Self {
            muted: p.text_muted,
            key: p.info,
            string_val: p.success,
            number_val: p.bits,
            keyword_val: p.brand,
        }
    }
}

fn colored_text_span<'a, Msg: 'a>(
    content: String,
    color: Color,
    mono: iced::Font,
) -> Element<'a, Msg> {
    iced::widget::text(content)
        .size(FONT_XS)
        .color(color)
        .font(mono)
        .into()
}

fn lines_row<'a, Msg: 'a>(segs: Vec<Element<'a, Msg>>) -> Element<'a, Msg> {
    row(segs).spacing(0).into()
}

fn push_indent_seg<'a, Msg: 'a>(
    segs: &mut Vec<Element<'a, Msg>>,
    indent: usize,
    colors: &JsonColors,
    mono: iced::Font,
) {
    if indent > 0 {
        segs.push(colored_text_span("  ".repeat(indent), colors.muted, mono));
    }
}

fn push_key_segs<'a, Msg: 'a>(
    segs: &mut Vec<Element<'a, Msg>>,
    key: Option<&str>,
    colors: &JsonColors,
    mono: iced::Font,
) {
    if let Some(k) = key {
        segs.push(colored_text_span(format!(r#""{k}""#), colors.key, mono));
        segs.push(colored_text_span(": ".to_string(), colors.muted, mono));
    }
}

const JSON_VIEWER_MAX_DEPTH: usize = 32;

fn push_lines<'a, Msg: 'a>(
    out: &mut Vec<Element<'a, Msg>>,
    value: &serde_json::Value,
    indent: usize,
    key: Option<&str>,
    trailing_comma: bool,
    colors: &JsonColors,
    mono: iced::Font,
) {
    use serde_json::Value;

    if indent >= JSON_VIEWER_MAX_DEPTH {
        let mut segs = Vec::new();
        push_indent_seg(&mut segs, indent, colors, mono);
        push_key_segs(&mut segs, key, colors, mono);
        let trail = if trailing_comma {
            "\"...\","
        } else {
            "\"...\""
        };
        segs.push(colored_text_span(trail.to_string(), colors.muted, mono));
        out.push(lines_row(segs));
        return;
    }

    match value {
        Value::Object(map) => {
            let mut segs = Vec::new();
            push_indent_seg(&mut segs, indent, colors, mono);
            push_key_segs(&mut segs, key, colors, mono);
            segs.push(colored_text_span("{".to_string(), colors.muted, mono));
            out.push(lines_row(segs));

            let len = map.len();
            for (i, (k, v)) in map.iter().enumerate() {
                push_lines(out, v, indent + 1, Some(k), i + 1 < len, colors, mono);
            }

            let closing = if trailing_comma { "}," } else { "}" };
            let mut segs = Vec::new();
            push_indent_seg(&mut segs, indent, colors, mono);
            segs.push(colored_text_span(closing.to_string(), colors.muted, mono));
            out.push(lines_row(segs));
        }

        Value::Array(arr) => {
            let mut segs = Vec::new();
            push_indent_seg(&mut segs, indent, colors, mono);
            push_key_segs(&mut segs, key, colors, mono);
            segs.push(colored_text_span("[".to_string(), colors.muted, mono));
            out.push(lines_row(segs));

            let len = arr.len();
            for (i, v) in arr.iter().enumerate() {
                push_lines(out, v, indent + 1, None, i + 1 < len, colors, mono);
            }

            let closing = if trailing_comma { "]," } else { "]" };
            let mut segs = Vec::new();
            push_indent_seg(&mut segs, indent, colors, mono);
            segs.push(colored_text_span(closing.to_string(), colors.muted, mono));
            out.push(lines_row(segs));
        }

        Value::String(_) => {
            let json_repr = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
            let mut segs = Vec::new();
            push_indent_seg(&mut segs, indent, colors, mono);
            push_key_segs(&mut segs, key, colors, mono);
            segs.push(colored_text_span(json_repr, colors.string_val, mono));
            if trailing_comma {
                segs.push(colored_text_span(",".to_string(), colors.muted, mono));
            }
            out.push(lines_row(segs));
        }

        Value::Number(n) => {
            let mut segs = Vec::new();
            push_indent_seg(&mut segs, indent, colors, mono);
            push_key_segs(&mut segs, key, colors, mono);
            segs.push(colored_text_span(n.to_string(), colors.number_val, mono));
            if trailing_comma {
                segs.push(colored_text_span(",".to_string(), colors.muted, mono));
            }
            out.push(lines_row(segs));
        }

        Value::Bool(b) => {
            let kw = if *b { "true" } else { "false" };
            let mut segs = Vec::new();
            push_indent_seg(&mut segs, indent, colors, mono);
            push_key_segs(&mut segs, key, colors, mono);
            segs.push(colored_text_span(kw.to_string(), colors.keyword_val, mono));
            if trailing_comma {
                segs.push(colored_text_span(",".to_string(), colors.muted, mono));
            }
            out.push(lines_row(segs));
        }

        Value::Null => {
            let mut segs = Vec::new();
            push_indent_seg(&mut segs, indent, colors, mono);
            push_key_segs(&mut segs, key, colors, mono);
            segs.push(colored_text_span(
                "null".to_string(),
                colors.keyword_val,
                mono,
            ));
            if trailing_comma {
                segs.push(colored_text_span(",".to_string(), colors.muted, mono));
            }
            out.push(lines_row(segs));
        }
    }
}

pub fn json_viewer<'a, Msg: 'a>(
    value: &'a serde_json::Value,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let mono = font(FontRole::Monospace);
    let colors = JsonColors::from_palette(palette);

    let mut lines: Vec<Element<'a, Msg>> = Vec::new();
    push_lines(&mut lines, value, 0, None, false, &colors, mono);

    let content = column(lines).spacing(0);

    let base = palette.base;
    let border_color = palette.border_regular;

    container(scrollable(content).height(Length::Shrink))
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(base)),
            border: Border {
                color: border_color,
                width: 0.5,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// `loading = true` renders a disabled, muted, spinner-iconed button — the
/// busy-guard so a double-click on Replay cannot fire two concurrent replays
/// (the button simply has no `on_press` attached while a replay is in flight).
pub fn replay_button<'a, Msg: Clone + 'a>(
    on_click: Msg,
    loading: bool,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let brand = palette.brand;
    let border_color = palette.border_regular;
    let content_color = if loading { palette.text_faint } else { brand };
    let hover_bg = Color {
        r: brand.r,
        g: brand.g,
        b: brand.b,
        a: 0.08,
    };

    let icon = if loading {
        tabler_icon(Icon::Loader2, FONT_SM, content_color)
    } else {
        tabler_icon(Icon::Repeat, FONT_SM, content_color)
    };

    let label_key = if loading {
        "widget.event.replaying"
    } else {
        "widget.event.replay"
    };
    let label = iced::widget::text(crate::tr!(label_key))
        .size(FONT_XS)
        .color(content_color);

    let content = container(
        row![icon, label]
            .spacing(6)
            .align_y(iced::Alignment::Center),
    )
    .center_x(Length::Fill);

    let btn = button(content)
        .padding(Padding {
            top: spf(Spacing::Xs),
            right: spf(Spacing::Sm),
            bottom: spf(Spacing::Xs),
            left: spf(Spacing::Sm),
        })
        .width(Length::Fill)
        .style(
            move |_theme: &iced::Theme, status: button::Status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed if !loading => {
                        Some(Background::Color(hover_bg))
                    }
                    _ => None,
                },
                text_color: content_color,
                border: Border {
                    color: border_color,
                    width: 0.5,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        );

    if loading {
        btn.into()
    } else {
        btn.on_press(on_click).into()
    }
}

pub struct EventInspectorParams<'a, Msg> {
    pub source: EventSource,
    pub event_type: &'a str,
    pub timestamp: &'a str,
    pub event_id: &'a str,
    pub payload: &'a serde_json::Value,
    /// The event that caused the selected one (resolved by walking `caused_by`):
    /// `(summary, kind, on_click)`. `on_click` walks one hop up the causation chain.
    pub caused_event: Option<(&'a str, &'a str, Msg)>,
    pub on_replay: Msg,
    /// Busy-guard: `true` while a replay is in flight — renders the Replay
    /// button disabled/spinner so a double-click cannot fire a second replay.
    pub replay_loading: bool,
}

pub fn event_inspector<'a, Msg: Clone + 'a>(
    params: EventInspectorParams<'a, Msg>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let mono = font(FontRole::Monospace);
    let elevated = palette.elevated;
    let border_color = palette.border_regular;
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;
    let text_faint = palette.text_faint;

    let badge = source_badge(params.source, palette);

    let type_label = iced::widget::text(params.event_type)
        .size(FONT_XS)
        .color(text_primary)
        .font(mono);

    let header_top = row![badge, type_label]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    let secondary = iced::widget::text(format!("{} · #{}", params.timestamp, params.event_id))
        .size(FONT_XS)
        .color(text_muted)
        .font(mono);

    let header_card = container(column![header_top, secondary].spacing(6))
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_color,
                width: 0.5,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    let payload_label = iced::widget::text(crate::tr!("widget.event.payload_header"))
        .size(FONT_XS)
        .color(text_faint)
        .font(mono);

    let viewer = json_viewer(params.payload, palette);

    let mut col = column![header_card, payload_label, viewer].spacing(8);

    if let Some((summary, kind, on_click)) = params.caused_event {
        let caused_label = iced::widget::text(crate::tr!("widget.event.caused_header"))
            .size(FONT_XS)
            .color(text_faint)
            .font(mono);
        col = col
            .push(Space::new().height(4))
            .push(caused_label)
            .push(causation_chip(summary, kind, on_click, palette));
    }

    col.push(Space::new().height(2))
        .push(replay_button(
            params.on_replay,
            params.replay_loading,
            palette,
        ))
        .into()
}
