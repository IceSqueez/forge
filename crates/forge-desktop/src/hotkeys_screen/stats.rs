use forge_components::{
    FONT_SM, FONT_XXS, ForgePalette, body_family, card, fmt_relative_time, mono_family, tr,
};
use gpui::{AnyElement, Pixels, SharedString, div, prelude::*, px};

use super::HotkeysScreenView;

const STAT_GAP: Pixels = px(10.0);
const STAT_MARGIN_B: Pixels = px(14.0);
const STAT_PAD_V: Pixels = px(10.0);
const STAT_PAD_H: Pixels = px(12.0);
const STAT_LABEL_MB: Pixels = px(4.0);
const STAT_HINT_MT: Pixels = px(2.0);

const NO_VALUE: &str = "-";

impl HotkeysScreenView {
    pub(super) fn render_stats(&self, palette: &ForgePalette) -> AnyElement {
        let active = self.active_count();
        let conflicts = self.conflicts;
        let (conflicts_ink, conflicts_hint, conflicts_hint_ink) = if conflicts == 0 {
            (
                palette.success,
                tr!("hotkeys_stat_conflicts_none"),
                palette.success,
            )
        } else {
            (
                palette.warning,
                tr!("hotkeys_stat_conflicts_hint"),
                palette.text_faint,
            )
        };
        let (fired_value, fired_hint) = match &self.last_fired {
            Some(last) => (last.combo.clone(), fmt_relative_time(Some(last.at))),
            None => (NO_VALUE.to_owned(), tr!("hotkeys_stat_last_fired_none")),
        };

        div()
            .w_full()
            .flex()
            .items_stretch()
            .gap(STAT_GAP)
            .mb(STAT_MARGIN_B)
            .child(stat_card(
                tr!("hotkeys_stat_bindings"),
                self.total_count().to_string(),
                palette.text_primary,
                tr!("hotkeys_stat_bindings_hint", count = active as i64),
                palette.success,
                palette,
            ))
            .child(stat_card(
                tr!("hotkeys_stat_global"),
                self.global_count().to_string(),
                palette.text_primary,
                tr!("hotkeys_stat_global_hint"),
                palette.text_faint,
                palette,
            ))
            .child(stat_card(
                tr!("hotkeys_stat_conflicts"),
                conflicts.to_string(),
                conflicts_ink,
                conflicts_hint,
                conflicts_hint_ink,
                palette,
            ))
            .child(stat_card(
                tr!("hotkeys_stat_last_fired"),
                fired_value,
                palette.text_primary,
                fired_hint,
                palette.text_faint,
                palette,
            ))
            .into_any_element()
    }
}

fn stat_card(
    label: String,
    value: String,
    value_color: gpui::Rgba,
    hint: String,
    hint_color: gpui::Rgba,
    palette: &ForgePalette,
) -> impl IntoElement {
    let body = div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .mb(STAT_LABEL_MB)
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(SharedString::from(label.to_uppercase())),
        )
        .child(
            div()
                .w_full()
                .truncate()
                .font_family(body_family())
                .text_size(FONT_SM)
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(value_color)
                .child(value),
        )
        .child(
            div()
                .mt(STAT_HINT_MT)
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(hint_color)
                .child(hint),
        );

    let mut cell = div().min_w(px(0.0)).child(
        card(body, palette)
            .padding_xy(STAT_PAD_V, STAT_PAD_H)
            .full_width()
            .full_height(),
    );
    let style = cell.style();
    style.flex_grow = Some(1.0);
    style.flex_basis = Some(gpui::relative(0.0).into());
    cell
}
