use super::MidiScreenView;
use super::bridges::monitor_value_label;
use crate::midi_signal::{kind_color, short_kind_label};
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_HERO, FONT_LABEL, FONT_ROW, FONT_SECTION_LABEL,
    FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, SectionLabel, Spacing, badge, body_family, card,
    error_row_with_retry, ghost_button_with_icon, icon, mono_family, radius, scaled, spacing,
    toggle, tr,
};
use gpui::{AnyElement, ClickEvent, Context, Pixels, div, prelude::*, px};

const UNKNOWN_VALUE: &str = "-";
const HERO_PAD_V: Pixels = px(14.0);
const HERO_PAD_H: Pixels = px(18.0);
const HERO_GAP: Pixels = px(14.0);
const HERO_MARGIN_B: Pixels = px(14.0);
const HERO_TILE: Pixels = px(40.0);
const HERO_TILE_RADIUS: Pixels = px(10.0);
const HERO_GLYPH: Pixels = px(20.0);
const HERO_TITLE_FS: Pixels = FONT_HERO;
const HERO_BLURB_MT: Pixels = px(1.0);
const HERO_TOGGLE_GAP: Pixels = px(8.0);
const LABEL_FS: Pixels = FONT_LABEL;
const DEVICE_PAD_H: Pixels = px(12.0);
const DEVICE_GLYPH: Pixels = px(14.0);
const DEVICE_NAME_FS: Pixels = FONT_ROW;
const DEVICE_BADGE_FS: Pixels = FONT_SECTION_LABEL;
const OFFLINE_OPACITY: f32 = 0.6;
const RESCAN_MT: Pixels = px(2.0);
const MONITOR_MT: Pixels = px(12.0);
const MONITOR_PAD: Pixels = px(12.0);
const MONITOR_FS: Pixels = FONT_XXS;
const MONITOR_LINE_H: Pixels = px(18.9);
const MONITOR_GAP: Pixels = px(8.0);
const MONITOR_KIND_W: Pixels = px(54.0);

impl MidiScreenView {
    pub(super) fn render_hero(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let label_color = if self.enabled {
            palette.success
        } else {
            palette.text_faint
        };
        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(HERO_GAP, density))
            .child(
                div()
                    .flex_shrink_0()
                    .size(HERO_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(HERO_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::Piano, HERO_GLYPH, palette.info)),
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
                            .child("MIDI"),
                    )
                    .child(
                        div()
                            .mt(HERO_BLURB_MT)
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("midi_hero_blurb")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(HERO_TOGGLE_GAP, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(LABEL_FS)
                            .text_color(label_color)
                            .child(if self.enabled {
                                tr!("midi_hero_enabled")
                            } else {
                                tr!("midi_hero_disabled")
                            }),
                    )
                    .child(toggle(self.enabled, palette).on_click(
                        "midi-enabled",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enabled(cx)),
                    )),
            );

        div()
            .w_full()
            .mb(scaled(HERO_MARGIN_B, density))
            .child(
                card(body, palette)
                    .padding_xy(scaled(HERO_PAD_V, density), scaled(HERO_PAD_H, density))
                    .full_width(),
            )
            .into_any_element()
    }

    pub(super) fn render_devices(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let known = self.known();
        let mut cards: Vec<AnyElement> = Vec::with_capacity(known.len());
        for (index, name) in known.iter().enumerate() {
            let live_index = self.live_ports.iter().position(|port| port == name);
            cards.push(self.render_device(index, name, live_index, palette, cx));
        }

        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        if let Some(reason) = self.known_devices.failure() {
            list = list.child(error_row_with_retry(
                "midi-devices-retry",
                tr!("midi_devices_load_failed", reason = reason),
                palette,
                cx.listener(|this, _: &ClickEvent, _, cx| this.load_known_devices(cx)),
                density,
            ));
        } else if cards.is_empty() {
            let note = if self.known_devices.is_loaded() {
                tr!("midi_devices_empty")
            } else {
                UNKNOWN_VALUE.to_owned()
            };
            list = list.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(note),
            );
        } else {
            list = list.children(cards);
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("midi_section_devices"), palette)
                    .row()
                    .trailing(None),
            )
            .child(
                list.child(
                    div().w_full().mt(RESCAN_MT).child(
                        ghost_button_with_icon(Icon::Refresh, tr!("midi_rescan_ports"), palette)
                            .full_width()
                            .on_click(
                                "midi-rescan",
                                cx.listener(|this, _: &ClickEvent, _, cx| this.rescan(cx)),
                            ),
                    ),
                ),
            )
            .into_any_element()
    }

    fn render_device(
        &self,
        index: usize,
        name: &str,
        live_index: Option<usize>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let online = live_index.is_some();
        let selected = self.selected_device.as_deref() == Some(name);
        let glyph_color = if online {
            palette.success
        } else {
            palette.text_faint
        };
        let badge_ink = if online {
            palette.info
        } else {
            palette.text_faint
        };
        let port_line = match live_index {
            Some(index) => tr!("midi_device_port", index = index as i64),
            None => tr!("midi_device_offline"),
        };
        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(icon(Icon::Plug, DEVICE_GLYPH, glyph_color))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .font_family(body_family())
                            .text_size(DEVICE_NAME_FS)
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(name.to_owned()),
                    )
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(MONITOR_FS)
                            .text_color(palette.text_faint)
                            .child(port_line),
                    ),
            )
            .child(
                badge(
                    palette.surface_overlay,
                    badge_ink,
                    tr!(
                        "midi_device_maps",
                        count = self.maps_for_device(name) as i64
                    ),
                    true,
                    DEVICE_BADGE_FS,
                )
                .flex_none(),
            );

        let (surface, border) = if selected {
            (palette.surface_overlay, palette.brand)
        } else {
            (palette.elevated, palette.border_regular)
        };
        let hover_surface = palette.surface_overlay;
        let target = name.to_owned();
        let mut wrapper = div()
            .id(("midi-device", index))
            .w_full()
            .py(spacing(Spacing::Sm, density))
            .px(scaled(DEVICE_PAD_H, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border)
            .bg(surface)
            .text_color(palette.text_primary)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.select_device(target.clone(), cx)
            }))
            .child(row);
        if !selected {
            wrapper = wrapper.hover(move |style| style.bg(hover_surface));
        }
        if !online {
            wrapper = wrapper.opacity(OFFLINE_OPACITY);
        }
        wrapper.into_any_element()
    }

    pub(super) fn render_monitor(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let mut lines = div()
            .w_full()
            .font_family(mono_family())
            .text_size(MONITOR_FS)
            .line_height(MONITOR_LINE_H)
            .text_color(palette.text_secondary);

        for event in &self.monitor {
            lines = lines.child(
                div()
                    .w_full()
                    .flex()
                    .gap(scaled(MONITOR_GAP, density))
                    .child(
                        div()
                            .w(MONITOR_KIND_W)
                            .flex_shrink_0()
                            .text_color(palette.text_faint)
                            .child(short_kind_label(event.kind)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(kind_color(event.kind.event_kind(), palette))
                            .child(monitor_value_label(event)),
                    ),
            );
        }

        if !self.enabled {
            lines = lines.child(
                div()
                    .text_color(palette.text_faint)
                    .child(tr!("midi_monitor_disabled")),
            );
        } else if self.monitor.is_empty() {
            lines = lines.child(
                div()
                    .text_color(palette.text_faint)
                    .child(tr!("midi_monitor_empty")),
            );
        }

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("midi_section_monitor"), palette)
                    .row()
                    .trailing(None),
            )
            .child(lines);

        div()
            .w_full()
            .mt(MONITOR_MT)
            .child(
                card(body, palette)
                    .padding(scaled(MONITOR_PAD, density))
                    .full_width(),
            )
            .into_any_element()
    }
}
