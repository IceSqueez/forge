use super::super::permission_help::{gated_platforms, rung_meanings};
use super::*;
use crate::config_field_label::row_label;
use crate::config_form::{config_label_cell, render_config_control};
use forge_components::ActivePresentation;
use forge_components::FONT_META;
use forge_components::FONT_XS;
use forge_components::{
    Density, FONT_XXS, Icon, KeyActivated, Radius, Spacing, body_family, icon, mono_family, radius,
    row_card, scaled, segment, segmented, spacing, toggle, tr,
};
use gpui::{AnyElement, ClickEvent, SharedString};
use std::collections::HashSet;

const ROW_GLYPH: Pixels = px(12.0);
const CFG_KEY_FS: Pixels = FONT_META;
const USED_FS: Pixels = FONT_XS;
const CFG_KEY_W: Pixels = px(110.0);
const REVERT_W: Pixels = px(22.0);

const CFG_ROW_PAD_V: Pixels = px(8.0);
const CFG_ROW_PAD_H: Pixels = px(12.0);
const USED_ROW_PAD_V: Pixels = px(7.0);
const USED_ROW_PAD_H: Pixels = px(10.0);
const PLACEHOLDER_PAD_V: Pixels = px(14.0);
const PLACEHOLDER_PAD_H: Pixels = px(12.0);

impl TriggerDetailPane {
    fn section_label(
        &self,
        label: impl Into<SharedString>,
        right: Option<AnyElement>,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let label = label.into();
        div()
            .w_full()
            .flex()
            .items_center()
            .py(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex_1()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(label),
            )
            .children(right)
            .into_any_element()
    }

    pub(super) fn render_config_section(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let override_count = detail.instance.overrides.len();
        let right = if override_count > 0 {
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.accent_peach)
                .child(tr!(
                    "triggers_sheet_config_overridden",
                    count = i64::try_from(override_count).unwrap_or(i64::MAX)
                ))
                .into_any_element()
        } else {
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_config_all_defaults"))
                .into_any_element()
        };

        let card: AnyElement = if detail.fields.is_empty() {
            div()
                .py(scaled(PLACEHOLDER_PAD_V, density))
                .px(scaled(PLACEHOLDER_PAD_H, density))
                .italic()
                .font_family(body_family())
                .text_size(CFG_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_no_config"))
                .into_any_element()
        } else {
            let overridden: HashSet<&str> = detail
                .instance
                .overrides
                .keys()
                .map(String::as_str)
                .collect();
            let last = detail.fields.len().saturating_sub(1);
            let mut col = div().flex().flex_col();
            for (i, field) in detail.fields.iter().enumerate() {
                let label = row_label(&detail.labels, i, field);
                col = col.child(self.render_config_row(
                    field,
                    label,
                    &overridden,
                    i == last,
                    palette,
                    cx,
                ));
            }
            col.into_any_element()
        };

        let framed = div()
            .w_full()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(card);

        div()
            .flex()
            .flex_col()
            .pb(spacing(Spacing::Md, density))
            .child(self.section_label(
                tr!("triggers_sheet_section_configuration"),
                Some(right),
                palette,
                density,
            ))
            .child(framed)
            .children(
                timer_field_hints(
                    self.deps.registry.get(&detail.instance.kind_id),
                    palette,
                    density,
                )
                .map(|hints| div().pt(spacing(Spacing::Xs, density)).child(hints)),
            )
            .into_any_element()
    }

    fn render_config_row(
        &self,
        field: &ConfigField,
        label: SharedString,
        overridden: &HashSet<&str>,
        last: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let key = field.key().to_owned();
        let is_overridden = overridden.contains(key.as_str());
        let key_color = if is_overridden {
            palette.accent_peach
        } else {
            palette.text_muted
        };

        let label = config_label_cell(label, CFG_KEY_W, CFG_KEY_FS, key_color);

        let value = render_config_control(
            field,
            palette,
            "triggers-cfg",
            &cx.entity(),
            &self.detail_config_handlers(),
            density,
        );

        let revert: AnyElement = if is_overridden && !matches!(field, ConfigField::Hint { .. }) {
            let revert_key = key.clone();
            let hover = palette.surface_overlay;
            div()
                .id(SharedString::from(format!("triggers-cfg-revert-{key}")))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .size(REVERT_W)
                .rounded(radius(Radius::Sm))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(icon(Icon::X, px(11.0), palette.text_faint))
                .key_activated(palette.text_primary)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.revert_config_field(revert_key.clone(), cx)
                }))
                .into_any_element()
        } else {
            div().w(REVERT_W).flex_none().into_any_element()
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .when(!last, |row| {
                row.border_b(BORDER_THIN)
                    .border_color(palette.border_regular)
            })
            .child(label)
            .child(div().flex_1().min_w(px(0.0)).child(value))
            .child(revert)
            .into_any_element()
    }

    pub(super) fn render_cooldown_section(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let caption = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("triggers_sheet_cooldown_caption"))
            .into_any_element();

        let label_cell = |label: SharedString| {
            div()
                .w(CFG_KEY_W)
                .flex_none()
                .overflow_hidden()
                .font_family(mono_family())
                .text_size(CFG_KEY_FS)
                .text_color(palette.text_muted)
                .child(label)
        };

        let value_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(label_cell(tr!("triggers_sheet_cooldown_value").into()))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(detail.cooldown_input.clone()),
            );

        let scope_toggle = toggle(!detail.cooldown_per_user, palette).on_click(
            "triggers-detail-cooldown-scope",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_cooldown_scope(cx)),
        );
        let scope_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .child(label_cell(tr!("triggers_sheet_cooldown_scope").into()))
            .child(div().flex_1().min_w(px(0.0)).child(scope_toggle));

        let card = div().flex().flex_col().child(value_row).child(scope_row);

        let framed = div()
            .w_full()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(card);

        div()
            .flex()
            .flex_col()
            .pb(spacing(Spacing::Md, density))
            .child(self.section_label(
                tr!("triggers_sheet_section_cooldown"),
                Some(caption),
                palette,
                density,
            ))
            .child(framed)
            .into_any_element()
    }

    pub(super) fn render_permission_section(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let density = cx.density();
        let descriptor = self.deps.registry.get(&detail.instance.kind_id)?;
        descriptor.chat_trigger_family()?;
        let platforms = gated_platforms(
            descriptor.platform_contract(),
            &detail.instance.platform_scope,
        );
        let meanings = rung_meanings(detail.permission_rung, &platforms);

        let segments = PERMISSION_RUNGS
            .into_iter()
            .map(|rung| {
                segment(
                    SharedString::from(format!("triggers-detail-rung-{}", rung.as_str())),
                    permission_rung_label(rung),
                    detail.permission_rung == rung,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_permission_rung(rung, cx)
                    }),
                )
            })
            .collect();
        let ladder = segmented(segments, palette).wrap(spacing(Spacing::Xxs, density));

        let rung_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .w(CFG_KEY_W)
                    .flex_none()
                    .overflow_hidden()
                    .font_family(mono_family())
                    .text_size(CFG_KEY_FS)
                    .text_color(palette.text_muted)
                    .child(tr!("triggers_sheet_permission_value")),
            )
            .child(div().flex_1().min_w(px(0.0)).child(ladder));

        let helper_row = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .py(scaled(CFG_ROW_PAD_V, density))
            .px(scaled(CFG_ROW_PAD_H, density))
            .font_family(body_family())
            .text_size(CFG_VAL_FS)
            .text_color(palette.text_faint)
            .child(tr!("triggers_sheet_permission_helper"))
            .children(meanings.into_iter().map(|meaning| div().child(meaning)));

        let framed = div()
            .w_full()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .child(div().flex().flex_col().child(rung_row).child(helper_row));

        Some(
            div()
                .flex()
                .flex_col()
                .pb(spacing(Spacing::Md, density))
                .child(self.section_label(
                    tr!("triggers_sheet_section_permission"),
                    None,
                    palette,
                    density,
                ))
                .child(framed)
                .into_any_element(),
        )
    }

    pub(super) fn render_used_in_section(
        &self,
        detail: &TriggerDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let count = detail.used_in.len();
        let label = if count > 0 {
            tr!(
                "triggers_sheet_section_used_in_count",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            )
        } else {
            tr!("triggers_sheet_section_used_in")
        };

        let inner: AnyElement = if detail.used_in.is_empty() {
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .gap(scaled(px(2.0), density))
                .py(scaled(PLACEHOLDER_PAD_V, density))
                .px(scaled(PLACEHOLDER_PAD_H, density))
                .rounded(radius(Radius::Md))
                .border(BORDER_THIN)
                .border_color(palette.border_input)
                .text_center()
                .font_family(body_family())
                .text_size(CFG_VAL_FS)
                .text_color(palette.text_faint)
                .child(tr!("triggers_sheet_used_in_empty_title"))
                .child(tr!("triggers_sheet_used_in_empty_hint"))
                .into_any_element()
        } else {
            let mut col = div().flex().flex_col().gap(scaled(px(3.0), density));
            for (action_id, name) in &detail.used_in {
                let action_id = *action_id;
                let title = div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .font_family(mono_family())
                    .text_size(USED_FS)
                    .text_color(palette.text_primary)
                    .child(name.clone());
                col = col.child(
                    row_card(title, palette)
                        .leading(icon(Icon::Bolt, ROW_GLYPH, palette.brand))
                        .idle_background(palette.shell)
                        .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Sm))
                        .padding_xy(scaled(USED_ROW_PAD_V, density), USED_ROW_PAD_H)
                        .on_click(
                            SharedString::from(format!("triggers-used-in-{action_id}")),
                            cx.listener(move |_this, _: &ClickEvent, _, cx| {
                                cx.emit(TriggerPaneEvent::OpenAction(action_id))
                            }),
                        )
                        .into_any_element(),
                );
            }
            col.into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .child(self.section_label(label, None, palette, density))
            .child(inner)
            .into_any_element()
    }
}
