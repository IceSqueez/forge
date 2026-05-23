use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{Space, button, column, container, row, stack, text},
};

use crate::chat::filter_chip;
use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, FONT_MD, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Enabled,
    Disabled,
    Error,
}

#[derive(Debug, Clone)]
pub struct NodeProps<'a, Msg> {
    pub label: &'a str,
    pub status: NodeStatus,
    pub sub_action_count: u16,
    pub selected: bool,
    pub on_press: Msg,
}

#[derive(Debug, Clone)]
pub struct SubActionProps<'a> {
    pub index: u8,
    pub kind_label: &'a str,
    pub icon_char: char,
    pub telemetry: Option<&'a str>,
    pub variable_preview: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct TriggerCardProps<'a, Msg> {
    pub kind_label: &'a str,
    pub icon_char: char,
    pub summary: &'a str,
    pub on_remove: Msg,
}

#[derive(Debug, Clone)]
pub struct ModalProps<'a, Msg> {
    pub title: &'a str,
    pub on_close: Msg,
    pub kbd_hint: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ToggleProps<'a, Msg> {
    pub label: &'a str,
    pub description: &'a str,
    pub value: bool,
    pub on_toggle: Msg,
}

pub(crate) fn node_status_dot_color(status: NodeStatus, palette: &ForgePalette) -> Color {
    match status {
        NodeStatus::Enabled => palette.success,
        NodeStatus::Disabled => palette.border_regular,
        NodeStatus::Error => palette.random,
    }
}

pub(crate) fn toggle_thumb_bg(value: bool, palette: &ForgePalette) -> Color {
    if value {
        palette.brand
    } else {
        palette.surface_overlay
    }
}

pub fn tree_node_with_status<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    props: NodeProps<'a, Msg>,
) -> Element<'a, Msg> {
    let dot_color = node_status_dot_color(props.status, palette);
    let dot = status_dot_small(dot_color);

    let label_color = if props.selected {
        palette.text_primary
    } else if props.status == NodeStatus::Disabled {
        palette.text_faint
    } else {
        palette.text_secondary
    };

    let label_el = text(props.label)
        .size(FONT_SM)
        .color(label_color)
        .font(font(FontRole::Body));

    let count_str = format!("{} sub", props.sub_action_count);
    let count_el = text(count_str)
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(font(FontRole::Monospace));

    let inner = row![dot, label_el, Space::new().width(Length::Fill), count_el]
        .spacing(10)
        .align_y(Alignment::Center);

    let (bg, left_border_color) = if props.selected {
        (Some(Background::Color(palette.elevated)), palette.brand)
    } else {
        (None, Color::TRANSPARENT)
    };

    let btn_radius = radius(Radius::Sm);

    button(inner)
        .on_press(props.on_press)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, _status| button::Style {
            background: bg,
            border: Border {
                color: left_border_color,
                width: if props.selected { 2.0 } else { 0.0 },
                radius: btn_radius.into(),
            },
            text_color: label_color,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

pub fn sub_action_card<'a, Msg: 'a>(
    palette: &'a ForgePalette,
    props: SubActionProps<'a>,
) -> Element<'a, Msg> {
    let icon_color = palette.brand;
    let icon_el = container(
        text(props.icon_char.to_string())
            .size(FONT_SM)
            .color(icon_color)
            .font(font(FontRole::Body)),
    )
    .width(28)
    .height(28)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let kind_el = text(props.kind_label)
        .size(FONT_SM)
        .color(palette.text_primary)
        .font(font(FontRole::Body));

    let mut label_col = column![kind_el].spacing(2);

    if let Some(preview) = props.variable_preview {
        let preview_el = text(preview)
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace));
        label_col = label_col.push(preview_el);
    }

    let mut main_row_items: Vec<Element<'a, Msg>> = vec![
        icon_el.into(),
        container(label_col).width(Length::Fill).into(),
    ];

    if let Some(tel) = props.telemetry {
        let tel_el = container(
            text(tel)
                .size(FONT_XS)
                .color(palette.success)
                .font(font(FontRole::Monospace)),
        )
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });
        main_row_items.push(tel_el.into());
    }

    let main_row = row(main_row_items).spacing(8).align_y(Alignment::Center);

    container(main_row)
        .width(Length::Fill)
        .padding(Padding {
            top: spf(Spacing::Xs),
            right: spf(Spacing::Sm),
            bottom: spf(Spacing::Xs),
            left: spf(Spacing::Sm),
        })
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn variable_chip<'a, Msg: 'a>(palette: &'a ForgePalette, name: &str) -> Element<'a, Msg> {
    let label = format!("%{}%", name);
    container(
        text(label)
            .size(FONT_SM)
            .color(palette.warning)
            .font(font(FontRole::Monospace)),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    })
    .into()
}

pub fn trigger_card<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    props: TriggerCardProps<'a, Msg>,
) -> Element<'a, Msg> {
    let icon_el = container(
        text(props.icon_char.to_string())
            .size(FONT_SM)
            .color(palette.brand)
            .font(font(FontRole::Body)),
    )
    .width(26)
    .height(26)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let kind_el = text(props.kind_label)
        .size(FONT_SM)
        .color(palette.text_primary)
        .font(font(FontRole::Body));

    let summary_el = text(props.summary)
        .size(FONT_XS)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let label_col = column![kind_el, summary_el].spacing(1);

    let remove_msg = props.on_remove.clone();
    let remove_btn = button(tabler_icon(Icon::X, FONT_SM, palette.text_faint))
        .on_press(remove_msg)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
        .style(|_theme: &iced::Theme, _status| button::Style {
            background: None,
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let inner = row![
        icon_el,
        container(label_col).width(Length::Fill),
        remove_btn,
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding(Padding {
            top: spf(Spacing::Xs),
            right: spf(Spacing::Xs),
            bottom: spf(Spacing::Xs),
            left: spf(Spacing::Xs),
        })
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn modal<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    props: ModalProps<'a, Msg>,
    body: Element<'a, Msg>,
    footer: Element<'a, Msg>,
) -> Element<'a, Msg> {
    let close_msg = props.on_close.clone();

    let title_el = text(props.title)
        .size(FONT_MD)
        .color(palette.text_primary)
        .font(font(FontRole::Body));

    let close_btn = button(tabler_icon(Icon::X, FONT_MD, palette.text_faint))
        .on_press(props.on_close.clone())
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
        .style(|_theme: &iced::Theme, _status| button::Style {
            background: None,
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let header_row = row![container(title_el).width(Length::Fill), close_btn,]
        .align_y(Alignment::Center)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    let header_container =
        container(header_row)
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                border: Border {
                    color: palette.border_regular,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            });

    let body_container = container(body).padding([sp(Spacing::Md), sp(Spacing::Md)]);

    let mut footer_col = column![footer].spacing(6);
    if let Some(hint) = props.kbd_hint {
        let hint_el = text(hint)
            .size(FONT_XS)
            .color(palette.text_faint)
            .font(font(FontRole::Monospace));
        footer_col = footer_col.push(hint_el);
    }

    let footer_container = container(footer_col)
        .width(Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.shell)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let card_content = column![header_container, body_container, footer_container];

    let card = container(card_content)
        .max_width(560)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    let backdrop_dismiss = close_msg;
    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(backdrop_dismiss)
        .padding(0)
        .style(|_theme: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.6,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let centered_card = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    stack![backdrop, centered_card].into()
}

pub fn toggle<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    props: ToggleProps<'a, Msg>,
) -> Element<'a, Msg> {
    let track_bg = toggle_thumb_bg(props.value, palette);
    let thumb_color = palette.text_primary;

    let track_width = 36_f32;
    let track_height = 20_f32;
    let thumb_size = 16_f32;
    let thumb_offset = if props.value {
        track_width - thumb_size - 2.0
    } else {
        2.0
    };

    let thumb = container(Space::new().width(thumb_size).height(thumb_size))
        .width(thumb_size)
        .height(thumb_size)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(thumb_color)),
            border: Border {
                radius: (thumb_size / 2.0).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let thumb_padded = container(thumb).padding(Padding {
        top: spf(Spacing::Xxs),
        right: 0.0,
        bottom: 0.0,
        left: thumb_offset,
    });

    let track = container(thumb_padded)
        .width(track_width)
        .height(track_height)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(track_bg)),
            border: Border {
                radius: (track_height / 2.0).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let label_el = text(props.label)
        .size(FONT_SM)
        .color(palette.text_primary)
        .font(font(FontRole::Body));

    let desc_el = text(props.description)
        .size(FONT_SM)
        .color(palette.text_faint)
        .font(font(FontRole::Body));

    let label_col = column![label_el, desc_el].spacing(2);

    let inner = row![container(label_col).width(Length::Fill), track,]
        .spacing(12)
        .align_y(Alignment::Center);

    button(inner)
        .on_press(props.on_toggle)
        .padding([sp(Spacing::Xs), 0])
        .width(Length::Fill)
        .style(|_theme: &iced::Theme, _status| button::Style {
            background: None,
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

/// Re-exports `filter_chip` under a semantically appropriate name for trigger modal category
/// filtering. The visual contract matches: dot + label, pill shape, active/inactive bg.
pub fn category_chip<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    label: &str,
    dot_color: Color,
    active: bool,
    on_press: Msg,
) -> Element<'a, Msg> {
    filter_chip(palette, label, dot_color, active, on_press)
}

fn status_dot_small<'a, Msg: 'a>(color: Color) -> Element<'a, Msg> {
    let size = 6.0_f32;
    container(Space::new().width(size).height(size))
        .width(size)
        .height(size)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: (size / 2.0).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn enabled_status_dot_maps_to_success() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(node_status_dot_color(NodeStatus::Enabled, &p), p.success);
    }

    #[test]
    fn disabled_status_dot_maps_to_border_regular() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(
            node_status_dot_color(NodeStatus::Disabled, &p),
            p.border_regular
        );
    }

    #[test]
    fn error_status_dot_maps_to_random() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(node_status_dot_color(NodeStatus::Error, &p), p.random);
    }

    #[test]
    fn toggle_off_returns_surface_overlay() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(toggle_thumb_bg(false, &p), p.surface_overlay);
    }

    #[test]
    fn toggle_on_returns_brand() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(toggle_thumb_bg(true, &p), p.brand);
    }

    #[test]
    fn tree_node_with_status_enabled_selected_compiles() {
        let _: Element<'_, ()> = tree_node_with_status(
            &CATPPUCCIN_MOCHA,
            NodeProps {
                label: "!quote",
                status: NodeStatus::Enabled,
                sub_action_count: 5,
                selected: true,
                on_press: (),
            },
        );
    }

    #[test]
    fn tree_node_with_status_disabled_unselected_compiles() {
        let _: Element<'_, ()> = tree_node_with_status(
            &CATPPUCCIN_MOCHA,
            NodeProps {
                label: "!stats",
                status: NodeStatus::Disabled,
                sub_action_count: 3,
                selected: false,
                on_press: (),
            },
        );
    }

    #[test]
    fn sub_action_card_full_props_compiles() {
        let _: Element<'_, ()> = sub_action_card(
            &CATPPUCCIN_MOCHA,
            SubActionProps {
                index: 3,
                kind_label: "Run Rhai script",
                icon_char: '\u{ea77}',
                telemetry: Some("2 ms avg"),
                variable_preview: Some("%lines% → %quote%"),
            },
        );
    }

    #[test]
    fn sub_action_card_minimal_props_compiles() {
        let _: Element<'_, ()> = sub_action_card(
            &CATPPUCCIN_MOCHA,
            SubActionProps {
                index: 1,
                kind_label: "Send Twitch message",
                icon_char: '\u{ea21}',
                telemetry: None,
                variable_preview: None,
            },
        );
    }

    #[test]
    fn variable_chip_compiles() {
        let _: Element<'_, ()> = variable_chip(&CATPPUCCIN_MOCHA, "user");
    }

    #[test]
    fn trigger_card_compiles() {
        let _: Element<'_, ()> = trigger_card(
            &CATPPUCCIN_MOCHA,
            TriggerCardProps {
                kind_label: "Twitch \u{00b7} Chat command",
                icon_char: '\u{ea21}',
                summary: "!quote \u{00b7} cooldown 5s \u{00b7} everyone",
                on_remove: (),
            },
        );
    }

    #[test]
    fn modal_compiles() {
        let body: Element<'_, ()> = text("body").into();
        let footer: Element<'_, ()> = text("footer").into();
        let _: Element<'_, ()> = modal(
            &CATPPUCCIN_MOCHA,
            ModalProps {
                title: "Add trigger",
                on_close: (),
                kbd_hint: Some("ESC to cancel"),
            },
            body,
            footer,
        );
    }

    #[test]
    fn toggle_on_compiles() {
        let _: Element<'_, ()> = toggle(
            &CATPPUCCIN_MOCHA,
            ToggleProps {
                label: "Enabled",
                description: "Action will run on trigger",
                value: true,
                on_toggle: (),
            },
        );
    }

    #[test]
    fn toggle_off_compiles() {
        let _: Element<'_, ()> = toggle(
            &CATPPUCCIN_MOCHA,
            ToggleProps {
                label: "Concurrent execution",
                description: "Allow parallel runs in this queue",
                value: false,
                on_toggle: (),
            },
        );
    }

    #[test]
    fn category_chip_active_compiles() {
        let _: Element<'_, ()> = category_chip(
            &CATPPUCCIN_MOCHA,
            "Twitch",
            CATPPUCCIN_MOCHA.brand,
            true,
            (),
        );
    }

    #[test]
    fn category_chip_inactive_compiles() {
        let _: Element<'_, ()> = category_chip(
            &CATPPUCCIN_MOCHA,
            "YouTube",
            CATPPUCCIN_MOCHA.random,
            false,
            (),
        );
    }
}
