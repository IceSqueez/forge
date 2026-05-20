use iced::{
    Alignment, Background, Border, Element, Length, Shadow,
    widget::{button, column, container, row, text},
};

use crate::icons::{BOOTSTRAP_FONT, ICON_PLAY, ICON_X};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, Density, FONT_BODY_LG, FONT_BODY_SM, FONT_CAPS_SM, FontRole, Radius, Spacing,
    font, radius, spacing,
};

pub struct ClipCardData {
    pub name: String,
    pub duration_label: String,
    pub hotkey_label: Option<String>,
    pub device_label: String,
    pub volume_pct: u8,
}

fn chip_style(bg: iced::Color, border: iced::Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: border,
            width: 0.5,
            radius: radius(Radius::Xs).into(),
        },
        ..container::Style::default()
    }
}

fn action_btn<'a, Msg: 'a + Clone>(
    icon: char,
    color: iced::Color,
    on_press: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    button(
        text(icon.to_string())
            .font(BOOTSTRAP_FONT)
            .size(12.0)
            .color(color),
    )
    .on_press(on_press)
    .padding([4.0, 8.0])
    .style(move |_theme, status| button::Style {
        background: if matches!(status, iced::widget::button::Status::Hovered) {
            Some(Background::Color(iced::Color { a: 0.1, ..color }))
        } else {
            None
        },
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 5.0.into(),
        },
        text_color: color,
        shadow: Shadow::default(),
        snap: false,
    })
    .into()
}

pub fn clip_card<'a, Msg: 'a + Clone>(
    data: &'a ClipCardData,
    on_play: Msg,
    on_edit: Msg,
    on_delete: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let name_row = text(data.name.as_str())
        .size(FONT_BODY_LG)
        .color(palette.text_primary)
        .font(font(FontRole::Body));

    let duration_chip = container(
        text(data.duration_label.as_str())
            .size(FONT_CAPS_SM)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
    )
    .padding([2.0, 6.0])
    .style(chip_style(palette.surface_overlay, palette.border_regular));

    let chips_row: Element<'a, Msg> = if let Some(hk) = &data.hotkey_label {
        let hk_chip = container(
            text(hk.as_str())
                .size(FONT_CAPS_SM)
                .color(palette.warning)
                .font(font(FontRole::Monospace)),
        )
        .padding([2.0, 6.0])
        .style(chip_style(palette.surface_overlay, palette.border_regular));

        row![duration_chip, hk_chip]
            .spacing(f32::from(spacing(Spacing::Sm, Density::Cozy)))
            .into()
    } else {
        duration_chip.into()
    };

    let device_row = row![
        text(data.device_label.as_str())
            .size(FONT_BODY_SM)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace))
            .width(Length::Fill),
        text(format!("{}%", data.volume_pct))
            .size(FONT_BODY_SM)
            .color(if data.volume_pct > 100 {
                palette.warning
            } else {
                palette.text_secondary
            })
            .font(font(FontRole::Monospace)),
    ]
    .spacing(f32::from(spacing(Spacing::Sm, Density::Cozy)));

    let play_btn = action_btn(ICON_PLAY, palette.success, on_play, palette);
    let edit_btn = action_btn('\u{F4CA}', palette.info, on_edit, palette);
    let delete_btn = action_btn(ICON_X, palette.random, on_delete, palette);

    let action_row = row![play_btn, edit_btn, delete_btn]
        .spacing(f32::from(spacing(Spacing::Sm, Density::Cozy)));

    let separator = container(iced::widget::Space::new())
        .width(Length::Fill)
        .height(1.0)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        });

    let content = column![
        name_row,
        chips_row,
        device_row,
        separator,
        row![iced::widget::Space::new().width(Length::Fill), action_row].align_y(Alignment::Center),
    ]
    .spacing(f32::from(spacing(Spacing::Md, Density::Cozy)));

    container(content)
        .padding(f32::from(spacing(Spacing::Xxl, Density::Cozy)))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    fn sample_data() -> ClipCardData {
        ClipCardData {
            name: "Air horn".to_string(),
            duration_label: "2.3s".to_string(),
            hotkey_label: Some("Ctrl+1".to_string()),
            device_label: "default".to_string(),
            volume_pct: 80,
        }
    }

    #[test]
    fn clip_card_with_hotkey_constructs() {
        let data = sample_data();
        let _ = clip_card::<u8>(&data, 0, 1, 2, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn clip_card_without_hotkey_constructs() {
        let data = ClipCardData {
            hotkey_label: None,
            ..sample_data()
        };
        let _ = clip_card::<u8>(&data, 0, 1, 2, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn clip_card_post_gain_volume_constructs() {
        let data = ClipCardData {
            volume_pct: 130,
            ..sample_data()
        };
        let _ = clip_card::<u8>(&data, 0, 1, 2, &CATPPUCCIN_MOCHA);
    }
}
