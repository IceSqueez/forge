use forge_components::{
    BORDER_THIN, Density, ForgePalette, fmt_bytes, mono_family, status_dot, tr,
};
use gpui::{AnyElement, Context, Pixels, div, prelude::*, px};

use super::SoundboardView;

pub(super) const FOOTER_FS: Pixels = px(10.5);
const FOOTER_DOT: Pixels = px(6.0);
pub(super) const FOOTER_PAD_Y: Pixels = px(7.0);
pub(super) const FOOTER_PAD_X: Pixels = px(14.0);

impl SoundboardView {
    pub(super) fn render_footer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let category_count = self.categories_present().len();
        let size_label = self
            .total_size
            .map(fmt_bytes)
            .unwrap_or_else(|| "\u{2014}".to_owned());
        let ready = self.output_ready();
        let (dot, status_text) = if ready {
            (palette.success, tr!("soundboard_output_ready"))
        } else {
            (palette.warning, tr!("soundboard_output_missing"))
        };
        let counts = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(FOOTER_PAD_Y)
            .px(FOOTER_PAD_X)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FOOTER_FS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "soundboard_footer_left",
                        sounds = self.clips.len() as i64,
                        categories = category_count as i64,
                        size = size_label.as_str()
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(status_dot(dot, FOOTER_DOT))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FOOTER_FS)
                            .text_color(palette.text_faint)
                            .child(status_text),
                    ),
            );

        div()
            .w_full()
            .flex()
            .flex_col()
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .children(self.render_adoption_strip(palette, density, cx))
            .child(counts)
            .into_any_element()
    }
}
