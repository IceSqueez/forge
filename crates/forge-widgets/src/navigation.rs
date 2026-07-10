use iced::{
    Border, Color, Element,
    widget::button::Status as ButtonStatus,
    widget::button::Style as ButtonStyle,
    widget::{Space, button, column, container, row, scrollable, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::sections::{DividerAxis, divider};
use crate::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};

pub const SIDEBAR_WIDTH: u16 = 200;

pub struct Sidebar<Msg> {
    pub items: Vec<NavItem<Msg>>,
    pub bottom_items: Vec<NavItem<Msg>>,
}

pub enum NavItem<Msg> {
    Section(String),
    MiniLabel(String),
    Leaf {
        icon: Icon,
        label: String,
        active: bool,
        on_press: Msg,
    },
    FlatLink {
        dot_color: Color,
        status: Option<Color>,
        label: String,
        active: bool,
        on_press: Msg,
    },
    Divider,
}

pub fn sidebar<'a, Msg: 'a + Clone>(
    palette: &'a ForgePalette,
    props: Sidebar<Msg>,
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
    item: NavItem<Msg>,
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
            status,
            label,
            active,
            on_press,
        } => nav_flat_link(dot_color, status, label, active, on_press, palette),
        NavItem::Divider => container(divider(palette, DividerAxis::Horizontal))
            .padding(iced::Padding {
                top: spf(Spacing::Sm),
                right: spf(Spacing::Xs),
                bottom: 0.0,
                left: spf(Spacing::Xs),
            })
            .width(iced::Length::Fill)
            .into(),
    }
}

fn nav_section_label<'a, Msg: 'a>(label: String, palette: &ForgePalette) -> Element<'a, Msg> {
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

fn nav_mini_label<'a, Msg: 'a>(label: String, palette: &ForgePalette) -> Element<'a, Msg> {
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
    status: Option<Color>,
    label: String,
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

    let mut content = row![dot, text(label).size(FONT_XS).width(iced::Length::Fill)]
        .spacing(10)
        .align_y(iced::Alignment::Center);

    if let Some(status_color) = status {
        let status_dot =
            container(Space::new())
                .width(5_u32)
                .height(5_u32)
                .style(move |_: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(status_color)),
                    border: Border {
                        radius: 2.5.into(),
                        ..Border::default()
                    },
                    ..Default::default()
                });
        content = content.push(status_dot);
    }

    button(content)
        .on_press(on_press)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .width(iced::Length::Fill)
        .style(move |_theme: &iced::Theme, btn_status| match btn_status {
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
    label: String,
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
