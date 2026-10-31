use super::ScriptEditorView;
use super::contract_editing::KindTarget;
use super::details_pane::{details_heading, variant_kind_display};
use crate::globals::variant_kind_color;
use forge_components::{
    ActivePresentation, BORDER_THIN, FONT_XS, FONT_XXS, ForgePalette, Icon, MenuItem, Radius,
    Spacing, body_family, context_menu, icon, menu_divider, menu_item, mono_family, radius,
    spacing, tr, with_alpha,
};
use forge_types::{ScriptInput, VariantKind};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, Pixels, Rgba,
    SharedString, Window, div, px,
};

const CONTRACT_KINDS: [VariantKind; 7] = [
    VariantKind::Int,
    VariantKind::Float,
    VariantKind::Bool,
    VariantKind::String,
    VariantKind::Datetime,
    VariantKind::Array,
    VariantKind::Object,
];

const SIGNATURE_NAME_MIN_W: Pixels = px(56.0);
const SIGNATURE_ACTION_PAD: Pixels = px(3.0);
const GLYPH_SIGNATURE_ACTION: Pixels = px(11.0);
const GLYPH_RETURNS: Pixels = px(11.0);

impl ScriptEditorView {
    pub(super) fn signature_section(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let Some(contract) = self.contract() else {
            return div().into_any_element();
        };

        let mut section = div().flex().flex_col().child(details_heading(
            tr!("script_editor_signature_heading"),
            true,
            palette,
            density,
        ));

        if contract.inputs.is_empty() {
            section = section.child(
                div()
                    .py(spacing(Spacing::Xxs, density))
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("script_editor_signature_no_inputs")),
            );
        }

        let last_index = contract.inputs.len().saturating_sub(1);
        for (index, input) in contract.inputs.iter().enumerate() {
            section = section.child(self.signature_input_row(
                index,
                input,
                index == last_index,
                palette,
                cx,
            ));
        }

        if let Some(error) = self.contract_error.as_ref() {
            section = section.child(
                div()
                    .pt(spacing(Spacing::Xxs, density))
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.danger)
                    .child(error.clone()),
            );
        }

        let hover_bg = palette.surface_overlay;
        section
            .child(
                div()
                    .id("script-contract-add")
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .mt(spacing(Spacing::Xxs, density))
                    .py(spacing(Spacing::Xxs, density.tighter()))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.add_contract_input(cx)),
                    )
                    .child(icon(Icon::Plus, GLYPH_SIGNATURE_ACTION, palette.brand))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.brand)
                            .child(tr!("script_editor_signature_add_input")),
                    ),
            )
            .child(self.signature_returns_row(contract.returns, palette, cx))
            .into_any_element()
    }

    fn signature_input_row(
        &self,
        index: usize,
        input: &ScriptInput,
        is_last: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let hover_bg = palette.surface_overlay;
        let renaming = self
            .input_rename
            .as_ref()
            .filter(|state| state.index == index);

        let name_cell: AnyElement = match renaming {
            Some(state) => div()
                .flex_1()
                .min_w(SIGNATURE_NAME_MIN_W)
                .child(state.editor.clone())
                .into_any_element(),
            None => div()
                .id(("script-contract-name", index))
                .flex_1()
                .min_w(SIGNATURE_NAME_MIN_W)
                .truncate()
                .cursor_pointer()
                .text_color(palette.code_var)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.start_input_rename(index, window, cx)
                }))
                .child(SharedString::from(input.name.clone()))
                .into_any_element(),
        };

        let kind = input.kind;
        let kind_cell = div()
            .id(("script-contract-kind", index))
            .flex_none()
            .px(spacing(Spacing::Xxs, density.tighter()))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .text_size(FONT_XXS)
            .text_color(variant_kind_color(kind, palette))
            .hover(move |s| s.bg(hover_bg))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    let position = e.position;
                    this.open_kind_menu(KindTarget::Input(index), position, cx);
                }),
            )
            .child(variant_kind_display(kind));

        let move_up: AnyElement = if index == 0 {
            signature_action_disabled(Icon::ArrowUp, palette).into_any_element()
        } else {
            signature_action(
                ("script-contract-up", index),
                Icon::ArrowUp,
                palette.text_faint,
                hover_bg,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.move_contract_input(index, true, cx)
                }),
            )
            .into_any_element()
        };

        let move_down: AnyElement = if is_last {
            signature_action_disabled(Icon::ArrowDown, palette).into_any_element()
        } else {
            signature_action(
                ("script-contract-down", index),
                Icon::ArrowDown,
                palette.text_faint,
                hover_bg,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.move_contract_input(index, false, cx)
                }),
            )
            .into_any_element()
        };

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xxs, density))
            .font_family(mono_family())
            .text_size(FONT_XS)
            .child(name_cell)
            .child(kind_cell)
            .child(move_up)
            .child(move_down)
            .child(signature_action(
                ("script-contract-remove", index),
                Icon::X,
                palette.text_faint,
                hover_bg,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.remove_contract_input(index, cx)
                }),
            ))
            .into_any_element()
    }

    fn signature_returns_row(
        &self,
        returns: Option<VariantKind>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let hover_bg = palette.surface_overlay;
        let (value, value_color): (SharedString, Rgba) = match returns {
            Some(kind) => (
                variant_kind_display(kind).into(),
                variant_kind_color(kind, palette),
            ),
            None => (
                tr!("script_editor_signature_no_return").into(),
                palette.text_faint,
            ),
        };

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .mt(spacing(Spacing::Xxs, density))
            .pt(spacing(Spacing::Xs, density))
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .font_family(mono_family())
            .text_size(FONT_XS)
            .child(icon(Icon::ArrowBackUp, GLYPH_RETURNS, palette.text_faint))
            .child(
                div()
                    .text_color(palette.text_faint)
                    .child(tr!("script_editor_details_returns")),
            )
            .child(
                div()
                    .id("script-contract-returns")
                    .flex_1()
                    .flex()
                    .justify_end()
                    .px(spacing(Spacing::Xxs, density.tighter()))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .text_size(FONT_XXS)
                    .text_color(value_color)
                    .hover(move |s| s.bg(hover_bg))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &MouseDownEvent, _, cx| {
                            let position = e.position;
                            this.open_kind_menu(KindTarget::Returns, position, cx);
                        }),
                    )
                    .child(value),
            )
            .into_any_element()
    }

    pub(super) fn render_kind_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.kind_menu.as_ref()?;
        let target = menu.target;
        let position = menu.position;
        let view = cx.entity();

        let mut items: Vec<MenuItem> = Vec::new();
        if target == KindTarget::Returns {
            items.push(
                menu_item(
                    "script-contract-kind-none",
                    tr!("script_editor_signature_no_return"),
                    cx.listener(|this, _: &ClickEvent, _, cx| this.clear_contract_return(cx)),
                )
                .color(palette.text_faint)
                .into(),
            );
            items.push(menu_divider());
        }
        for kind in CONTRACT_KINDS {
            items.push(
                menu_item(
                    SharedString::from(format!("script-contract-kind-{}", kind.contract_name())),
                    variant_kind_display(kind),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_contract_kind(target, kind, cx)
                    }),
                )
                .color(variant_kind_color(kind, palette))
                .into(),
            );
        }

        Some(
            context_menu(position, palette)
                .items(items)
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_kind_menu(cx));
                })
                .into_any_element(),
        )
    }
}

fn signature_action<F>(
    id: impl Into<ElementId>,
    glyph: Icon,
    tint: Rgba,
    hover_bg: Rgba,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .p(SIGNATURE_ACTION_PAD)
        .rounded(radius(Radius::Sm))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_click(on_click)
        .child(icon(glyph, GLYPH_SIGNATURE_ACTION, tint))
}

fn signature_action_disabled(glyph: Icon, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .p(SIGNATURE_ACTION_PAD)
        .child(icon(
            glyph,
            GLYPH_SIGNATURE_ACTION,
            with_alpha(palette.text_faint, 0.35),
        ))
}
