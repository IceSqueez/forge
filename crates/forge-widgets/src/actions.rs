use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{Space, button, column, container, row, text},
};

use crate::chat::filter_chip;
use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::semantic::SemanticState;
use crate::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
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

pub(crate) fn node_status_dot_color(status: NodeStatus, palette: &ForgePalette) -> Color {
    let state = match status {
        NodeStatus::Enabled => SemanticState::Enabled,
        NodeStatus::Disabled => SemanticState::Disabled,
        NodeStatus::Error => SemanticState::Error,
    };
    state.color(palette)
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
