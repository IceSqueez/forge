use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, HAIRLINE, Icon, Radius,
    Spacing, body_family, icon, mono_family, radius, spacing, tooltip_builder, tr, with_alpha,
};
use forge_platform_core::{QuickAction, QuickActionAccent};
use gpui::{AnyElement, ClickEvent, Context, Rgba, div, prelude::*, px};

use crate::integration_detail::IntegrationDetail;

/// One rendered row of the grouped quick-actions grid.
const GRID_COLUMNS: usize = 3;

impl IntegrationDetail {
    pub(crate) fn quick_actions_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(12.0))
            .py(spacing(Spacing::Sm, density))
            .px(px(14.0))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(icon(Icon::Bolt, FONT_SM, palette.warning))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("widget_quick_actions_title")),
                    ),
            )
            .child(div().flex_1().min_w(px(0.0)))
            .child(
                div()
                    .flex_none()
                    .w(px(240.0))
                    .child(self.qa_search.field().clone()),
            );

        let divider = div().w_full().h(HAIRLINE).bg(palette.border_regular);

        let matches: Vec<(usize, &QuickAction)> = self
            .quick_actions
            .iter()
            .enumerate()
            .filter(|(_, a)| self.qa_search.matches(&a.label))
            .collect();

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .pt(px(4.0))
            .pb(px(12.0))
            .px(px(14.0));

        if matches.is_empty() {
            body = body.child(
                div()
                    .w_full()
                    .py(spacing(Spacing::Lg, density))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("integration_qa_no_matches")),
                    ),
            );
        } else {
            for group in group_order(&matches) {
                body = body.child(self.qa_group(group, &matches, palette, density, cx));
            }
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(header)
            .child(divider)
            .child(body)
            .into_any_element()
    }

    fn qa_group(
        &self,
        group: Option<&str>,
        matches: &[(usize, &QuickAction)],
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let members: Vec<(usize, &QuickAction)> = matches
            .iter()
            .filter(|(_, a)| a.group.as_deref() == group)
            .map(|(i, a)| (*i, *a))
            .collect();

        let mut section = div().w_full().flex().flex_col().gap(px(7.0));
        if let Some(label) = group {
            section = section.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(label.to_uppercase()),
                    )
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(members.len().to_string()),
                    ),
            );
        }

        let mut grid = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        for chunk in members.chunks(GRID_COLUMNS) {
            let mut row = div().w_full().flex().gap(spacing(Spacing::Xs, density));
            for (idx, action) in chunk {
                row = row.child(self.qa_button(*idx, action, palette, density, cx));
            }
            for _ in chunk.len()..GRID_COLUMNS {
                row = row.child(div().flex_1().min_w(px(0.0)));
            }
            grid = grid.child(row);
        }

        section.child(grid).into_any_element()
    }

    fn qa_button(
        &self,
        idx: usize,
        action: &QuickAction,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locked = action.locked_reason.clone();
        let disabled = !action.enabled || locked.is_some();
        let destructive = action.destructive;

        let (icon_color, label_color, border_color) = if disabled {
            (
                with_alpha(palette.text_faint, 0.6),
                with_alpha(palette.text_faint, 0.6),
                with_alpha(palette.border_regular, 0.6),
            )
        } else if destructive {
            (
                palette.random,
                palette.random,
                with_alpha(palette.random, 0.4),
            )
        } else {
            (
                accent_color(action.accent, palette),
                palette.text_primary,
                palette.border_regular,
            )
        };

        let content = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(7.0))
            .child(icon(
                Icon::from_name(action.icon.as_str()),
                FONT_XS,
                icon_color,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(label_color)
                    .child(action.label.clone()),
            );

        let mut btn = div()
            .id(("quick-action", idx))
            .flex_1()
            .min_w(px(0.0))
            .py(px(7.0))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.shell)
            .child(content);

        if disabled {
            let reason = locked.unwrap_or_else(|| tr!("integration_quick_action_na"));
            btn = btn.tooltip(tooltip_builder(reason, palette));
        } else {
            let hover_border = if destructive {
                palette.random
            } else {
                palette.border_active
            };
            btn = btn
                .cursor_pointer()
                .hover(move |s| s.border_color(hover_border))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.on_quick_action(idx, window, cx)
                }));
        }
        btn.into_any_element()
    }
}

/// First-seen order of groups, with ungrouped (`None`) always last so it renders as the
/// single untitled section.
fn group_order<'a>(matches: &[(usize, &'a QuickAction)]) -> Vec<Option<&'a str>> {
    let mut order: Vec<Option<&str>> = Vec::new();
    for (_, action) in matches {
        let key = action.group.as_deref();
        if key.is_some() && !order.contains(&key) {
            order.push(key);
        }
    }
    if matches.iter().any(|(_, a)| a.group.is_none()) {
        order.push(None);
    }
    order
}

fn accent_color(accent: QuickActionAccent, palette: &ForgePalette) -> Rgba {
    match accent {
        QuickActionAccent::Brand => palette.brand,
        QuickActionAccent::Success => palette.success,
        QuickActionAccent::Warning => palette.warning,
        QuickActionAccent::Info => palette.info,
        QuickActionAccent::Bits => palette.bits,
        QuickActionAccent::AccentPinkLight => palette.accent_pink_light,
        QuickActionAccent::Danger => palette.random,
    }
}
