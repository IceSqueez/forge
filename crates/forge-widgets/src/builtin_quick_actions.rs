use forge_platform_core::QuickAction;
use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Column, Row, Space, container, text},
};

use crate::builtin::{card_container, horizontal_divider};
use crate::{
    icons::{Icon, tabler_icon},
    palette::ForgePalette,
    tokens::{BORDER_THIN, Density, FONT_SM, FONT_XS, Radius, Spacing, radius, spacing},
};

pub fn builtin_quick_actions_grid<'a, Msg: Clone + 'a>(
    actions: &'a [QuickAction],
    on_click: impl Fn(usize) -> Msg + 'a,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    builtin_quick_actions_grid_with_hint(actions, on_click, None, palette)
}

pub fn builtin_quick_actions_grid_with_hint<'a, Msg: Clone + 'a>(
    actions: &'a [QuickAction],
    on_click: impl Fn(usize) -> Msg + 'a,
    hint: Option<&'a str>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Md);

    let header = quick_actions_section_header(hint, palette);
    let divider = horizontal_divider(border_color);

    let capped: &[QuickAction] = if actions.len() > 4 {
        &actions[..4]
    } else {
        actions
    };

    let gap = spacing(Spacing::Xs, Density::Cozy) as f32;
    let mut btn_row: Row<'a, Msg> = Row::new().spacing(gap);
    for (i, action) in capped.iter().enumerate() {
        let msg = if action.enabled {
            Some(on_click(i))
        } else {
            None
        };
        btn_row = btn_row.push(quick_action_btn(action, msg, palette));
    }
    for _ in capped.len()..4 {
        btn_row = btn_row.push(Space::new().width(Length::Fill));
    }

    let grid_container = container(btn_row)
        .padding([
            spacing(Spacing::Sm, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
        ])
        .width(Length::Fill);

    card_container(
        Column::new()
            .push(header)
            .push(divider)
            .push(grid_container),
        bg,
        border_color,
        r,
    )
}

fn quick_actions_section_header<'a, Msg: 'a>(
    hint: Option<&'a str>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let icon_elem = tabler_icon(Icon::Bolt, FONT_SM, palette.warning);

    let title_elem = text("Quick actions")
        .size(FONT_SM)
        .color(palette.text_primary);

    let left: Element<'a, Msg> = Row::new()
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(icon_elem)
        .push(title_elem)
        .into();

    let mut outer = Row::new()
        .align_y(Alignment::Center)
        .push(container(left).width(Length::Fill));

    if let Some(h) = hint {
        outer = outer.push(text(h.to_owned()).size(FONT_XS).color(palette.text_faint));
    }

    container(outer)
        .padding([
            spacing(Spacing::Sm, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
        ])
        .width(Length::Fill)
        .into()
}

fn quick_action_btn<'a, Msg: Clone + 'a>(
    action: &'a QuickAction,
    msg: Option<Msg>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let shell = palette.shell;
    let border_color = palette.border_regular;
    let r = radius(Radius::Sm);
    let enabled = msg.is_some();

    let (icon_color, label_color, bg_color, bdr_color) = if enabled {
        (
            palette.text_secondary,
            palette.text_primary,
            shell,
            border_color,
        )
    } else {
        (
            Color {
                a: 0.5,
                ..palette.text_faint
            },
            Color {
                a: 0.5,
                ..palette.text_faint
            },
            Color { a: 0.5, ..shell },
            Color {
                a: 0.5,
                ..border_color
            },
        )
    };

    let icon_elem: Element<'a, Msg> =
        tabler_icon(Icon::from_name(action.icon.as_str()), FONT_SM, icon_color);

    let label_elem: Element<'a, Msg> = text(action.label.clone())
        .size(FONT_SM)
        .color(label_color)
        .into();

    let mut content_row: Row<'a, Msg> = Row::new()
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(icon_elem)
        .push(label_elem);

    if !enabled {
        let na_color = Color {
            a: 0.5,
            ..palette.text_faint
        };
        content_row = content_row
            .push(Space::new().width(Length::Fill))
            .push(text("N/A").size(FONT_XS).color(na_color));
    }

    let mut btn = iced::widget::button(container(content_row).width(Length::Fill))
        .padding([
            spacing(Spacing::Xs, Density::Cozy),
            spacing(Spacing::Sm, Density::Cozy),
        ])
        .width(Length::Fill)
        .style(move |_: &iced::Theme, status| {
            use iced::widget::button::Status;
            let bg = if enabled && matches!(status, Status::Hovered) {
                Color {
                    a: (bg_color.a + 0.06).min(1.0),
                    ..bg_color
                }
            } else {
                bg_color
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: label_color,
                border: Border {
                    color: bdr_color,
                    width: BORDER_THIN,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

    if let Some(m) = msg {
        btn = btn.on_press(m);
    }

    btn.into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn quick_actions_grid_four_enabled_compiles() {
        use forge_platform_core::{QuickAction, SectionIcon};
        use forge_types::SubActionSpec;

        let actions: Vec<QuickAction> = (0..4)
            .map(|i| QuickAction {
                label: format!("Action {i}"),
                icon: SectionIcon::new("b"),
                enabled: true,
                subaction_template: SubActionSpec::Delay { ms: 0 },
                picker: None,
            })
            .collect();

        let _: Element<'_, ()> = builtin_quick_actions_grid(&actions, |_idx| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn quick_actions_grid_mixed_enabled_disabled_compiles() {
        use forge_platform_core::{QuickAction, SectionIcon};
        use forge_types::SubActionSpec;

        let actions = vec![
            QuickAction {
                label: "Resync".to_owned(),
                icon: SectionIcon::new("r"),
                enabled: true,
                subaction_template: SubActionSpec::Delay { ms: 0 },
                picker: None,
            },
            QuickAction {
                label: "Send message".to_owned(),
                icon: SectionIcon::new("s"),
                enabled: false,
                subaction_template: SubActionSpec::Delay { ms: 0 },
                picker: None,
            },
            QuickAction {
                label: "Scrape info".to_owned(),
                icon: SectionIcon::new("i"),
                enabled: true,
                subaction_template: SubActionSpec::Delay { ms: 0 },
                picker: None,
            },
        ];

        let _: Element<'_, ()> = builtin_quick_actions_grid(&actions, |_idx| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn quick_actions_grid_with_hint_compiles() {
        use forge_platform_core::{QuickAction, SectionIcon};
        use forge_types::SubActionSpec;

        let actions: Vec<QuickAction> = (0..4)
            .map(|i| QuickAction {
                label: format!("Action {i}"),
                icon: SectionIcon::new("b"),
                enabled: true,
                subaction_template: SubActionSpec::Delay { ms: 0 },
                picker: None,
            })
            .collect();

        let _: Element<'_, ()> = builtin_quick_actions_grid_with_hint(
            &actions,
            |_idx| (),
            Some("Test commands without writing an action"),
            &CATPPUCCIN_MOCHA,
        );
    }
}
