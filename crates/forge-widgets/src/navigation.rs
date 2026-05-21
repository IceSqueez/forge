use iced::{
    Border, Color, Element,
    widget::button::Status as ButtonStatus,
    widget::button::Style as ButtonStyle,
    widget::{Space, button, column, container, row, scrollable, text},
};

use crate::icons::{BOOTSTRAP_FONT, ICON_CHEVRON_DOWN, ICON_CHEVRON_UP};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, FONT_BODY, FONT_SM, FONT_XS, FontRole, Radius, font, radius};

pub const SIDEBAR_WIDTH: u16 = 200;

pub fn sidebar<'a, Msg: 'a>(
    sections: Vec<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.shell;
    let border_color = palette.border_regular;
    let content = column(sections).spacing(4);

    container(scrollable(content).height(iced::Length::Fill))
        .width(u32::from(SIDEBAR_WIDTH))
        .height(iced::Length::Fill)
        .padding(12)
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

pub fn sidebar_section<'a, Msg: 'a>(
    title: &str,
    items: Vec<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let header_color = palette.text_faint;
    let header = text(title.to_uppercase())
        .size(11)
        .font(font(FontRole::Monospace))
        .color(header_color);

    let mut col = column![header].spacing(2).padding([0_u16, 4]);
    for item in items {
        col = col.push(item);
    }

    col.into()
}

pub fn tree_node<'a, Msg: 'a + Clone>(
    label: &'a str,
    depth: u8,
    expanded: bool,
    on_toggle: Option<Msg>,
    on_select: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let indent = f32::from(depth) * 16.0;
    let text_color = palette.text_secondary;
    let text_hover = palette.text_primary;
    let bg_hover = Color {
        a: 0.08,
        ..palette.brand
    };
    let hover_radius = radius(Radius::Sm);

    let chevron: Element<'_, Msg> = match on_toggle {
        Some(toggle_msg) => {
            let ch = if expanded { '▾' } else { '▸' };
            let ch_color = palette.text_muted;
            let ch_hover = palette.text_secondary;
            button(text(ch.to_string()).size(12).color(ch_color))
                .on_press(toggle_msg)
                .padding([2, 4])
                .style(move |_theme: &iced::Theme, status| match status {
                    ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
                        background: None,
                        text_color: ch_hover,
                        border: Border::default(),
                        shadow: iced::Shadow::default(),
                        snap: false,
                    },
                    _ => ButtonStyle {
                        background: None,
                        text_color: ch_color,
                        border: Border::default(),
                        shadow: iced::Shadow::default(),
                        snap: false,
                    },
                })
                .into()
        }
        None => Space::new().width(16).into(),
    };

    let label_text = text(label).size(13).color(text_color);
    let inner = row![chevron, label_text]
        .spacing(2)
        .align_y(iced::Alignment::Center);

    let row_with_indent = row![Space::new().width(indent), inner].align_y(iced::Alignment::Center);

    button(row_with_indent)
        .on_press(on_select)
        .padding([4, 6])
        .width(iced::Length::Fill)
        .style(move |_theme: &iced::Theme, status| match status {
            ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
                background: Some(iced::Background::Color(bg_hover)),
                text_color: text_hover,
                border: Border {
                    radius: hover_radius.into(),
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

pub struct SidebarV2<'a, Msg> {
    pub items: Vec<NavItem<'a, Msg>>,
}

pub enum NavItem<'a, Msg> {
    Section(&'a str),
    Leaf {
        icon: char,
        label: &'a str,
        active: bool,
        on_press: Msg,
    },
    Group {
        icon: char,
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

pub fn sidebar_v2<'a, Msg: 'a + Clone>(
    palette: &'a ForgePalette,
    props: SidebarV2<'a, Msg>,
) -> Element<'a, Msg> {
    let bg = palette.shell;
    let border_color = palette.border_regular;

    let items: Vec<Element<'a, Msg>> = props
        .items
        .into_iter()
        .map(|item| render_nav_item(item, palette))
        .collect();

    let content = column(items).spacing(2);

    container(scrollable(content).height(iced::Length::Fill))
        .width(u32::from(SIDEBAR_WIDTH))
        .height(iced::Length::Fill)
        .padding([12, 8])
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
        NavItem::Leaf {
            icon,
            label,
            active,
            on_press,
        } => nav_leaf(icon, label, active, on_press, palette),
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
        text(label.to_uppercase())
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(color),
    )
    .padding(iced::Padding {
        top: 14.0,
        bottom: 6.0,
        left: 10.0,
        right: 10.0,
    })
    .width(iced::Length::Fill)
    .into()
}

fn nav_leaf<'a, Msg: 'a + Clone>(
    icon: char,
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
        text(icon.to_string())
            .size(15.0)
            .font(BOOTSTRAP_FONT)
            .color(icon_color),
        text(label).size(FONT_BODY),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    button(content)
        .on_press(on_press)
        .padding([8, 10])
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
    icon: char,
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
        ICON_CHEVRON_UP
    } else {
        ICON_CHEVRON_DOWN
    };
    let hover_bg = Color {
        a: 0.5,
        ..palette.surface_overlay
    };
    let hover_text = palette.text_primary;
    let btn_radius = radius(Radius::Sm);

    let content = row![
        text(icon.to_string())
            .size(15.0)
            .font(BOOTSTRAP_FONT)
            .color(icon_color),
        text(label).size(FONT_BODY),
        Space::new().width(iced::Length::Fill),
        text(chevron.to_string())
            .size(13.0)
            .font(BOOTSTRAP_FONT)
            .color(chevron_color),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    button(content)
        .on_press(on_toggle)
        .padding([8, 10])
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
        top: 10.0,
        right: 6.0,
        bottom: 0.0,
        left: 6.0,
    })
    .width(iced::Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use crate::tokens::radius;

    #[test]
    fn sidebar_width_constant_matches_design() {
        assert_eq!(SIDEBAR_WIDTH, 200);
    }

    #[test]
    fn tree_node_hover_radius_matches_design() {
        assert_eq!(radius(Radius::Sm), 6.0);
    }

    #[test]
    fn sidebar_builds_with_unit_msg() {
        let _: Element<'_, ()> = sidebar(vec![], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn sidebar_section_builds_with_unit_msg() {
        let _: Element<'_, ()> = sidebar_section("Platforms", vec![], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn tree_node_collapsed_no_toggle() {
        let _: Element<'_, ()> = tree_node("Actions", 0, false, None, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn tree_node_expanded_with_toggle() {
        let _: Element<'_, ()> = tree_node("Triggers", 1, true, Some(()), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn tree_node_deep_indent() {
        let _: Element<'_, ()> = tree_node("Sub-action", 3, false, None, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn sidebar_v2_builds_empty() {
        let _: Element<'_, ()> = sidebar_v2(&CATPPUCCIN_MOCHA, SidebarV2 { items: vec![] });
    }

    #[test]
    fn sidebar_v2_leaf_inactive_builds() {
        let _: Element<'_, ()> = sidebar_v2(
            &CATPPUCCIN_MOCHA,
            SidebarV2 {
                items: vec![NavItem::Leaf {
                    icon: crate::icons::ICON_HOME,
                    label: "Home",
                    active: false,
                    on_press: (),
                }],
            },
        );
    }

    #[test]
    fn sidebar_v2_leaf_active_builds() {
        let _: Element<'_, ()> = sidebar_v2(
            &CATPPUCCIN_MOCHA,
            SidebarV2 {
                items: vec![NavItem::Leaf {
                    icon: crate::icons::ICON_HOME,
                    label: "Home",
                    active: true,
                    on_press: (),
                }],
            },
        );
    }

    #[test]
    fn sidebar_v2_group_collapsed_builds() {
        let _: Element<'_, ()> = sidebar_v2(
            &CATPPUCCIN_MOCHA,
            SidebarV2 {
                items: vec![NavItem::Group {
                    icon: crate::icons::ICON_BROADCAST,
                    label: "Platforms",
                    active: false,
                    expanded: false,
                    on_toggle: (),
                    children: vec![],
                }],
            },
        );
    }

    #[test]
    fn sidebar_v2_group_expanded_with_children_builds() {
        let _: Element<'_, ()> = sidebar_v2(
            &CATPPUCCIN_MOCHA,
            SidebarV2 {
                items: vec![NavItem::Group {
                    icon: crate::icons::ICON_BROADCAST,
                    label: "Platforms",
                    active: false,
                    expanded: true,
                    on_toggle: (),
                    children: vec![
                        NavChild {
                            dot_color: CATPPUCCIN_MOCHA.brand,
                            label: "Twitch",
                            active: false,
                            on_press: (),
                        },
                        NavChild {
                            dot_color: CATPPUCCIN_MOCHA.random,
                            label: "YouTube",
                            active: false,
                            on_press: (),
                        },
                    ],
                }],
            },
        );
    }

    #[test]
    fn sidebar_v2_divider_builds() {
        let _: Element<'_, ()> = sidebar_v2(
            &CATPPUCCIN_MOCHA,
            SidebarV2 {
                items: vec![
                    NavItem::Leaf {
                        icon: crate::icons::ICON_PEOPLE,
                        label: "Viewers",
                        active: false,
                        on_press: (),
                    },
                    NavItem::Divider,
                    NavItem::Leaf {
                        icon: crate::icons::ICON_GEAR,
                        label: "Settings",
                        active: false,
                        on_press: (),
                    },
                ],
            },
        );
    }

    #[test]
    fn sidebar_v2_active_child_builds() {
        let _: Element<'_, ()> = sidebar_v2(
            &CATPPUCCIN_MOCHA,
            SidebarV2 {
                items: vec![NavItem::Group {
                    icon: crate::icons::ICON_BROADCAST,
                    label: "Platforms",
                    active: true,
                    expanded: true,
                    on_toggle: (),
                    children: vec![NavChild {
                        dot_color: CATPPUCCIN_MOCHA.brand,
                        label: "Twitch",
                        active: true,
                        on_press: (),
                    }],
                }],
            },
        );
    }
}
