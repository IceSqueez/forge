use forge_components::Density;
use forge_components::Spacing;
use forge_components::spacing;
use forge_components::{ForgePalette, fmt_display_relative, stat_card, tr};
use gpui::{AnyElement, Pixels, div, prelude::*, px};

use super::HotkeysScreenView;

const STAT_MARGIN_B: Pixels = px(14.0);

const NO_VALUE: &str = "-";

impl HotkeysScreenView {
    pub(super) fn render_stats(&self, palette: &ForgePalette, density: Density) -> AnyElement {
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
            Some(last) => (last.combo.clone(), fmt_display_relative(Some(last.at))),
            None => (NO_VALUE.to_owned(), tr!("hotkeys_stat_last_fired_none")),
        };

        div()
            .w_full()
            .flex()
            .items_stretch()
            .gap(spacing(Spacing::Sm, density))
            .mb(STAT_MARGIN_B)
            .child(stat_card(
                tr!("hotkeys_stat_bindings"),
                self.total_count().to_string(),
                palette.text_primary,
                tr!("hotkeys_stat_bindings_hint", count = active as i64),
                palette.success,
                palette,
                density,
            ))
            .child(stat_card(
                tr!("hotkeys_stat_global"),
                self.global_count().to_string(),
                palette.text_primary,
                tr!("hotkeys_stat_global_hint"),
                palette.text_faint,
                palette,
                density,
            ))
            .child(stat_card(
                tr!("hotkeys_stat_conflicts"),
                conflicts.to_string(),
                conflicts_ink,
                conflicts_hint,
                conflicts_hint_ink,
                palette,
                density,
            ))
            .child(stat_card(
                tr!("hotkeys_stat_last_fired"),
                fired_value,
                palette.text_primary,
                fired_hint,
                palette.text_faint,
                palette,
                density,
            ))
            .into_any_element()
    }
}
