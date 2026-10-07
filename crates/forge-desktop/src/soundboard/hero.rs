use std::sync::Arc;

use forge_components::{
    BORDER_THIN, Density, FONT_XS, ForgePalette, Icon, Radius, Spacing, body_family, icon, radius,
    spacing, toggle, tr,
};
use forge_storage::set_soundboard_enabled;
use gpui::{AnyElement, ClickEvent, Context, Pixels, div, prelude::*, px};

use super::routing::ROUTING_PAD;
use super::{LABEL_FS, SoundboardView};
use crate::async_bridge::{self, ErrorSink};

const HERO_ICON_TILE: Pixels = px(40.0);
const HERO_ICON_TILE_RADIUS: Pixels = px(10.0);
const HERO_GLYPH: Pixels = px(20.0);
const HERO_TITLE_FS: Pixels = px(15.0);
const HERO_GAP: Pixels = px(14.0);
const HEADER_ICON: Pixels = px(13.0);

impl SoundboardView {
    fn toggle_enabled(&mut self, cx: &mut Context<Self>) {
        self.settings = self
            .player
            .update_settings(|settings| settings.enabled = !settings.enabled);
        let repo = Arc::clone(&self.settings_repo);
        let value = self.settings.enabled;
        async_bridge::report_failure(
            &self.rt_handle,
            async move { set_soundboard_enabled(repo.as_ref(), value).await },
            ErrorSink::Toast,
            tr!("soundboard_persist_failed"),
            cx,
        );
        cx.notify();
    }

    pub(super) fn render_header_right(&self, palette: &ForgePalette) -> AnyElement {
        let device = self.device_short_label();
        let summary = tr!(
            "soundboard_header_summary",
            device = device.as_str(),
            count = self.clips.len() as i64
        );
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .child(icon(Icon::Volume, HEADER_ICON, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(LABEL_FS)
                    .text_color(palette.text_muted)
                    .child(summary),
            )
            .into_any_element()
    }

    pub(super) fn render_hero(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = self.settings.enabled;
        let label_color = if enabled {
            palette.success
        } else {
            palette.text_faint
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(HERO_GAP)
            .py(ROUTING_PAD)
            .px(spacing(Spacing::Lg, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(
                div()
                    .flex_shrink_0()
                    .size(HERO_ICON_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(HERO_ICON_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::Music, HERO_GLYPH, palette.bits)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(HERO_TITLE_FS)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(palette.text_primary)
                            .child(tr!("soundboard_hero_title")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("soundboard_hero_blurb")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Sm, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(LABEL_FS)
                            .text_color(label_color)
                            .child(if enabled {
                                tr!("soundboard_hero_enabled")
                            } else {
                                tr!("soundboard_hero_disabled")
                            }),
                    )
                    .child(toggle(enabled, palette).on_click(
                        "sb-enabled",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enabled(cx)),
                    )),
            )
            .into_any_element()
    }
}
