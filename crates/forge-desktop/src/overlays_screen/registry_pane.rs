use forge_components::{
    BORDER_THIN, FONT_XXS, ForgePalette, Icon, MenuPlacement, body_family, icon, menu_button,
    menu_divider, menu_item, mono_family, pad_tile, toggle, tr,
};
use forge_storage::{OverlayDefinition, OverlayId};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Pixels, Rgba, SharedString, div, prelude::*, px,
};

use super::OverlaysView;

const PANE_W: Pixels = px(254.0);
const PANE_HEAD_PAD_TOP: Pixels = px(12.0);
const PANE_HEAD_PAD_H: Pixels = px(12.0);
const PANE_HEAD_PAD_BOTTOM: Pixels = px(8.0);
const PANE_TITLE_FS: Pixels = px(13.0);

const LIST_PAD_H: Pixels = px(8.0);
const LIST_PAD_TOP: Pixels = px(8.0);
const LIST_PAD_BOTTOM: Pixels = px(12.0);

const ROW_GAP: Pixels = px(10.0);
const ROW_PAD_V: Pixels = px(9.0);
const ROW_PAD_H: Pixels = px(10.0);
const ROW_RADIUS: Pixels = px(8.0);
const ROW_MB: Pixels = px(2.0);
const ROW_STRIPE_W: Pixels = px(2.0);
const ROW_TILE: Pixels = px(28.0);
const ROW_TILE_RADIUS: Pixels = px(7.0);
const ROW_GLYPH: Pixels = px(14.0);
const ROW_NAME_FS: Pixels = px(12.5);
const ROW_KIND_FS: Pixels = px(9.5);

const EMPTY_PAD_V: Pixels = px(14.0);
const EMPTY_PAD_H: Pixels = px(10.0);

const FOOTER_PAD_V: Pixels = px(10.0);
const FOOTER_PAD_H: Pixels = px(12.0);
const ADD_GLYPH: Pixels = px(13.0);

impl OverlaysView {
    pub(super) fn render_registry_pane(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_none()
            .w(PANE_W)
            .min_w(PANE_W)
            .max_w(PANE_W)
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(self.render_pane_header(palette))
            .child(self.render_list(palette, cx))
            .child(self.render_add_bar(palette, cx))
            .into_any_element()
    }

    fn render_pane_header(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(PANE_HEAD_PAD_H)
            .pt(PANE_HEAD_PAD_TOP)
            .pb(PANE_HEAD_PAD_BOTTOM)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(PANE_TITLE_FS)
                    .text_color(palette.text_primary)
                    .child(tr!("overlays_pane_title")),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(self.overlays.len().to_string()),
            )
            .into_any_element()
    }

    fn render_list(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("overlays-registry-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px(LIST_PAD_H)
            .pt(LIST_PAD_TOP)
            .pb(LIST_PAD_BOTTOM);

        if self.overlays.is_empty() {
            let message = if self.loading {
                tr!("overlays_pane_loading")
            } else {
                tr!("overlays_pane_empty")
            };
            return list.child(empty_note(message, palette)).into_any_element();
        }

        for (index, definition) in self.overlays.iter().enumerate() {
            list = list.child(self.render_row(index, definition, palette, cx));
        }
        list.into_any_element()
    }

    fn render_row(
        &self,
        index: usize,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = definition.id.clone();
        let selected = self.selected.as_ref() == Some(&id);
        let visuals = self.visuals(definition, palette);

        let stripe: Rgba = if selected {
            visuals.accent
        } else {
            gpui::transparent_black().into()
        };
        let row_bg: Rgba = if selected {
            palette.surface_overlay
        } else {
            gpui::transparent_black().into()
        };
        let name_color = if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };
        let glyph_color = if definition.enabled {
            visuals.accent
        } else {
            palette.text_faint
        };
        let kind_label = visuals
            .label
            .clone()
            .unwrap_or_else(|| tr!("overlays_type_unavailable"));

        let tile = div()
            .flex_none()
            .size(ROW_TILE)
            .flex()
            .items_center()
            .justify_center()
            .rounded(ROW_TILE_RADIUS)
            .bg(palette.elevated)
            .child(icon(visuals.icon, ROW_GLYPH, glyph_color));

        let labels = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .truncate()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(ROW_NAME_FS)
                    .text_color(name_color)
                    .child(definition.display_name.clone()),
            )
            .child(
                div()
                    .w_full()
                    .truncate()
                    .font_family(mono_family())
                    .text_size(ROW_KIND_FS)
                    .text_color(palette.text_faint)
                    .child(kind_label),
            );

        let toggle_id = id.clone();
        let switch = toggle(definition.enabled, palette)
            .on_color(visuals.accent)
            .on_click(
                ("overlay-toggle", index),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.toggle_enabled(toggle_id.clone(), cx)
                }),
            );

        let select_id = id.clone();
        let hover_bg = palette.elevated;
        div()
            .id(("overlay-row", index))
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .px(ROW_PAD_H)
            .py(ROW_PAD_V)
            .mb(ROW_MB)
            .rounded(ROW_RADIUS)
            .border_l(ROW_STRIPE_W)
            .border_color(stripe)
            .bg(row_bg)
            .cursor_pointer()
            .when(!selected, |row| row.hover(move |s| s.bg(hover_bg)))
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select(select_id.clone(), cx)),
            )
            .child(tile)
            .child(labels)
            .child(switch)
            .child(self.render_row_menu(index, &id, palette, cx))
            .into_any_element()
    }

    fn render_row_menu(
        &self,
        index: usize,
        id: &OverlayId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.menu_open.as_ref() == Some(id);
        let view = cx.entity();
        let rename_id = id.clone();
        let copy_id = id.clone();
        let delete_id = id.clone();
        let toggle_id = id.clone();

        menu_button(Icon::DotsVertical, open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(self.menu_click_pos)
            .items(vec![
                menu_item(
                    ("overlay-menu-rename", index),
                    tr!("overlays_menu_rename"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_rename_form(rename_id.clone(), cx)
                    }),
                )
                .icon(Icon::Pencil)
                .into(),
                menu_item(
                    ("overlay-menu-copy-url", index),
                    tr!("overlays_menu_copy_url"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_url(&copy_id, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_divider(),
                menu_item(
                    ("overlay-menu-delete", index),
                    tr!("common_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.prompt_delete(delete_id.clone(), cx)
                    }),
                )
                .icon(Icon::Trash)
                .color(palette.random)
                .into(),
            ])
            .on_toggle(
                ("overlay-menu-trigger", index),
                cx.listener(move |this, event: &ClickEvent, _, cx| {
                    this.toggle_menu(&toggle_id, event.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    fn render_add_bar(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_none()
            .px(FOOTER_PAD_H)
            .py(FOOTER_PAD_V)
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                pad_tile(
                    "overlays-add",
                    icon(Icon::Plus, ADD_GLYPH, palette.brand),
                    div()
                        .flex()
                        .items_center()
                        .child(tr!("overlays_add_overlay")),
                    palette,
                )
                .bar(palette)
                .title_color(palette.brand)
                .hover_border(palette.brand)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_create_form(cx))),
            )
            .into_any_element()
    }
}

fn empty_note(message: String, palette: &ForgePalette) -> impl IntoElement {
    div()
        .w_full()
        .px(EMPTY_PAD_H)
        .py(EMPTY_PAD_V)
        .font_family(body_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_faint)
        .child(SharedString::from(message))
}
