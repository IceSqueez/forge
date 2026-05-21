use iced::{
    Alignment, Background, Border, Element, Length, Shadow,
    widget::{button, pick_list, row, text},
};

use crate::icons::{BOOTSTRAP_FONT, ICON_PLAY, ICON_REFRESH};
use crate::palette::ForgePalette;
use crate::tokens::{Density, FONT_SM, FontRole, Spacing, font, spacing};

#[derive(Debug, Clone)]
pub struct DeviceLabel {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

fn display_name(d: &DeviceLabel) -> String {
    if d.is_default {
        format!("{} (default)", d.name)
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
    let refresh_btn = button(
        text(ICON_REFRESH.to_string())
            .font(BOOTSTRAP_FONT)
            .size(12.0)
            .color(p.text_secondary),
    )
    .on_press(on_refresh)
    .padding([4.0, 8.0])
    .style(move |_theme, status| icon_btn_style(&p, status));

    let p2 = *palette;
    let test_btn = button(
        row![
            text(ICON_PLAY.to_string())
                .font(BOOTSTRAP_FONT)
                .size(11.0)
                .color(p2.text_secondary),
            text("Test")
                .size(FONT_SM)
                .color(p2.text_secondary)
                .font(font(FontRole::Body)),
        ]
        .spacing(4.0)
        .align_y(Alignment::Center),
    )
    .on_press(on_test)
    .padding([4.0, 10.0])
    .style(move |_theme, status| icon_btn_style(&p2, status));

    let gap = f32::from(spacing(Spacing::Sm, Density::Cozy));

    row![picker, refresh_btn, test_btn]
        .spacing(gap)
        .align_y(Alignment::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn picker_empty_devices_constructs() {
        let _ = output_device_picker::<usize>(&[], 0, |i| i, 0, 0, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn picker_single_device_constructs() {
        let devices = vec![DeviceLabel {
            id: "dev-1".to_string(),
            name: "Speakers".to_string(),
            is_default: true,
        }];
        let _ = output_device_picker::<usize>(&devices, 0, |i| i, 0, 0, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn picker_three_devices_constructs() {
        let devices = vec![
            DeviceLabel {
                id: "dev-1".to_string(),
                name: "Speakers".to_string(),
                is_default: true,
            },
            DeviceLabel {
                id: "dev-2".to_string(),
                name: "Headphones".to_string(),
                is_default: false,
            },
            DeviceLabel {
                id: "dev-3".to_string(),
                name: "Virtual Cable".to_string(),
                is_default: false,
            },
        ];
        let _ = output_device_picker::<usize>(&devices, 1, |i| i, 0, 0, &CATPPUCCIN_MOCHA);
    }
}
