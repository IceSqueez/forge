use forge_components::{FONT_XS, ForgePalette, Icon, body_family, card, icon, toggle, tr};
use gpui::{AnyElement, ClickEvent, Context, Pixels, div, prelude::*, px};

use super::{HotkeysScreenView, LABEL_FS};
use crate::async_bridge::{self, ErrorSink};
use crate::integration_supervisor::LifecycleState;

const HERO_PAD_V: Pixels = px(14.0);
const HERO_PAD_H: Pixels = px(18.0);
const HERO_GAP: Pixels = px(14.0);
const HERO_MARGIN_B: Pixels = px(14.0);
const HERO_TILE: Pixels = px(40.0);
const HERO_TILE_RADIUS: Pixels = px(10.0);
const HERO_GLYPH: Pixels = px(20.0);
const HERO_TITLE_FS: Pixels = px(15.0);
const HERO_BLURB_MT: Pixels = px(1.0);
const HERO_TOGGLE_GAP: Pixels = px(8.0);

impl HotkeysScreenView {
    fn toggle_engine(&mut self, cx: &mut Context<Self>) {
        let previous = self.enabled;
        self.enabled = !previous;
        let enabled = self.enabled;
        let engine = self.engine.clone();
        async_bridge::optimistic(
            &self.rt_handle,
            previous,
            async move {
                let engine = engine.ok_or_else(|| "hotkey integration unavailable".to_owned())?;
                match engine.set_enabled(enabled).await? {
                    LifecycleState::Failed(reason) => Err(reason),
                    _ => Ok(()),
                }
            },
            |this, previous, _message, cx| {
                this.enabled = previous;
                ErrorSink::Toast.report(tr!("hotkeys_toggle_failed"), cx);
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn render_hero(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let label_color = if self.enabled {
            palette.success
        } else {
            palette.text_faint
        };
        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(HERO_GAP)
            .child(
                div()
                    .flex_shrink_0()
                    .size(HERO_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(HERO_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::Keyboard, HERO_GLYPH, palette.success)),
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
                            .child(tr!("hotkeys_hero_title")),
                    )
                    .child(
                        div()
                            .mt(HERO_BLURB_MT)
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("hotkeys_hero_blurb")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(HERO_TOGGLE_GAP)
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(LABEL_FS)
                            .text_color(label_color)
                            .child(if self.enabled {
                                tr!("hotkeys_hero_enabled")
                            } else {
                                tr!("hotkeys_hero_disabled")
                            }),
                    )
                    .child(toggle(self.enabled, palette).on_click(
                        "hotkeys-enabled",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_engine(cx)),
                    )),
            );

        div()
            .w_full()
            .mb(HERO_MARGIN_B)
            .child(
                card(body, palette)
                    .padding_xy(HERO_PAD_V, HERO_PAD_H)
                    .full_width(),
            )
            .into_any_element()
    }
}
