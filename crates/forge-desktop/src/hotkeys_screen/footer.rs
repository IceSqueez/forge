use forge_components::Density;
use forge_components::FONT_XXS;
use forge_components::Spacing;
use forge_components::spacing;
use forge_components::{
    BORDER_THIN, ForgePalette, fmt_display_relative, mono_family, scaled, status_dot, tr,
};
use gpui::{AnyElement, Pixels, div, prelude::*, px};

use super::HotkeysScreenView;

const FOOTER_FS: Pixels = FONT_XXS;
const FOOTER_DOT: Pixels = px(6.0);
const FOOTER_PAD_V: Pixels = px(7.0);
const FOOTER_PAD_H: Pixels = px(14.0);
const FOOTER_MT: Pixels = px(14.0);
const FOOTER_SEPARATOR: &str = "·";

impl HotkeysScreenView {
    pub(super) fn render_footer(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let listener = if self.enabled {
            tr!("hotkeys_footer_listening")
        } else {
            tr!("hotkeys_footer_stopped")
        };
        let (dot, right) = if self.conflicts == 0 {
            (palette.success, tr!("hotkeys_footer_no_conflicts"))
        } else {
            (
                palette.warning,
                tr!("hotkeys_footer_conflicts", count = self.conflicts as i64),
            )
        };
        let synthesized = self.last_synthesized.map(|at| {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .child(footer_text(FOOTER_SEPARATOR.to_owned(), palette))
                .child(status_dot(palette.warning, FOOTER_DOT))
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(FOOTER_FS)
                        .text_color(palette.warning)
                        .child(tr!(
                            "hotkeys_footer_hold_closed",
                            when = fmt_display_relative(Some(at))
                        )),
                )
        });

        div()
            .w_full()
            .mt(FOOTER_MT)
            .flex()
            .items_center()
            .justify_between()
            .py(scaled(FOOTER_PAD_V, density))
            .px(scaled(FOOTER_PAD_H, density))
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(footer_text(
                        tr!("hotkeys_footer_bindings", count = self.total_count() as i64),
                        palette,
                    ))
                    .child(footer_text(FOOTER_SEPARATOR.to_owned(), palette))
                    .child(footer_text(listener, palette))
                    .children(synthesized),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(status_dot(dot, FOOTER_DOT))
                    .child(footer_text(right, palette)),
            )
            .into_any_element()
    }
}

fn footer_text(text: String, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FOOTER_FS)
        .text_color(palette.text_faint)
        .child(text)
}
