use iced::{
    Border, Color, Element,
    widget::button::Status as ButtonStatus,
    widget::button::Style as ButtonStyle,
    widget::{Space, button, column, container, row, scrollable, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};

pub const SIDEBAR_WIDTH: u16 = 200;

pub struct Sidebar<'a, Msg> {
    pub items: Vec<NavItem<'a, Msg>>,
    pub bottom_items: Vec<NavItem<'a, Msg>>,
}

pub enum NavItem<'a, Msg> {
    Section(&'a str),
    MiniLabel(&'a str),
    Leaf {
        icon: Icon,
        label: &'a str,
        active: bool,
        on_press: Msg,
    },
    FlatLink {
        dot_color: Color,
        label: &'a str,
        active: bool,
        on_press: Msg,
    },
    Group {
        icon: Icon,
        label: &'a str,
        active: bool,
        expanded: bool,
        on_toggle: Msg,
        children: Vec<NavChild<'a, Msg>>,
    },
    Divider,
}

pub struct NavChild<'a, Msg> {
    pub dot_color: Color,
    pub label: &'a str,
    pub active: bool,
    pub on_press: Msg,
}

pub fn sidebar<'a, Msg: 'a + Clone>(
    palette: &'a ForgePalette,
    props: Sidebar<'a, Msg>,
) -> Element<'a, Msg> {
    let bg = palette.shell;
    let border_color = palette.border_regular;

    let items: Vec<Element<'a, Msg>> = props
        .items
        .into_iter()
        .map(|item| render_nav_item(item, palette))
        .collect();
    let bottom: Vec<Element<'a, Msg>> = props
        .bottom_items
        .into_iter()
        .map(|item| render_nav_item(item, palette))
        .collect();

    let main = column(items).spacing(2);
    let bottom_col = column(bottom).spacing(2);

    let body = column![scrollable(main).height(iced::Length::Fill), bottom_col,];

    container(body)
        .width(u32::from(SIDEBAR_WIDTH))
        .height(iced::Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Xs)])
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn render_nav_item<'a, Msg: 'a + Clone>(
    item: NavItem<'a, Msg>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    match item {
        NavItem::Section(label) => nav_section_label(label, palette),
        NavItem::MiniLabel(label) => nav_mini_label(label, palette),
        NavItem::Leaf {
            icon,
            label,
            active,
            on_press,
        } => nav_leaf(icon, label, active, on_press, palette),
        NavItem::FlatLink {
            dot_color,
            label,
            active,
            on_press,
        } => nav_flat_link(dot_color, label, active, on_press, palette),
        NavItem::Group {
            icon,
            label,
            active,
            expanded,
            on_toggle,
            children,
        } => {
            let header = nav_group_header(
                icon,
                label,
                active || expanded,
                expanded,
                on_toggle,
                palette,
            );
            if expanded && !children.is_empty() {
                let mut col = column![header].spacing(2);
                for child in children {
                    col = col.push(nav_child_row(child, palette));
                }
                col.into()
            } else {
                header
            }
        }
        NavItem::Divider => nav_divider(palette.border_regular),
    }
}

fn nav_section_label<'a, Msg: 'a>(label: &'a str, palette: &ForgePalette) -> Element<'a, Msg> {
    let color = palette.text_faint;
    container(
        text(label)
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(color),
    )
    .padding(iced::Padding {
        top: spf(Spacing::Md),
        bottom: spf(Spacing::Xs),
        left: spf(Spacing::Sm),
        right: spf(Spacing::Sm),
    })
    .width(iced::Length::Fill)
    .into()
}

fn nav_mini_label<'a, Msg: 'a>(label: &'a str, palette: &ForgePalette) -> Element<'a, Msg> {
    container(
        text(label.to_ascii_uppercase())
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(palette.text_faint),
    )
    .padding(iced::Padding {
        top: spf(Spacing::Xs),
        bottom: spf(Spacing::Xxs),
        left: spf(Spacing::Sm),
        right: spf(Spacing::Sm),
    })
    .width(iced::Length::Fill)
    .into()
}

fn nav_flat_link<'a, Msg: 'a + Clone>(
    dot_color: Color,
    label: &'a str,
    active: bool,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let text_color = if active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    let bg = if active {
        Some(iced::Background::Color(palette.surface_overlay))
    } else {
        None
    };
    let hover_bg = palette.base;
    let hover_text = palette.text_primary;
    let btn_radius = radius(Radius::Sm);

    let dot = container(Space::new())
        .width(8_u32)
        .height(8_u32)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(dot_color)),
            border: Border {
                radius: 2.0.into(),
                ..Border::default()
            },
            ..Default::default()
        });

    let content = row![dot, text(label).size(FONT_XS)]
        .spacing(10)
        .align_y(iced::Alignment::Center);

    button(content)
        .on_press(on_press)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(iced::Length::Fill)
        .style(move |_theme: &iced::Theme, status| match status {
            ButtonStatus::Hovered | ButtonStatus::Pressed if !active => ButtonStyle {
                background: Some(iced::Background::Color(hover_bg)),
                text_color: hover_text,
                border: Border {
                    radius: btn_radius.into(),
                    ..Border::default()
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            _ => ButtonStyle {
                background: bg,
                text_color,
                border: Border {
                    radius: btn_radius.into(),
                    ..Border::default()
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

fn nav_leaf<'a, Msg: 'a + Clone>(
    icon: Icon,
    label: &'a str,
    active: bool,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let icon_color = if active {
        palette.brand
    } else {
        palette.text_secondary
    };
    let text_color = if active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    let bg = if active {
        Some(iced::Background::Color(palette.surface_overlay))
    } else {
        None
    };
    let hover_bg = Color {
        a: 0.5,
        ..palette.surface_overlay
    };
    let hover_text = palette.text_primary;
    let btn_radius = radius(Radius::Sm);

    let content = row![
        tabler_icon(icon, 15.0, icon_color),
        text(label).size(FONT_SM),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    button(content)
        .on_press(on_press)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(iced::Length::Fill)
        .style(move |_theme: &iced::Theme, status| match status {
            ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
                background: Some(iced::Background::Color(hover_bg)),
                text_color: hover_text,
                border: Border {
                    radius: btn_radius.into(),
                    ..Border::default()
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            _ => ButtonStyle {
                background: bg,
                text_color,
                border: Border {
                    radius: if active {
                        btn_radius.into()
                    } else {
                        0.0.into()
                    },
                    ..Border::default()
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

fn nav_group_header<'a, Msg: 'a + Clone>(
    icon: Icon,
    label: &'a str,
    highlighted: bool,
    expanded: bool,
    on_toggle: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let icon_color = if highlighted {
        palette.brand
    } else {
        palette.text_secondary
    };
    let text_color = if highlighted {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    let chevron_color = palette.text_secondary;
    let chevron = if expanded {
        Icon::ChevronUp
    } else {
        Icon::ChevronDown
    };
    let hover_bg = Color {
        a: 0.5,
        ..palette.surface_overlay
    };
    let hover_text = palette.text_primary;
    let btn_radius = radius(Radius::Sm);

    let content = row![
        tabler_icon(icon, 15.0, icon_color),
        text(label).size(FONT_SM),
        Space::new().width(iced::Length::Fill),
        tabler_icon(chevron, 13.0, chevron_color),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    button(content)
        .on_press(on_toggle)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(iced::Length::Fill)
        .style(move |_theme: &iced::Theme, status| match status {
            ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
                background: Some(iced::Background::Color(hover_bg)),
                text_color: hover_text,
                border: Border {
                    radius: btn_radius.into(),
                    ..Border::default()
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            _ => ButtonStyle {
                background: None,
                text_color,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

fn nav_child_row<'a, Msg: 'a + Clone>(
    child: NavChild<'a, Msg>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let text_color = if child.active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    let bg = if child.active {
        Some(iced::Background::Color(palette.surface_overlay))
    } else {
        None
    };
    let hover_bg = Color {
        a: 0.5,
        ..palette.surface_overlay
    };
    let hover_text = palette.text_primary;
    let btn_radius = radius(Radius::Sm);
    let dot_color = child.dot_color;
    let child_active = child.active;

    let dot = container(Space::new())
        .width(8_u32)
        .height(8_u32)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(dot_color)),
            border: Border {
                radius: 2.0.into(),
                ..Border::default()
            },
            ..Default::default()
        });

    let content = row![dot, text(child.label).size(FONT_SM)]
        .spacing(10)
        .align_y(iced::Alignment::Center);

    let child_btn = button(content)
        .on_press(child.on_press)
        .padding([6, 10])
        .width(iced::Length::Fill)
        .style(move |_theme: &iced::Theme, status| match status {
            ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
                background: Some(iced::Background::Color(hover_bg)),
                text_color: hover_text,
                border: Border {
                    radius: btn_radius.into(),
                    ..Border::default()
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            _ => ButtonStyle {
                background: bg,
                text_color,
                border: Border {
                    radius: if child_active {
                        btn_radius.into()
                    } else {
                        0.0.into()
                    },
                    ..Border::default()
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        });

    row![Space::new().width(18_u32), child_btn]
        .align_y(iced::Alignment::Center)
        .into()
}

fn nav_divider<'a, Msg: 'a>(border_color: Color) -> Element<'a, Msg> {
    use iced::widget::rule;

    container(
        rule::horizontal(1.0_f32).style(move |_: &iced::Theme| rule::Style {
            color: border_color,
            radius: 0.0.into(),
            fill_mode: rule::FillMode::Full,
            snap: true,
        }),
    )
    .padding(iced::Padding {
        top: spf(Spacing::Sm),
        right: spf(Spacing::Xs),
        bottom: 0.0,
        left: spf(Spacing::Xs),
    })
    .width(iced::Length::Fill)
    .into()
}
