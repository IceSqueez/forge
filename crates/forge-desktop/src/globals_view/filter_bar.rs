use super::GlobalsView;
use crate::globals::GlobalsFilter;
use forge_components::{
    ActivePresentation, ChipGlyph, Density, ForgePalette, Icon, KeyActivated, Radius, Spacing,
    chip, header_stat, header_stats, icon, primary_button_with_icon, radius, spacing, tr,
};
use gpui::prelude::*;
use gpui::{ClickEvent, Context, div, px};

const ICON_ROW: gpui::Pixels = px(14.0);
const SEARCH_W: gpui::Pixels = px(200.0);
const EXPORT_BUTTON_PAD: gpui::Pixels = px(5.0);

impl GlobalsView {
    pub(super) fn render_stats<'a>(
        &self,
        palette: &'a ForgePalette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<'a> {
        let density = cx.density();
        let globals = self.globals.read(cx);

        header_stats(
            vec![
                header_stat(
                    globals.total().to_string(),
                    palette.text_primary,
                    tr!("globals_stat_total"),
                ),
                header_stat(
                    globals.persisted_count().to_string(),
                    palette.success,
                    tr!("globals_stat_persisted"),
                ),
                header_stat(
                    globals.session_count().to_string(),
                    palette.warning,
                    tr!("globals_stat_in_memory"),
                ),
            ],
            palette,
            density,
            cx,
        )
    }

    pub(super) fn render_filter_left(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let chips = [
            (
                "globals-filter-all",
                tr!("globals_filter_all"),
                GlobalsFilter::All,
                palette.brand,
            ),
            (
                "globals-filter-persisted",
                tr!("globals_filter_persisted"),
                GlobalsFilter::Persisted,
                palette.success,
            ),
            (
                "globals-filter-session",
                tr!("globals_filter_session"),
                GlobalsFilter::Session,
                palette.warning,
            ),
        ];
        let mut chip_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (id, label, filter, dot) in chips {
            let active = self.filter == filter;
            chip_row = chip_row.child(chip(label, ChipGlyph::Dot(dot), active, palette).on_click(
                id,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_filter(filter, cx)),
            ));
        }

        let search = div().w(SEARCH_W).child(self.search.field().clone());

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(search)
            .child(chip_row)
    }

    pub(super) fn render_filter_right(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let export_hover = palette.surface_overlay;
        let export = div()
            .id("globals-export")
            .flex()
            .items_center()
            .justify_center()
            .p(EXPORT_BUTTON_PAD)
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(export_hover))
            .child(icon(Icon::Download, ICON_ROW, palette.success))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export(cx)));
        let new_btn =
            primary_button_with_icon(Icon::Plus, tr!("globals_editor_title_create"), palette)
                .density(density)
                .on_click(
                    "globals-new",
                    cx.listener(|this, _: &ClickEvent, window, cx| this.open_create(window, cx)),
                );

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(export)
            .child(new_btn)
    }
}
