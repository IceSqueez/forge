use iced::{
    Alignment, Background, Border, Element, Length, Shadow,
    widget::{button, pick_list, row, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{FONT_SM, FontRole, Spacing, font, spf};

#[derive(Debug, Clone)]
pub struct DeviceLabel {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

fn display_name(d: &DeviceLabel) -> String {
    if d.is_default {
        format!("{} {}", d.name, crate::tr!("widget.device.default_suffix"))
    } else {
        d.name.clone()
    }
}

fn icon_btn_style(palette: &ForgePalette, status: iced::widget::button::Status) -> button::Style {
    let bg = if matches!(status, iced::widget::button::Status::Hovered) {
        Some(Background::Color(iced::Color {
            a: 0.08,
            ..palette.border_regular
        }))
    } else {
        None
    };
    button::Style {
        background: bg,
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 6.0.into(),
        },
        text_color: palette.text_secondary,
        shadow: Shadow::default(),
        snap: false,
    }
}

pub fn output_device_picker<'a, Msg: 'a + Clone>(
    devices: &'a [DeviceLabel],
    selected: usize,
    on_select: impl Fn(usize) -> Msg + 'a,
    on_refresh: Msg,
    on_test: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let options: Vec<String> = devices.iter().map(display_name).collect();
    let selected_name: Option<String> = devices.get(selected).map(display_name);
    let options_for_closure = options.clone();

    let picker = pick_list(options, selected_name, move |chosen: String| {
        let idx = options_for_closure
            .iter()
            .position(|n| n == &chosen)
            .unwrap_or(0);
        on_select(idx)
    })
    .text_size(FONT_SM)
    .font(font(FontRole::Body))
    .width(Length::Fill);

    let p = *palette;
    let refresh_btn = button(tabler_icon(Icon::Refresh, 12.0, p.text_secondary))
        .on_press(on_refresh)
        .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
        .style(move |_theme, status| icon_btn_style(&p, status));

    let p2 = *palette;
    let test_btn = button(
        row![
            tabler_icon(Icon::PlayerPlay, 11.0, p2.text_secondary),
            text(crate::tr!("widget.device.test"))
                .size(FONT_SM)
                .color(p2.text_secondary)
                .font(font(FontRole::Body)),
        ]
        .spacing(4.0)
        .align_y(Alignment::Center),
    )
    .on_press(on_test)
    .padding([spf(Spacing::Xxs), spf(Spacing::Sm)])
    .style(move |_theme, status| icon_btn_style(&p2, status));

    let gap = spf(Spacing::Sm);

    row![picker, refresh_btn, test_btn]
        .spacing(gap)
        .align_y(Alignment::Center)
        .into()
}
