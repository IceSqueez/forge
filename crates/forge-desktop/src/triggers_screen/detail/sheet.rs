use super::*;
use crate::integration_switch::{category_inactive_badge, inactive_badge};
use forge_components::ActivePresentation;
use forge_components::FONT_SM;
use forge_components::OverflowRevealingFocus;
use forge_components::ROW_OFF_OPACITY;
use forge_components::{
    FONT_XXS, Icon, KeyActivated, Radius, ResizeEdge, ResizeRange, Spacing, body_family,
    ghost_button_with_icon, icon, install_resize, mono_family, primary_button, radius, scaled,
    spacing, status_dot, toggle, tr,
};
use gpui::{AnyElement, ClickEvent, FontWeight, SharedString};

const DETAIL_SHEET_MIN: Pixels = px(320.0);
const DETAIL_SHEET_MAX: Pixels = px(640.0);
const TILE: Pixels = px(30.0);
const TILE_RADIUS: Pixels = px(8.0);
const TILE_GLYPH: Pixels = px(16.0);
const KIND_DOT: Pixels = px(5.0);
const NAME_FS: Pixels = FONT_SM;
const KIND_FS: Pixels = FONT_XXS;
struct TriggerDetailResizeDrag;

const HEADER_PAD_V: Pixels = px(12.0);
const BODY_PAD_V: Pixels = px(14.0);

impl TriggerDetailPane {
    pub(in crate::triggers_screen) fn render_detail_sheet(
        &self,
        id: TriggerInstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.detail.as_ref().filter(|d| d.instance.id == id) {
            Some(detail) => self.render_detail_panel(detail, palette, cx),
            None => self.render_detail_loading(palette, cx),
        }
    }

    fn detail_shell(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> gpui::Div {
        let panel = div()
            .w(self.detail_width)
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .min_h(px(0.0))
            .bg(palette.base)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular);

        install_resize(
            panel,
            TriggerDetailResizeDrag,
            "triggers-detail-resize",
            ResizeEdge::Left,
            ResizeRange {
                min: DETAIL_SHEET_MIN,
                max: DETAIL_SHEET_MAX,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_detail_width(*width, cx)),
        )
    }

    fn render_detail_loading(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        self.detail_shell(palette, cx)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(body_family())
                    .text_size(CFG_VAL_FS)
                    .text_color(palette.text_muted)
                    .child(tr!("triggers_detail_loading")),
            )
            .into_any_element()
    }

    fn render_detail_panel(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.detail_shell(palette, cx)
            .child(self.render_detail_header(detail, palette, cx))
            .child(self.render_detail_body(detail, palette, cx))
            .child(self.render_detail_footer(detail, palette, cx))
            .into_any_element()
    }

    fn render_detail_header(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let instance = &detail.instance;
        let id = instance.id;
        let dot_color = platform_dot_color(&instance.kind_id, &self.deps.registry, palette);
        let glyph = self
            .deps
            .registry
            .get(&instance.kind_id)
            .map(|d| Icon::from_name(d.icon_name()))
            .unwrap_or(Icon::Bolt);

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(TILE)
            .rounded(TILE_RADIUS)
            .bg(palette.surface_overlay)
            .child(icon(glyph, TILE_GLYPH, dot_color));

        let name = div()
            .id("triggers-detail-rename")
            .cursor_pointer()
            .font_family(body_family())
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(NAME_FS)
            .text_color(palette.text_primary)
            .child(instance.name.clone())
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(TriggerPaneEvent::StartRename(id))
            }));
        let kind_line = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(dot_color, KIND_DOT))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(KIND_FS)
                    .text_color(palette.text_faint)
                    .child(instance.kind_id.clone()),
            )
            .when_some(self.switched_off_owner(&instance.kind_id), |line, owner| {
                line.child(inactive_badge(&owner, palette, density))
            })
            .when_some(
                self.uncovered_category(&instance.kind_id),
                |line, category| line.child(category_inactive_badge(category, palette, density)),
            );
        let title = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(scaled(px(2.0), density))
            .child(name)
            .child(kind_line);

        let enable_toggle = toggle(instance.enabled, palette).on_click(
            SharedString::from(format!("triggers-detail-toggle-{id}")),
            cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(TriggerPaneEvent::ToggleEnable(id))
            }),
        );

        let close = div()
            .id("triggers-detail-close")
            .flex_none()
            .cursor_pointer()
            .p(spacing(Spacing::Xxs, density))
            .child(icon(Icon::X, px(15.0), palette.text_faint))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_detail(cx)));

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(HEADER_PAD_V, density))
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(tile)
            .child(title)
            .child(enable_toggle)
            .child(close)
            .into_any_element()
    }

    fn render_detail_body(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        div()
            .id("triggers-detail-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_revealing_focus()
            .py(scaled(BODY_PAD_V, density))
            .px(spacing(Spacing::Md, density))
            .flex()
            .flex_col()
            .child(self.render_config_section(detail, palette, cx))
            .when(
                cooldown_applies(self.deps.registry.get(&detail.instance.kind_id)),
                |body| body.child(self.render_cooldown_section(detail, palette, cx)),
            )
            .children(self.render_permission_section(detail, palette, cx))
            .child(self.render_used_in_section(detail, palette, cx))
            .into_any_element()
    }

    fn render_detail_footer(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let id = detail.instance.id;
        let can_delete = self.listed_uses == 0;

        let template = ghost_button_with_icon(Icon::Copy, tr!("triggers_menu_template"), palette)
            .on_click(
                "triggers-detail-template",
                cx.listener(|this, _: &ClickEvent, _, cx| this.use_as_template(cx)),
            );

        let delete_color = if can_delete {
            palette.danger
        } else {
            palette.disabled
        };
        let delete_base = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .child(icon(Icon::Eraser, px(11.0), delete_color))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(CFG_VAL_FS)
                    .text_color(delete_color)
                    .child(tr!("triggers_sheet_delete_btn")),
            );
        let delete: AnyElement = if can_delete {
            delete_base
                .id("triggers-detail-delete")
                .cursor_pointer()
                .key_activated(palette.text_primary)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(TriggerPaneEvent::RequestDelete(id))
                }))
                .into_any_element()
        } else {
            delete_base.opacity(ROW_OFF_OPACITY).into_any_element()
        };

        let save = primary_button(tr!("triggers_sheet_save_btn"), palette).on_click(
            "triggers-detail-save",
            cx.listener(|this, _: &ClickEvent, _, cx| this.commit_config(cx)),
        );

        div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .bg(palette.shell)
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(template)
            .child(div().flex_1())
            .child(save)
            .child(delete)
            .into_any_element()
    }
}
