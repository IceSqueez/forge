use super::GlobalsView;
use crate::automation::globals::{Global, variant_kind_color};
use forge_components::{
    ActivePresentation, BORDER_THIN, ConfirmTone, Density, FONT_XS, ForgePalette, Icon, Language,
    ModalSize, OverflowRevealingFocus, OverlayPosition, Radius, Spacing, SyntaxHighlights,
    confirm_modal, modal, overlay, primary_button_with_icon, radius, scaled, secondary_button,
    spacing, syntax_highlights, tr,
};
use forge_types::Variant;
use gpui::prelude::*;
use gpui::{ClickEvent, Context, SharedString, StyledText, div, px};

const GUTTER_W: gpui::Pixels = px(34.0);
const PANEL_MAX_H: gpui::Pixels = px(400.0);
const GUTTER_PAD_R: gpui::Pixels = px(12.0);
const LISTING_PAD_H: gpui::Pixels = px(12.0);

struct InspectLine {
    text: SharedString,
    highlights: SyntaxHighlights,
}

pub(super) struct InspectedGlobal {
    global: Global,
    json_lines: Vec<SharedString>,
    painted: Option<(ForgePalette, Vec<InspectLine>)>,
}

impl InspectedGlobal {
    pub(super) fn new(global: Global) -> Self {
        let json = serde_json::to_string_pretty(&global.value.to_plain_json()).unwrap_or_default();
        Self {
            json_lines: json.lines().map(|line| line.to_owned().into()).collect(),
            global,
            painted: None,
        }
    }

    pub(super) fn paint(&mut self, palette: &ForgePalette) {
        if self
            .painted
            .as_ref()
            .is_some_and(|(held, _)| held == palette)
        {
            return;
        }
        let lines = self
            .json_lines
            .iter()
            .map(|line| InspectLine {
                text: line.clone(),
                highlights: syntax_highlights(Language::Json, line, palette),
            })
            .collect();
        self.painted = Some((*palette, lines));
    }
}

impl GlobalsView {
    pub(super) fn render_delete_confirm(
        &self,
        name: SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            tr!("globals_delete_confirm_title"),
            tr!("globals_delete_confirm_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "globals-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "globals-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("globals-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
    }

    pub(super) fn render_inspect(
        &self,
        inspected: &InspectedGlobal,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let density = cx.density();
        let g = &inspected.global;
        let kind = g.kind();
        let (count_key, count) = inspect_count(&g.value);
        let subtitle = tr!(
            count_key,
            kind = kind.contract_name(),
            count = i64::try_from(count).unwrap_or(i64::MAX)
        );

        let listing = inspect_listing(inspected, palette, density, cx);
        let code = div()
            .w_full()
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .rounded(radius(Radius::Md))
            .py(spacing(Spacing::Sm, density))
            .px(scaled(LISTING_PAD_H, density))
            .child(listing);
        let body = div()
            .id("globals-inspect-scroll")
            .w_full()
            .max_h(PANEL_MAX_H)
            .min_h_0()
            .overflow_y_revealing_focus()
            .child(code);

        let footer = self.inspect_footer(g, palette, density, cx);

        let card = modal(g.name.clone(), body, palette)
            .subtitle(subtitle)
            .header_icon(Icon::Code, variant_kind_color(kind, palette))
            .size(ModalSize::Md)
            .body_scrolls_itself()
            .footer(footer)
            .on_close(
                "globals-inspect-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_inspect(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("globals-inspect-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.close_inspect(cx));
            })
            .into_any_element()
    }

    fn inspect_footer(
        &self,
        g: &Global,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let hint = div()
            .font_family(cx.body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .child(tr!("globals_inspect_snapshot"));
        let close = secondary_button(tr!("globals_inspect_close"), palette).on_click(
            "globals-inspect-close-btn",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close_inspect(cx)),
        );
        let edit_name = g.name.clone();
        let edit = primary_button_with_icon(Icon::Edit, tr!("globals_inspect_edit"), palette)
            .on_click(
                "globals-inspect-edit-btn",
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.inspecting = None;
                    this.open_edit(edit_name.clone(), window, cx);
                }),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(hint)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(close)
                    .child(edit),
            )
    }
}

fn inspect_listing(
    inspected: &InspectedGlobal,
    palette: &ForgePalette,
    density: Density,
    cx: &gpui::App,
) -> gpui::Div {
    let mut listing = div().flex().flex_col();
    let painted = inspected.painted.iter().flat_map(|(_, lines)| lines);
    for (i, line) in painted.enumerate() {
        let mut code_line = div().flex_1().flex();
        if line.text.is_empty() {
            code_line = code_line.child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XS)
                    .child("\u{00A0}"),
            );
        } else {
            code_line = code_line.child(
                div()
                    .flex_none()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XS)
                    .whitespace_nowrap()
                    .child(
                        StyledText::new(line.text.clone()).with_highlights(line.highlights.clone()),
                    ),
            );
        }
        listing = listing.child(
            div()
                .flex()
                .child(
                    div()
                        .flex_none()
                        .w(GUTTER_W)
                        .pr(scaled(GUTTER_PAD_R, density))
                        .flex()
                        .justify_end()
                        .font_family(cx.mono_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child(format!("{}", i + 1)),
                )
                .child(code_line),
        );
    }
    listing
}

pub(super) fn inspect_count(value: &Variant) -> (&'static str, usize) {
    match value {
        Variant::Array(items) => ("globals_inspect_subtitle_items", items.len()),
        Variant::Object(map) => ("globals_inspect_subtitle_keys", map.len()),
        _ => ("globals_inspect_subtitle_keys", 0),
    }
}
