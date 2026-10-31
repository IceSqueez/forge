use super::ScriptEditorView;
use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Spacing, badge, body_family, icon,
    mono_family, spacing, tr,
};
use forge_script::MethodDescriptor;
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, Pixels, div, px};

const GLYPH_PIN: Pixels = px(12.0);
const RIGHT_PANE_W: Pixels = px(220.0);

impl ScriptEditorView {
    pub(super) fn toggle_api_docs(&mut self, cx: &mut Context<Self>) {
        self.api_docs_open = !self.api_docs_open;
        self.reset_contract_editing();
        cx.notify();
    }

    pub(super) fn right_pane(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pb(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("script_editor_api_reference")),
            )
            .child(icon(Icon::Pin, GLYPH_PIN, palette.text_faint));

        let search = div()
            .pb(spacing(Spacing::Sm, density))
            .child(self.api_search.field().clone());

        let mut pane = div()
            .id("script-api-pane")
            .flex_none()
            .w(RIGHT_PANE_W)
            .h_full()
            .overflow_y_scroll()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .p(spacing(Spacing::Sm, density))
            .child(header)
            .child(search);

        let matches: Vec<&'static MethodDescriptor> = forge_script::catalog()
            .iter()
            .filter(|entry| {
                self.api_search.matches(entry.name)
                    || entry
                        .namespace
                        .is_some_and(|ns| self.api_search.matches(ns))
            })
            .collect();

        let mut current_ns: Option<Option<&'static str>> = None;
        for entry in &matches {
            if current_ns != Some(entry.namespace) {
                current_ns = Some(entry.namespace);
                pane = pane.child(
                    div()
                        .pt(spacing(Spacing::Sm, density))
                        .pb(spacing(Spacing::Xxs, density))
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_muted)
                        .child(entry.namespace.unwrap_or("core").to_uppercase()),
                );
            }
            pane = pane.child(api_fn_row(entry, palette, density));
        }

        if matches.is_empty() {
            pane = pane.child(
                div()
                    .pt(spacing(Spacing::Sm, density))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("script_editor_api_no_matches")),
            );
        }

        pane.into_any_element()
    }
}

fn api_fn_row(
    entry: &MethodDescriptor,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let params = entry
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let sig = format!("{}({}) -> {}", entry.name, params, entry.return_type);

    let mut row = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xxs, density))
        .child(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .child(badge(palette.brand, palette.shell, "fn", true, FONT_XXS))
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_primary)
                        .child(sig),
                ),
        );

    if let Some(doc) = entry.doc {
        row = row.child(
            div()
                .pl(spacing(Spacing::Lg, density))
                .font_family(body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(doc),
        );
    }
    row
}
