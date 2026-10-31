use super::{Capture, MappingRow, MidiScreenView};
use crate::midi_signal::kind_color;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_BADGE, FONT_LABEL, FONT_MICRO, FONT_XS,
    FONT_XXS, ForgePalette, Icon, MenuPlacement, Radius, SectionLabel, Spacing, badge, body_family,
    card, icon, menu_button, menu_divider, menu_item, mono_family, pad_tile, radius, scaled,
    spacing, status_dot, tr,
};
use forge_types::TriggerInstanceId;
use gpui::{AnyElement, ClickEvent, Context, Pixels, div, prelude::*, px};

const BINDINGS_FS: Pixels = FONT_MICRO;
const MAP_PAD_V: Pixels = px(9.0);
const MAP_PAD_H: Pixels = px(12.0);
const SIG_FS: Pixels = FONT_LABEL;
const SIG_RADIUS: Pixels = px(5.0);
const SIG_PAD_V: Pixels = px(3.0);
const SIG_PAD_H: Pixels = px(8.0);
const SIG_MIN_W: Pixels = px(66.0);
const SIG_LINE_H: Pixels = px(14.0);
const CH_BADGE_FS: Pixels = FONT_BADGE;
const ARROW_GLYPH: Pixels = px(13.0);
const ACCENT_DOT: Pixels = px(6.0);
const TARGET_FS: Pixels = FONT_XS;
const ADD_BAR_PAD_H: Pixels = px(12.0);
const ADD_BAR_RADIUS: Pixels = radius(Radius::Md);
const ADD_BAR_GLYPH: Pixels = px(13.0);
const ADD_BAR_FS: Pixels = FONT_XS;
const KBD_FS: Pixels = FONT_MICRO;
const KBD_PAD_V: Pixels = px(1.0);
const KBD_RADIUS: Pixels = px(4.0);
const FOOTER_FS: Pixels = FONT_XXS;
const FOOTER_DOT: Pixels = px(6.0);
const FOOTER_PAD_V: Pixels = px(7.0);
const FOOTER_PAD_H: Pixels = px(14.0);
const FOOTER_MT: Pixels = px(14.0);
const LEARN_PAD_V: Pixels = px(11.0);
const LEARN_GAP: Pixels = px(8.0);
const LEARN_GLYPH: Pixels = px(14.0);

impl MidiScreenView {
    pub(super) fn render_mappings(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let visible = self.visible_mappings();
        let count = div()
            .font_family(mono_family())
            .text_size(BINDINGS_FS)
            .text_color(palette.text_faint)
            .child(tr!(
                "midi_bindings_count",
                count = i64::try_from(visible.len()).unwrap_or(i64::MAX)
            ))
            .into_any_element();

        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        if visible.is_empty() {
            let empty = if self.mappings.is_empty() {
                tr!("midi_mappings_empty")
            } else {
                tr!("midi_mappings_empty_device")
            };
            list = list.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(empty),
            );
        } else {
            for (index, row) in visible.iter().enumerate() {
                list = list.child(self.render_mapping(index, row, palette, cx));
            }
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("midi_section_mappings"), palette)
                    .row()
                    .trailing(Some(count)),
            )
            .child(list.child(self.render_add_bar(palette, cx)))
            .into_any_element()
    }

    fn render_mapping(
        &self,
        index: usize,
        row: &MappingRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let (target_text, target_ink) = match row.action.as_ref() {
            Some((_, name)) => (name.clone(), palette.text_primary),
            None => (tr!("midi_unassigned"), palette.text_faint),
        };
        let id = row.id;
        let mut content = div()
            .id(("midi-mapping-row", index))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.edit_mapping(id, cx)))
            .child(
                div()
                    .flex_none()
                    .min_w(SIG_MIN_W)
                    .py(scaled(SIG_PAD_V, density))
                    .px(scaled(SIG_PAD_H, density))
                    .rounded(SIG_RADIUS)
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .bg(palette.shell)
                    .text_align(gpui::TextAlign::Center)
                    .font_family(mono_family())
                    .text_size(SIG_FS)
                    .line_height(SIG_LINE_H)
                    .text_color(palette.text_primary)
                    .child(row.signal.label()),
            )
            .child(
                badge(
                    palette.surface_overlay,
                    palette.text_muted,
                    row.signal.channel_label(),
                    true,
                    CH_BADGE_FS,
                )
                .flex_none(),
            );

        if row.signal.device.is_none() {
            content = content.child(
                badge(
                    palette.surface_overlay,
                    palette.text_faint,
                    tr!("midi_mapping_any_device"),
                    true,
                    CH_BADGE_FS,
                )
                .flex_none(),
            );
        }

        let content = content
            .child(icon(Icon::ArrowRight, ARROW_GLYPH, palette.text_faint))
            .child(
                div()
                    .flex_none()
                    .size(ACCENT_DOT)
                    .rounded_full()
                    .bg(kind_color(&row.signal.kind_id, palette)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .text_size(TARGET_FS)
                    .text_color(target_ink)
                    .child(target_text),
            );

        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(content)
            .child(self.render_row_menu(index, row.id, palette, cx));

        div()
            .w_full()
            .child(
                card(body, palette)
                    .padding_xy(scaled(MAP_PAD_V, density), scaled(MAP_PAD_H, density))
                    .full_width(),
            )
            .into_any_element()
    }

    fn render_row_menu(
        &self,
        index: usize,
        id: TriggerInstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.menu_open == Some(id);
        let view = cx.entity();
        menu_button(Icon::DotsVertical, open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(self.menu_click_pos)
            .items(vec![
                menu_item(
                    ("midi-menu-edit", index),
                    tr!("midi_menu_edit"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.edit_mapping(id, cx)),
                )
                .icon(Icon::Edit)
                .into(),
                menu_item(
                    ("midi-menu-duplicate", index),
                    tr!("common_duplicate"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate(id, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_divider(),
                menu_item(
                    ("midi-menu-delete", index),
                    tr!("common_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.delete(id, cx)),
                )
                .icon(Icon::Trash)
                .color(palette.danger)
                .into(),
            ])
            .on_toggle(
                ("midi-menu-trigger", index),
                cx.listener(move |this, event: &ClickEvent, _, cx| {
                    this.toggle_menu(id, event.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    fn render_add_bar(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        if self.capture == Capture::Learn {
            return self.render_learn_row(palette, cx);
        }
        let title = div()
            .flex()
            .items_center()
            .child(tr!("midi_add_learn"))
            .child(
                div()
                    .ml(spacing(Spacing::Xs, density))
                    .py(scaled(KBD_PAD_V, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(KBD_RADIUS)
                    .bg(palette.surface_overlay)
                    .font_family(mono_family())
                    .text_size(KBD_FS)
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(palette.text_faint)
                    .child(tr!("midi_add_learn_kbd")),
            );

        pad_tile(
            "midi-add-learn",
            icon(Icon::Plus, ADD_BAR_GLYPH, palette.info),
            title,
            palette,
        )
        .bar(palette)
        .title_color(palette.info)
        .hover_border(palette.info)
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_learn(cx)))
        .into_any_element()
    }

    fn render_learn_row(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let (accent, prompt) = if self.enabled {
            (palette.info, tr!("midi_learn_prompt"))
        } else {
            (palette.warning, tr!("midi_learn_input_disabled"))
        };
        let cancel_hover = palette.text_secondary;
        div()
            .id("midi-learn-row")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(scaled(LEARN_GAP, density))
            .py(scaled(LEARN_PAD_V, density))
            .px(scaled(ADD_BAR_PAD_H, density))
            .rounded(ADD_BAR_RADIUS)
            .border(BORDER_THIN)
            .border_color(accent)
            .bg(palette.surface_overlay)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_learn(cx)))
            .child(icon(Icon::Antenna, LEARN_GLYPH, accent))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .text_color(accent)
                    .child(prompt),
            )
            .child(
                div()
                    .id("midi-learn-cancel")
                    .ml(spacing(Spacing::Xs, density))
                    .flex()
                    .items_center()
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .text_color(palette.text_faint)
                    .cursor_pointer()
                    .hover(move |style| style.text_color(cancel_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.cancel_learn(cx);
                    }))
                    .child(tr!("common_cancel")),
            )
            .into_any_element()
    }

    pub(super) fn render_footer(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let (dot, status_text) = if self.enabled {
            (palette.success, tr!("midi_engine_running"))
        } else {
            (palette.text_faint, tr!("midi_engine_stopped"))
        };
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
                    .font_family(mono_family())
                    .text_size(FOOTER_FS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "midi_footer_left",
                        devices = i64::try_from(self.live_ports.len()).unwrap_or(i64::MAX),
                        mappings = i64::try_from(self.mappings.len()).unwrap_or(i64::MAX)
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(status_dot(dot, FOOTER_DOT))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FOOTER_FS)
                            .text_color(palette.text_faint)
                            .child(status_text),
                    ),
            )
            .into_any_element()
    }
}
