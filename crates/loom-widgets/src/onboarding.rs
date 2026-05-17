use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{column, container, row, text},
};

use crate::{
    buttons::{ghost_button, primary_button},
    palette::LoomPalette,
    tokens::{FontRole, font},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Done,
    Current,
    Pending,
    PulsingPending,
}

pub struct StepEntry {
    pub label: &'static str,
    pub sublabel: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Waiting,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Safe,
    Caution,
    Danger,
}

/// Maps seconds remaining to a display severity band.
/// Callers use this to drive color selection; the widget calls it internally.
pub fn expiration_color_band(seconds: u32) -> Severity {
    if seconds > 120 {
        Severity::Safe
    } else if seconds >= 30 {
        Severity::Caution
    } else {
        Severity::Danger
    }
}

/// `current` is 0-indexed; `total` equals `steps.len()`.
pub fn onboarding_stepper<'a, Msg: 'a>(
    steps: &'a [StepEntry],
    current: usize,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let mut col = column([]).spacing(2);

    for (i, entry) in steps.iter().enumerate() {
        let status = if i < current {
            StepStatus::Done
        } else if i == current {
            StepStatus::Current
        } else {
            StepStatus::Pending
        };

        let (dot_bg, dot_fg, label_color, sublabel_color) = match status {
            StepStatus::Done => (
                palette.success,
                palette.shell,
                palette.text_secondary,
                palette.text_muted,
            ),
            StepStatus::Current => (
                palette.brand,
                palette.shell,
                palette.text_primary,
                palette.text_secondary,
            ),
            StepStatus::Pending | StepStatus::PulsingPending => (
                palette.border_input,
                palette.text_faint,
                palette.text_muted,
                palette.text_faint,
            ),
        };

        let dot_number = if status == StepStatus::Done {
            "✓".to_string()
        } else {
            (i + 1).to_string()
        };

        let dot = container(text(dot_number).size(11).color(dot_fg))
            .width(22.0)
            .height(22.0)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(dot_bg)),
                border: Border {
                    radius: 11.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });

        let label_col = column![
            text(entry.label).size(13).color(label_color),
            text(entry.sublabel).size(11).color(sublabel_color),
        ]
        .spacing(2);

        let step_row = row![dot, label_col].spacing(10).align_y(Alignment::Center);

        col = col.push(step_row);

        if i + 1 < steps.len() {
            let connector_color = if i < current {
                palette.success
            } else {
                palette.border_regular
            };
            let connector_line = container(iced::widget::Space::new().width(1.0).height(12.0))
                .width(1.0)
                .height(12.0)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(iced::Background::Color(connector_color)),
                    ..container::Style::default()
                });
            let connector = row![iced::widget::Space::new().width(10.0), connector_line,];
            col = col.push(connector);
        }
    }

    col.into()
}

pub fn onboarding_step_header<'a, Msg: 'a>(
    step: usize,
    total: usize,
    optional: bool,
    waiting: bool,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let header_text = format!("STEP {} OF {}", step, total);

    let mut header_row = row![
        text(header_text)
            .font(font(FontRole::Monospace))
            .size(11)
            .color(palette.text_muted),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    if optional {
        let optional_bg = Color {
            a: 0.18,
            ..palette.warning
        };
        let pill = container(
            text("OPTIONAL")
                .font(font(FontRole::Monospace))
                .size(10)
                .color(palette.warning),
        )
        .padding([3, 8])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(optional_bg)),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });
        header_row = header_row.push(pill);
    }

    if waiting {
        let waiting_bg = Color {
            a: 0.18,
            ..palette.brand
        };
        let pill = container(
            text("WAITING")
                .font(font(FontRole::Monospace))
                .size(10)
                .color(palette.brand),
        )
        .padding([3, 8])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(waiting_bg)),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });
        header_row = header_row.push(pill);
    }

    header_row.into()
}

pub fn platform_picker_card<'a, Msg: Clone + 'a>(
    name: &'a str,
    brand_color: Color,
    selected: bool,
    on_press: Msg,
) -> Element<'a, Msg> {
    let (bg, border_color, border_width) = if selected {
        (
            Color {
                a: 0.12,
                ..brand_color
            },
            brand_color,
            2.0_f32,
        )
    } else {
        (Color::TRANSPARENT, brand_color, 0.0_f32)
    };

    let dot = container(iced::widget::Space::new().width(10.0).height(10.0))
        .width(10.0)
        .height(10.0)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(brand_color)),
            border: Border {
                radius: 5.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let content = row![dot, text(name).size(14)]
        .spacing(8)
        .align_y(Alignment::Center);

    iced::widget::button(
        container(content)
            .padding([12, 16])
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(bg)),
                border: Border {
                    color: border_color,
                    width: border_width,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            }),
    )
    .on_press(on_press)
    .padding(0)
    .style(
        move |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: None,
            text_color: Color::TRANSPARENT,
            border: Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        },
    )
    .into()
}

pub fn locale_tip_card<'a, Msg: Clone + 'a>(
    body: &'a str,
    link_label: Option<&'a str>,
    on_link: Option<Msg>,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let bg = Color {
        a: 0.10,
        ..palette.info
    };
    let border_color = Color {
        a: 0.25,
        ..palette.info
    };

    let mut content_col = column![
        text("ℹ").size(14).color(palette.info),
        text(body).size(13).color(palette.text_secondary),
    ]
    .spacing(6);

    if let (Some(label), Some(msg)) = (link_label, on_link) {
        content_col = content_col.push(ghost_button(label, msg, palette));
    }

    container(content_col)
        .padding(12)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn onboarding_footer<'a, Msg: Clone + 'a>(
    on_back: Option<Msg>,
    on_skip: Option<Msg>,
    on_continue: Msg,
    continue_enabled: bool,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let back_element: Element<'a, Msg> = match on_back {
        Some(msg) => ghost_button("Back", msg, palette),
        None => iced::widget::Space::new().width(64.0).into(),
    };

    let skip_element: Element<'a, Msg> = match on_skip {
        Some(msg) => {
            let text_color = palette.warning;
            iced::widget::button(text("Skip for now").size(13).color(text_color))
                .on_press(msg)
                .padding([6, 12])
                .style(
                    move |_theme: &iced::Theme, _status| iced::widget::button::Style {
                        background: None,
                        text_color,
                        border: Border::default(),
                        shadow: iced::Shadow::default(),
                        snap: false,
                    },
                )
                .into()
        }
        None => iced::widget::Space::new().width(0.0).into(),
    };

    let continue_element: Element<'a, Msg> = if continue_enabled {
        primary_button("Continue", on_continue, palette)
    } else {
        let bg = Color {
            a: 0.4,
            ..palette.brand
        };
        let text_color = Color {
            a: 0.4,
            ..palette.shell
        };
        container(text("Continue").size(13).color(text_color))
            .padding([8, 12])
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(bg)),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
    };

    row![
        back_element,
        iced::widget::Space::new().width(Length::Fill),
        skip_element,
        continue_element,
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

pub fn device_code_display<'a, Msg: Clone + 'a>(
    code: &'a str,
    on_copy: Msg,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let border_color = palette.brand;
    let bg = Color {
        a: 0.06,
        ..palette.brand
    };

    let code_text = container(
        text(code)
            .font(font(FontRole::Monospace))
            .size(28)
            .color(palette.text_primary),
    )
    .width(Length::Fill)
    .align_x(Alignment::Center);

    let copy_btn = iced::widget::button(text("Copy").size(12).color(palette.brand))
        .on_press(on_copy)
        .padding([4, 10])
        .style(
            move |_theme: &iced::Theme, _status| iced::widget::button::Style {
                background: None,
                text_color: border_color,
                border: Border {
                    color: border_color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        );

    let inner = column![code_text, copy_btn]
        .spacing(8)
        .align_x(Alignment::Center);

    container(inner)
        .padding(20)
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn expiration_timer<'a, Msg: Clone + 'a>(
    remaining: std::time::Duration,
    on_refresh: Msg,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let total_secs = remaining.as_secs() as u32;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    let label = format!("Expires in {:02}:{:02}", mins, secs);

    let color = match expiration_color_band(total_secs) {
        Severity::Safe => palette.success,
        Severity::Caution => palette.warning,
        Severity::Danger => palette.random,
    };

    let refresh_color = palette.brand;
    let refresh_btn = iced::widget::button(text("Get new code").size(12).color(refresh_color))
        .on_press(on_refresh)
        .padding([2, 8])
        .style(
            move |_theme: &iced::Theme, _status| iced::widget::button::Style {
                background: None,
                text_color: refresh_color,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        );

    row![
        text("⏱").size(13).color(color),
        text(label)
            .font(font(FontRole::Monospace))
            .size(13)
            .color(color),
        iced::widget::Space::new().width(Length::Fill),
        refresh_btn,
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

pub fn live_status_banner<'a, Msg: 'a>(
    kind: BannerKind,
    message: &'a str,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let (dot_color, bg_color, border_color) = match kind {
        BannerKind::Waiting => (
            palette.brand,
            Color {
                a: 0.10,
                ..palette.brand
            },
            Color {
                a: 0.25,
                ..palette.brand
            },
        ),
        BannerKind::Success => (
            palette.success,
            Color {
                a: 0.10,
                ..palette.success
            },
            Color {
                a: 0.25,
                ..palette.success
            },
        ),
        BannerKind::Error => (
            palette.random,
            Color {
                a: 0.10,
                ..palette.random
            },
            Color {
                a: 0.25,
                ..palette.random
            },
        ),
    };

    let dot = container(iced::widget::Space::new().width(8.0).height(8.0))
        .width(8.0)
        .height(8.0)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(dot_color)),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let content_row = row![dot, text(message).size(13).color(palette.text_primary)]
        .spacing(10)
        .align_y(Alignment::Center);

    container(content_row)
        .padding([10, 14])
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg_color)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn numbered_box_step<'a, Msg: 'a>(
    number: u8,
    content: Element<'a, Msg>,
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let dot_bg = palette.brand;
    let dot_fg = palette.shell;

    let badge = container(text(number.to_string()).size(11).color(dot_fg))
        .width(22.0)
        .height(22.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(dot_bg)),
            border: Border {
                radius: 11.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let inner = row![badge, content].spacing(12).align_y(Alignment::Start);

    container(inner)
        .padding(14)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use iced::widget::text;

    #[test]
    fn expiration_color_band_safe_above_120s() {
        assert_eq!(expiration_color_band(121), Severity::Safe);
        assert_eq!(expiration_color_band(300), Severity::Safe);
        assert_eq!(expiration_color_band(u32::MAX), Severity::Safe);
    }

    #[test]
    fn expiration_color_band_caution_between_30_and_120() {
        assert_eq!(expiration_color_band(120), Severity::Caution);
        assert_eq!(expiration_color_band(75), Severity::Caution);
        assert_eq!(expiration_color_band(30), Severity::Caution);
    }

    #[test]
    fn expiration_color_band_danger_below_30() {
        assert_eq!(expiration_color_band(29), Severity::Danger);
        assert_eq!(expiration_color_band(1), Severity::Danger);
        assert_eq!(expiration_color_band(0), Severity::Danger);
    }

    #[test]
    fn banner_kind_has_three_variants() {
        let kinds = [BannerKind::Waiting, BannerKind::Success, BannerKind::Error];
        assert_eq!(kinds.len(), 3);
    }

    #[test]
    fn stepper_with_steps_compiles() {
        let steps = [
            StepEntry {
                label: "Welcome",
                sublabel: "Introduction",
            },
            StepEntry {
                label: "Connect",
                sublabel: "Choose platform",
            },
        ];
        let _: Element<'_, ()> = onboarding_stepper(&steps, 0, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn onboarding_step_header_compiles_no_pills() {
        let _: Element<'_, ()> = onboarding_step_header(1, 5, false, false, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn onboarding_step_header_compiles_with_both_pills() {
        let _: Element<'_, ()> = onboarding_step_header(2, 5, true, true, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn platform_picker_card_compiles_selected() {
        let _: Element<'_, ()> = platform_picker_card("Twitch", CATPPUCCIN_MOCHA.brand, true, ());
    }

    #[test]
    fn platform_picker_card_compiles_unselected() {
        let _: Element<'_, ()> =
            platform_picker_card("YouTube", CATPPUCCIN_MOCHA.random, false, ());
    }

    #[test]
    fn locale_tip_card_compiles_without_link() {
        let _: Element<'_, ()> =
            locale_tip_card("Detected locale: en-US.", None, None, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn locale_tip_card_compiles_with_link() {
        let _: Element<'_, ()> = locale_tip_card(
            "Detected locale: uk-UA.",
            Some("Change language"),
            Some(()),
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn onboarding_footer_compiles_all_options() {
        let _: Element<'_, ()> = onboarding_footer(Some(()), Some(()), (), true, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn onboarding_footer_compiles_no_back_no_skip() {
        let _: Element<'_, ()> = onboarding_footer(None, None, (), false, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn device_code_display_compiles() {
        let _: Element<'_, ()> = device_code_display("WDJB-MJHT", (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn expiration_timer_compiles() {
        let _: Element<'_, ()> =
            expiration_timer(std::time::Duration::from_secs(90), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn live_status_banner_compiles_all_kinds() {
        let _: Element<'_, ()> = live_status_banner(
            BannerKind::Waiting,
            "Polling for authorization...",
            &CATPPUCCIN_MOCHA,
        );
        let _: Element<'_, ()> = live_status_banner(
            BannerKind::Success,
            "Authorization successful.",
            &CATPPUCCIN_MOCHA,
        );
        let _: Element<'_, ()> = live_status_banner(
            BannerKind::Error,
            "Authorization denied.",
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn numbered_box_step_compiles() {
        let content: Element<'_, ()> = text("Open the URL in your browser.").into();
        let _: Element<'_, ()> = numbered_box_step(1, content, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn step_status_is_copy() {
        let s = StepStatus::Current;
        let t = s;
        assert_eq!(s, t);
    }
}
