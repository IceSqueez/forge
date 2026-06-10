use iced::{
    Border, Element, Length,
    widget::{Row, Space, column, container, row, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::status::status_dot;
use crate::tokens::{BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp};

fn logo_box<'a, Msg: 'a>(palette: &ForgePalette) -> Element<'a, Msg> {
    let bg = palette.brand;
    let fg = palette.shell;
    container(text("F").size(FONT_XS).color(fg).font(iced::Font {
        weight: iced::font::Weight::Bold,
        ..iced::Font::DEFAULT
    }))
    .width(16)
    .height(16)
    .align_x(iced::Alignment::Center)
    .align_y(iced::Alignment::Center)
    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
        },
        ..Default::default()
    })
    .into()
}

pub fn title_bar<'a, Msg: 'a>(palette: &'a ForgePalette) -> Element<'a, Msg> {
    let shell = palette.shell;
    let border_color = palette.border_regular;
    let text_primary = palette.text_primary;

    let logo = logo_box(palette);
    let forge_label = text(crate::tr!("widget.layout.app_name"))
        .size(FONT_SM)
        .color(text_primary)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..iced::Font::DEFAULT
        });

    let content = row![logo, forge_label]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .padding([0, sp(Spacing::Md)]);

    container(content)
        .width(Length::Fill)
        .height(32)
        .align_y(iced::Alignment::Center)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(shell)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn app_footer<'a, Msg: 'a>(
    connected: u8,
    total: u8,
    uptime: &str,
    version: &str,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let shell = palette.shell;
    let border_color = palette.border_regular;
    let text_muted = palette.text_muted;
    let text_faint = palette.text_faint;
    let text_secondary = palette.text_secondary;
    let success = palette.success;
    let mono = font(FontRole::Monospace);

    let forge_label = text(crate::tr!("widget.layout.footer_app"))
        .size(FONT_XS)
        .color(text_muted)
        .font(mono);
    let dot_sep = text("·").size(FONT_XS).color(text_faint).font(mono);
    let version_label = text(format!("v{version}"))
        .size(FONT_XS)
        .color(text_faint)
        .font(mono);
    let left = row![forge_label, dot_sep, version_label]
        .spacing(8)
        .align_y(iced::Alignment::Center);

    let dot_color = if connected == 0 { text_faint } else { success };
    let dot_el = status_dot(dot_color, 6.0);
    let conn_label = text(crate::tr!(
        "widget.layout.connected",
        connected = connected as i64,
        total = total as i64
    ))
    .size(FONT_XS)
    .color(text_secondary)
    .font(mono);
    let conn_row = row![dot_el, conn_label]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    let sep2 = text("·").size(FONT_XS).color(text_faint).font(mono);

    let clock_icon = tabler_icon(Icon::Clock, 10.0, text_faint);
    let uptime_label = text(format!(
        "{uptime} {}",
        crate::tr!("widget.layout.uptime_suffix")
    ))
    .size(FONT_XS)
    .color(text_secondary)
    .font(mono);
    let uptime_row = row![clock_icon, uptime_label]
        .spacing(5)
        .align_y(iced::Alignment::Center);

    let right = row![conn_row, sep2, uptime_row]
        .spacing(12)
        .align_y(iced::Alignment::Center);

    let content = row![left, Space::new().width(Length::Fill), right]
        .align_y(iced::Alignment::Center)
        .padding([0, sp(Spacing::Md)]);

    container(content)
        .width(Length::Fill)
        .height(24)
        .align_y(iced::Alignment::Center)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(shell)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn toolbar<'a, Msg: 'a>(
    left: Vec<Element<'a, Msg>>,
    right: Vec<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let elevated = palette.elevated;
    let border_color = palette.border_regular;

    let mut left_row: Row<'a, Msg> = row([]).spacing(4).align_y(iced::Alignment::Center);
    for item in left {
        left_row = left_row.push(item);
    }

    let mut right_row: Row<'a, Msg> = row([]).spacing(4).align_y(iced::Alignment::Center);
    for item in right {
        right_row = right_row.push(item);
    }

    let content = row![left_row, Space::new().width(Length::Fill), right_row,]
        .align_y(iced::Alignment::Center)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)]);

    container(content)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(elevated)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn page_shell<'a, Msg: 'a>(
    title_bar_el: Element<'a, Msg>,
    toolbar_el: Option<Element<'a, Msg>>,
    sidebar_el: Element<'a, Msg>,
    content_el: Element<'a, Msg>,
    footer_el: Option<Element<'a, Msg>>,
) -> Element<'a, Msg> {
    let body = row![sidebar_el, content_el].height(Length::Fill);
    let mut shell_col = column![title_bar_el];
    if let Some(tb) = toolbar_el {
        shell_col = shell_col.push(tb);
    }
    shell_col = shell_col.push(body);
    if let Some(footer) = footer_el {
        shell_col = shell_col.push(footer);
    }
    shell_col.into()
}
