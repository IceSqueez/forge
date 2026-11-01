use super::{
    EventFeedView, INSPECTOR_MAX, INSPECTOR_MIN, RUN_ACTION_ICON, replay_badge, source_color,
    source_label,
};
use crate::audience::event_log::EventItem;
use forge_components::{
    ActivePresentation, BORDER_THIN, DateShape, Density, FONT_XS, FONT_XXS, ForgePalette, Icon,
    KeyActivated, Radius, SheetWidth, Spacing, badge, fmt_display, icon, radius, spacing, tr,
};
use gpui::{App, ClickEvent, Context, Pixels, Rgba, div, prelude::*, px};

impl EventFeedView {
    pub(super) fn render_inspector(
        &self,
        item: &EventItem,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let content = self.inspector_content(item, palette, cx);
        let width = self.inspector_width.clamp(INSPECTOR_MIN, INSPECTOR_MAX);

        forge_components::side_sheet(px(width), content, palette)
            .header(tr!("event_feed_inspector_title"))
            .header_icon(Icon::Pin, palette.brand)
            .on_resize(
                "event-inspector-resize",
                SheetWidth::new(width, INSPECTOR_MIN, INSPECTOR_MAX),
                cx.listener(|this, width: &Pixels, _, cx| this.set_inspector_width(width, cx)),
            )
    }

    fn inspector_content(
        &self,
        item: &EventItem,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        let summary_card = Self::inspector_summary_card(item, palette, density, cx);
        let payload = self.payload_block(item, palette, density, cx);
        let caused = Self::inspector_caused_row(self.caused_text(item, cx), palette, density, cx);
        let replay = self.inspector_replay_button(palette, density, cx);

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .gap(spacing(Spacing::Sm, density))
            .child(summary_card)
            .child(Self::section_label(
                tr!("widget_event_payload_header"),
                palette,
                cx,
            ))
            .child(payload)
            .child(Self::section_label(
                tr!("widget_event_caused_header"),
                palette,
                cx,
            ))
            .child(caused)
            .child(replay)
    }

    fn inspector_summary_card(
        item: &EventItem,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> impl IntoElement + use<> {
        let id_str = item.id.as_ref();
        let last6 = &id_str[id_str.len().saturating_sub(6)..];

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .gap(spacing(Spacing::Xs, density))
                    .child(badge(
                        palette.surface_overlay,
                        source_color(item.source, palette),
                        source_label(item.source),
                        true,
                        FONT_XXS,
                    ))
                    .children(item.replay.then(|| replay_badge(palette, FONT_XXS)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(cx.mono_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(item.kind.clone()),
                    ),
            )
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(format!(
                        "{} · #{last6}",
                        fmt_display(DateShape::TimeWithMillis, item.timestamp)
                    )),
            )
    }

    fn caused_text(&self, item: &EventItem, cx: &App) -> gpui::SharedString {
        item.caused_by
            .as_ref()
            .and_then(|cid| {
                let log = self.log.read(cx);
                log.items().iter().find(|e| &e.id == cid).map(|e| {
                    if e.summary.is_empty() {
                        e.kind.clone()
                    } else {
                        e.summary.clone()
                    }
                })
            })
            .unwrap_or_else(|| gpui::SharedString::from("-"))
    }

    fn inspector_caused_row(
        caused_text: gpui::SharedString,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> impl IntoElement + use<> {
        div()
            .w_full()
            .flex()
            .items_center()
            .overflow_hidden()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Md))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::Bolt, RUN_ACTION_ICON, palette.brand))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(caused_text),
            )
    }

    fn inspector_replay_button(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let replaying = self.replaying;
        let replay_color = if replaying {
            palette.disabled
        } else {
            palette.brand
        };
        let mut replay = div()
            .id("event-inspector-replay")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(if replaying {
                palette.disabled
            } else {
                palette.border_regular
            });
        replay = if replaying {
            replay.cursor_default()
        } else {
            replay
                .cursor_pointer()
                .key_activated(palette.text_primary)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.replay(cx)))
        };
        replay
            .child(icon(Icon::Repeat, FONT_XS, replay_color))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(replay_color)
                    .child(tr!("widget_event_replay")),
            )
    }

    fn section_label(text: String, palette: &ForgePalette, cx: &App) -> impl IntoElement + use<> {
        div()
            .font_family(cx.mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(text)
    }

    fn payload_block(
        &self,
        item: &EventItem,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> impl IntoElement + use<> {
        let brace = palette.text_muted;
        let mut lines: Vec<gpui::AnyElement> = Vec::new();
        lines.push(json_line(0, vec![json_span("{".to_owned(), brace)]));
        push_json_lines(
            &serde_json::json!(item.kind.as_ref()),
            1,
            Some("event"),
            true,
            palette,
            &mut lines,
        );
        push_json_lines(
            &serde_json::json!(item.source.as_str()),
            1,
            Some("source"),
            true,
            palette,
            &mut lines,
        );
        push_json_lines(
            &serde_json::json!(fmt_display(DateShape::TimeWithMillis, item.timestamp)),
            1,
            Some("timestamp"),
            true,
            palette,
            &mut lines,
        );
        if let Some(caused_by) = &item.caused_by {
            push_json_lines(
                &serde_json::json!(caused_by.as_ref()),
                1,
                Some("caused_by"),
                true,
                palette,
                &mut lines,
            );
        }
        push_json_lines(
            &item.payload,
            1,
            Some("payload"),
            false,
            palette,
            &mut lines,
        );
        lines.push(json_line(0, vec![json_span("}".to_owned(), brace)]));

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family(cx.mono_family())
            .text_size(FONT_XXS)
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .bg(palette.base)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .children(lines)
    }
}

const JSON_INDENT_PX: f32 = 10.0;

fn json_line(indent: usize, children: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    div()
        .flex()
        .pl(px(indent as f32 * JSON_INDENT_PX))
        .children(children)
        .into_any_element()
}

fn json_span(text: String, color: Rgba) -> gpui::AnyElement {
    div().text_color(color).child(text).into_any_element()
}

fn json_scalar_color(value: &serde_json::Value, palette: &ForgePalette) -> Rgba {
    match value {
        serde_json::Value::String(_) => palette.success,
        serde_json::Value::Number(_) => palette.warning,
        _ => palette.text_muted,
    }
}

fn push_json_lines(
    value: &serde_json::Value,
    indent: usize,
    key: Option<&str>,
    trailing_comma: bool,
    palette: &ForgePalette,
    out: &mut Vec<gpui::AnyElement>,
) {
    let brace = palette.text_muted;
    let key_color = palette.info;
    let comma = if trailing_comma { "," } else { "" };
    let prefix = |open: &str| {
        let mut spans = Vec::new();
        if let Some(k) = key {
            spans.push(json_span(format!("\"{k}\""), key_color));
            spans.push(json_span(": ".to_owned(), brace));
        }
        spans.push(json_span(open.to_owned(), brace));
        spans
    };

    match value {
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                out.push(json_line(indent, prefix(&format!("{{}}{comma}"))));
                return;
            }
            out.push(json_line(indent, prefix("{")));
            for (i, (k, v)) in map.iter().enumerate() {
                let last = i + 1 == map.len();
                push_json_lines(v, indent + 1, Some(k), !last, palette, out);
            }
            out.push(json_line(
                indent,
                vec![json_span(format!("}}{comma}"), brace)],
            ));
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                out.push(json_line(indent, prefix(&format!("[]{comma}"))));
                return;
            }
            out.push(json_line(indent, prefix("[")));
            for (i, v) in items.iter().enumerate() {
                let last = i + 1 == items.len();
                push_json_lines(v, indent + 1, None, !last, palette, out);
            }
            out.push(json_line(
                indent,
                vec![json_span(format!("]{comma}"), brace)],
            ));
        }
        scalar => {
            let mut spans = Vec::new();
            if let Some(k) = key {
                spans.push(json_span(format!("\"{k}\""), key_color));
                spans.push(json_span(": ".to_owned(), brace));
            }
            spans.push(json_span(
                scalar.to_string(),
                json_scalar_color(scalar, palette),
            ));
            if trailing_comma {
                spans.push(json_span(",".to_owned(), brace));
            }
            out.push(json_line(indent, spans));
        }
    }
}
