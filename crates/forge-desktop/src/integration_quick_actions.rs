use forge_components::FONT_LABEL;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    Radius, Spacing, body_family, icon, mono_family, pulse_dot, radius, scaled, spacing,
    tooltip_builder, tr, with_alpha,
};
use forge_platform_core::{QuickAction, QuickActionAccent, QuickActionGroup, QuickActionLiveness};
use gpui::{AnyElement, ClickEvent, Context, Pixels, Rgba, div, prelude::*, px};

use crate::integration_detail::IntegrationDetail;
use crate::user_text::user_text_string;

const GRID_COLUMNS: u16 = 3;
const LIVE_DOT: Pixels = px(5.0);

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
            .gap(scaled(px(12.0), density))
            .py(spacing(Spacing::Sm, density))
            .px(scaled(px(14.0), density))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(scaled(px(7.0), density))
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
            )
            .child(self.history_button(palette, cx));

        let divider = div().w_full().h(BORDER_THIN).bg(palette.border_regular);

        let matches: Vec<(usize, &QuickAction)> = self
            .quick_actions
            .iter()
            .enumerate()
            .filter(|(_, a)| self.qa_search.matches(&user_text_string(&a.label)))
            .collect();

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(px(12.0), density))
            .pt(spacing(Spacing::Xxs, density))
            .pb(scaled(px(12.0), density))
            .px(scaled(px(14.0), density));

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

    fn history_button(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let text_color = palette.text_secondary;
        let hover_border = palette.border_input;
        div()
            .id("integration-run-history")
            .flex_none()
            .flex()
            .items_center()
            .gap(scaled(px(5.0), density))
            .py(scaled(px(5.0), density))
            .px(scaled(px(11.0), density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_run_history(cx)))
            .child(icon(Icon::History, FONT_XS, text_color))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text_color)
                    .child(tr!("integration_run_history")),
            )
            .into_any_element()
    }

    fn qa_group(
        &self,
        group: Option<&QuickActionGroup>,
        matches: &[(usize, &QuickAction)],
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let members: Vec<(usize, &QuickAction)> = matches
            .iter()
            .filter(|(_, a)| a.group.as_ref() == group)
            .map(|(i, a)| (*i, *a))
            .collect();

        let mut section = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(px(7.0), density));
        if let Some(group) = group {
            let mut label_row = div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density));
            label_row = label_row.child(icon(
                Icon::from_name(group.icon.as_str()),
                FONT_XXS,
                accent_color(group.accent, palette),
            ));
            label_row = label_row.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(user_text_string(&group.label).to_uppercase()),
            );
            section = section.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(label_row)
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
            .grid()
            .grid_cols(GRID_COLUMNS)
            .gap(spacing(Spacing::Xs, density));
        for (idx, action) in &members {
            grid = grid.child(self.qa_button(*idx, action, palette, density, cx));
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
        let locked = action.locked_reason.as_ref().map(user_text_string);
        let disabled = !action.is_runnable();
        let destructive = action.destructive;
        let live = action.liveness == QuickActionLiveness::Live;

        let (icon_color, label_color, border_color) = if disabled {
            (
                with_alpha(palette.text_faint, 0.6),
                with_alpha(palette.text_faint, 0.6),
                with_alpha(palette.border_regular, 0.6),
            )
        } else if destructive {
            (
                palette.danger,
                palette.danger,
                with_alpha(palette.danger, 0.4),
            )
        } else {
            (
                accent_color(action.accent, palette),
                palette.text_primary,
                palette.border_regular,
            )
        };

        let mut content = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(px(7.0), density))
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
                    .text_size(FONT_LABEL)
                    .line_height(px(14.0))
                    .text_color(label_color)
                    .child(user_text_string(&action.label)),
            );
        if locked.is_some() {
            content = content.child(icon(Icon::Lock, FONT_XXS, icon_color));
        }
        if live {
            content = content.child(pulse_dot(
                ("quick-action-live", idx),
                palette.danger,
                LIVE_DOT,
            ));
        }

        let mut btn = div()
            .id(("quick-action", idx))
            .min_w(px(0.0))
            .py(scaled(px(7.0), density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.shell)
            .child(content);

        if disabled {
            let reason = locked.unwrap_or_else(|| {
                if action.liveness == QuickActionLiveness::Absent {
                    tr!("integration_quick_action_not_live")
                } else {
                    tr!("integration_quick_action_na")
                }
            });
            btn = btn.tooltip(tooltip_builder(reason, palette));
        } else {
            let hover_border = if destructive {
                palette.danger
            } else {
                palette.border_input
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

fn group_order<'a>(matches: &[(usize, &'a QuickAction)]) -> Vec<Option<&'a QuickActionGroup>> {
    let mut order: Vec<Option<&QuickActionGroup>> = Vec::new();
    for (_, action) in matches {
        let key = action.group.as_ref();
        if key.is_some() && !order.contains(&key) {
            order.push(key);
        }
    }
    if matches.iter().any(|(_, a)| a.group.is_none()) {
        order.push(None);
    }
    order
}

pub(crate) fn accent_color(accent: QuickActionAccent, palette: &ForgePalette) -> Rgba {
    match accent {
        QuickActionAccent::Brand => palette.brand,
        QuickActionAccent::Success => palette.success,
        QuickActionAccent::Warning => palette.warning,
        QuickActionAccent::Info => palette.info,
        QuickActionAccent::Bits => palette.accent_peach,
        QuickActionAccent::AccentPinkLight => palette.accent_pink_light,
        QuickActionAccent::Danger => palette.danger,
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{QuickActionGroup, QuickActionLiveness, SectionIcon};
    use forge_types::SubActionStep;

    use super::*;

    fn group(name: &str) -> QuickActionGroup {
        QuickActionGroup {
            label: forge_types::VerbatimText::new(name).into(),
            icon: SectionIcon::new("dot"),
            accent: QuickActionAccent::Brand,
        }
    }

    fn action(group: Option<QuickActionGroup>) -> QuickAction {
        QuickAction {
            label: forge_types::VerbatimText::new("action").into(),
            icon: SectionIcon::new("dot"),
            enabled: true,
            locked_reason: None,
            liveness: QuickActionLiveness::Unknown,
            group,
            destructive: false,
            accent: QuickActionAccent::Brand,
            subaction_template: SubActionStep {
                kind_id: "test.kind".to_owned(),
                config: Default::default(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            },
            fields: Vec::new(),
            collection: None,
        }
    }

    #[test]
    fn actions_of_one_group_fold_into_one_section_in_first_seen_order_with_ungrouped_last() {
        let actions = [
            action(None),
            action(Some(group("Polls"))),
            action(Some(group("Chat"))),
            action(Some(group("Polls"))),
            action(None),
            action(Some(group("Chat"))),
        ];
        let matches: Vec<(usize, &QuickAction)> = actions.iter().enumerate().collect();

        assert_eq!(
            group_order(&matches),
            vec![Some(&group("Polls")), Some(&group("Chat")), None]
        );
    }

    #[test]
    fn a_list_without_groups_has_a_single_ungrouped_section() {
        let actions = [action(None), action(None)];
        let matches: Vec<(usize, &QuickAction)> = actions.iter().enumerate().collect();

        assert_eq!(group_order(&matches), vec![None]);
    }
}
