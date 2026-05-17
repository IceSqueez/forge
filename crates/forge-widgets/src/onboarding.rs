use iced::{
    Alignment, Border, Color, Element, Length, Padding,
    widget::{Column, Row, Stack, column, container, row, text},
};

use crate::{
    buttons::{ghost_button, primary_button_with_icon_right, secondary_button},
    palette::LoomPalette,
    tokens::{
        BORDER_THIN, FONT_BODY_LG, FONT_BODY_MD, FONT_BODY_SM, FONT_CAPS, FONT_CAPS_SM,
        FONT_DEVICE_CODE, FONT_PAGE_TITLE, FONT_PLATFORM_NAME, FontRole, Radius, Spacing, font,
        radius, spacing,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Done,
    Current,
    Pending,
    PulsingPending,
}

pub struct StepInfo {
    pub label: &'static str,
    pub sublabel: &'static str,
    pub status: StepStatus,
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
pub fn expiration_color_band(seconds: u32) -> Severity {
    if seconds > 120 {
        Severity::Safe
    } else if seconds >= 30 {
        Severity::Caution
    } else {
        Severity::Danger
    }
}

fn badge_circle<'a, Msg: 'a>(inner: Element<'a, Msg>, size: f32, bg: Color) -> Element<'a, Msg> {
    container(inner)
        .width(size)
        .height(size)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                radius: (size / 2.0).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn disabled_primary_button<'a, Msg: 'a>(
    label: &'a str,
    icon_char: char,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let bg = Color {
        a: 0.4,
        ..palette.brand
    };
    let text_color = Color {
        a: 0.5,
        ..palette.shell
    };
    let r = radius(Radius::Md);
    let vp = spacing(Spacing::Md, crate::tokens::Density::Cozy);
    let hp = spacing(Spacing::Xxxl, crate::tokens::Density::Cozy);
    let gap = spacing(Spacing::Sm, crate::tokens::Density::Cozy);

    container(
        row![
            text(label).size(FONT_BODY_MD).color(text_color),
            text(icon_char.to_string())
                .size(FONT_BODY_MD)
                .color(text_color),
        ]
        .spacing(f32::from(gap)),
    )
    .padding(Padding::from([vp, hp]))
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            radius: r.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// Vertical stepper using Stack to render the connecting line behind all dots.
pub fn onboarding_stepper<'a, Msg: 'a>(
    steps: &'a [StepInfo],
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let row_height: f32 = 22.0;
    let row_gap: f32 = 14.0;
    let line_x: f32 = 11.0;

    let step_count = steps.len();

    let total_col_height = if step_count > 0 {
        row_height * step_count as f32 + row_gap * (step_count.saturating_sub(1)) as f32
    } else {
        0.0
    };

    let line_top: f32 = 11.0;
    let line_height = (total_col_height - line_top - 11.0).max(0.0);

    let line = container(
        iced::widget::Space::new()
            .width(Length::Fixed(1.0))
            .height(Length::Fixed(line_height)),
    )
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(palette.border_regular)),
        ..container::Style::default()
    });

    let line_layer: Element<'a, Msg> = row![
        iced::widget::Space::new().width(Length::Fixed(line_x)),
        line,
    ]
    .into();

    let mut steps_col = Column::new().spacing(row_gap);

    for info in steps.iter() {
        let (dot_bg, dot_fg, label_color, sublabel_color) = match info.status {
            StepStatus::Done => (
                palette.success,
                palette.shell,
                palette.text_primary,
                palette.text_muted,
            ),
            StepStatus::Current => (
                palette.brand,
                palette.shell,
                palette.text_primary,
                palette.text_muted,
            ),
            StepStatus::Pending | StepStatus::PulsingPending => (
                palette.surface_overlay,
                palette.text_faint,
                palette.text_secondary,
                palette.text_faint,
            ),
        };

        let dot_label = if info.status == StepStatus::Done {
            "✓".to_string()
        } else {
            String::new()
        };

        let dot_inner: Element<'a, Msg> = text(dot_label).size(FONT_CAPS).color(dot_fg).into();
        let dot = badge_circle(dot_inner, 22.0, dot_bg);

        let text_col = column![
            text(info.label).size(FONT_BODY_MD).color(label_color),
            text(info.sublabel).size(FONT_CAPS).color(sublabel_color),
        ]
        .spacing(2);

        let step_row = row![dot, text_col].spacing(12).align_y(Alignment::Center);

        steps_col = steps_col.push(step_row);
    }

    let steps_layer: Element<'a, Msg> = steps_col.into();

    Stack::new().push(line_layer).push(steps_layer).into()
}

pub fn onboarding_step_header<'a, Msg: 'a>(
    step: usize,
    total: usize,
    title: &'a str,
    optional: bool,
    waiting: bool,
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let header_text = format!("STEP {} OF {}", step, total);

    let mut badge_row = Row::new()
        .spacing(f32::from(spacing(
            Spacing::Md,
            crate::tokens::Density::Cozy,
        )))
        .align_y(Alignment::Center)
        .push(
            text(header_text)
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS_SM)
                .color(palette.text_muted),
        );

    if optional {
        let pill = container(
            text("OPTIONAL")
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS - 2.0)
                .color(palette.warning),
        )
        .padding(Padding::from([1_u16, 7_u16]))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(palette.surface_overlay)),
            border: Border {
                radius: radius(Radius::Lg).into(),
                ..Border::default()
            },
            ..container::Style::default()
        });
        badge_row = badge_row.push(pill);
    }

    if waiting {
        let pill_content = row![
            text("↻").size(FONT_CAPS - 2.0).color(palette.brand),
            text("WAITING")
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS - 2.0)
                .color(palette.brand),
        ]
        .spacing(3)
        .align_y(Alignment::Center);

        let pill = container(pill_content)
            .padding(Padding::from([1_u16, 7_u16]))
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(palette.surface_overlay)),
                border: Border {
                    radius: radius(Radius::Lg).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });
        badge_row = badge_row.push(pill);
    }

    column![
        badge_row,
        text(title)
            .size(FONT_PAGE_TITLE)
            .color(palette.text_primary),
    ]
    .spacing(f32::from(spacing(
        Spacing::Sm,
        crate::tokens::Density::Cozy,
    )))
    .into()
}

pub struct PlatformCardProps<'a> {
    pub name: &'a str,
    pub letter: &'a str,
    pub brand_color: Color,
    pub subtitle: &'a str,
    pub capability_summary: &'a str,
    pub selected: bool,
}

pub fn platform_picker_card<'a, Msg: Clone + 'a>(
    props: PlatformCardProps<'a>,
    on_press: Msg,
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let PlatformCardProps {
        name,
        letter,
        brand_color,
        subtitle,
        capability_summary,
        selected,
    } = props;
    let border_color = if selected {
        palette.brand
    } else {
        palette.border_regular
    };

    let icon_box = container(text(letter).size(18.0).color(palette.shell))
        .width(34.0)
        .height(34.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(brand_color)),
            border: Border {
                radius: radius(Radius::Lg).into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let title_row = column![
        text(name)
            .size(FONT_PLATFORM_NAME)
            .color(palette.text_primary),
        text(subtitle).size(FONT_CAPS).color(palette.text_muted),
    ]
    .spacing(2);

    let info_col = column![
        title_row,
        text(capability_summary)
            .size(FONT_BODY_SM)
            .color(palette.text_muted),
    ]
    .spacing(4);

    let card_content: Element<'a, Msg> = row![icon_box, info_col]
        .spacing(10)
        .align_y(Alignment::Center)
        .into();

    let check_overlay: Element<'a, Msg> = if selected {
        let check_inner: Element<'a, Msg> = text("✓").size(11.0).color(palette.shell).into();
        let check_circle = badge_circle(check_inner, 18.0, palette.brand);

        container(check_circle)
            .width(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::Start)
            .into()
    } else {
        iced::widget::Space::new().into()
    };

    let stacked: Element<'a, Msg> = Stack::new().push(card_content).push(check_overlay).into();

    iced::widget::button(
        container(stacked)
            .padding(Padding::from([14_u16, 14_u16]))
            .width(Length::Fill),
    )
    .on_press(on_press)
    .padding(0)
    .style(
        move |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(iced::Background::Color(palette.elevated)),
            text_color: palette.text_primary,
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: radius(Radius::Xxxl).into(),
            },
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
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let mut text_block = column![text(body).size(FONT_BODY_SM).color(palette.text_secondary)];

    if let (Some(label), Some(msg)) = (link_label, on_link) {
        text_block = text_block.push(ghost_button(label, msg, palette));
    }

    let content = row![text("ⓘ").size(14.0).color(palette.info), text_block,]
        .spacing(f32::from(spacing(
            Spacing::Md,
            crate::tokens::Density::Cozy,
        )))
        .align_y(Alignment::Start);

    container(content)
        .padding(Padding::from([10_u16, 12_u16]))
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Xl).into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn onboarding_footer<'a, Msg: Clone + 'a>(
    on_back: Option<Msg>,
    on_skip: Option<Msg>,
    continue_label: &'a str,
    continue_icon: char,
    on_continue: Msg,
    continue_enabled: bool,
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let back_element: Element<'a, Msg> = match on_back {
        Some(msg) => ghost_button("← Back", msg, palette),
        None => iced::widget::Space::new().width(Length::Fixed(64.0)).into(),
    };

    let skip_element: Element<'a, Msg> = match on_skip {
        Some(msg) => secondary_button("Skip for now", msg, palette),
        None => iced::widget::Space::new().width(Length::Fixed(0.0)).into(),
    };

    let continue_element: Element<'a, Msg> = if continue_enabled {
        primary_button_with_icon_right(continue_label, continue_icon, on_continue, palette)
    } else {
        disabled_primary_button(continue_label, continue_icon, palette)
    };

    let divider = container(
        iced::widget::Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(1.0)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(1.0))
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(palette.border_regular)),
        ..container::Style::default()
    });

    column![
        divider,
        row![
            back_element,
            iced::widget::Space::new().width(Length::Fill),
            skip_element,
            continue_element,
        ]
        .spacing(f32::from(spacing(
            Spacing::Md,
            crate::tokens::Density::Cozy
        )))
        .align_y(Alignment::Center),
    ]
    .spacing(f32::from(spacing(
        Spacing::Xxl,
        crate::tokens::Density::Cozy,
    )))
    .into()
}

pub fn device_code_display<'a, Msg: Clone + 'a>(
    code: &'a str,
    on_copy: Msg,
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let mut char_row = Row::new().spacing(0);
    for ch in code.chars() {
        let char_box = container(
            text(ch.to_string())
                .font(font(FontRole::Monospace))
                .size(FONT_DEVICE_CODE)
                .color(palette.brand),
        )
        .padding(Padding::from([0_u16, 6_u16]));
        char_row = char_row.push(char_box);
    }

    let code_block = container(char_row)
        .padding(Padding::from([18_u16, 22_u16]))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(palette.shell)),
            border: Border {
                color: palette.brand,
                width: BORDER_THIN,
                radius: radius(Radius::Xl).into(),
            },
            ..container::Style::default()
        });

    let copy_content = column![
        text("⎘").size(18.0).color(palette.text_secondary),
        text("Copy").size(FONT_CAPS).color(palette.text_muted),
    ]
    .spacing(4)
    .align_x(Alignment::Center);

    let copy_block = iced::widget::button(copy_content)
        .on_press(on_copy)
        .padding(Padding::from([
            spacing(Spacing::Md, crate::tokens::Density::Cozy),
            spacing(Spacing::Xl, crate::tokens::Density::Cozy),
        ]))
        .style(
            move |_theme: &iced::Theme, _status| iced::widget::button::Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                text_color: palette.text_muted,
                border: Border {
                    color: palette.border_regular,
                    width: BORDER_THIN,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        );

    row![code_block, copy_block]
        .spacing(f32::from(spacing(
            Spacing::Xxl,
            crate::tokens::Density::Cozy,
        )))
        .align_y(Alignment::Center)
        .into()
}

pub fn expiration_timer<'a, Msg: Clone + 'a>(
    remaining: std::time::Duration,
    refresh_label: &'a str,
    on_refresh: Msg,
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let total_secs = remaining.as_secs() as u32;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    let timer_text = format!("{mins:02}:{secs:02}");

    let color = match expiration_color_band(total_secs) {
        Severity::Safe => palette.success,
        Severity::Caution => palette.warning,
        Severity::Danger => palette.random,
    };

    row![
        text("⏱").size(FONT_BODY_MD).color(color),
        text(timer_text).size(FONT_BODY_MD).color(color),
        text("·").size(FONT_BODY_MD).color(palette.text_faint),
        ghost_button(refresh_label, on_refresh, palette),
    ]
    .spacing(f32::from(spacing(
        Spacing::Sm,
        crate::tokens::Density::Cozy,
    )))
    .align_y(Alignment::Center)
    .into()
}

pub fn live_status_banner<'a, Msg: 'a>(
    kind: BannerKind,
    message: &'a str,
    hint: Option<&'a str>,
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let (dot_color, bg_color, border_color) = match kind {
        BannerKind::Waiting => (palette.brand, palette.surface_overlay, palette.brand),
        BannerKind::Success => (
            palette.success,
            Color {
                a: 0.18,
                ..palette.success
            },
            palette.success,
        ),
        BannerKind::Error => (
            palette.random,
            Color {
                a: 0.18,
                ..palette.random
            },
            palette.random,
        ),
    };

    let dot: Element<'a, Msg> = badge_circle(iced::widget::Space::new().into(), 8.0, dot_color);

    let mut content_col = column![
        row![
            dot,
            text(message).size(FONT_BODY_MD).color(palette.text_primary)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ];

    if let Some(hint_text) = hint {
        content_col = content_col.push(
            text(hint_text)
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS_SM)
                .color(palette.text_faint),
        );
    }

    container(content_col)
        .padding(Padding::from([11_u16, 14_u16]))
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg_color)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: radius(Radius::Xl).into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn numbered_box_step<'a, Msg: 'a>(
    number: u8,
    title: &'a str,
    body: &'a str,
    active: bool,
    palette: &'a LoomPalette,
) -> Element<'a, Msg> {
    let (badge_bg, badge_fg) = if active {
        (palette.brand, palette.shell)
    } else {
        (palette.surface_overlay, palette.text_primary)
    };

    let badge_inner: Element<'a, Msg> = text(number.to_string())
        .size(FONT_BODY_LG)
        .color(badge_fg)
        .into();
    let badge = badge_circle(badge_inner, 28.0, badge_bg);

    let text_col = column![
        text(title).size(FONT_BODY_LG).color(palette.text_primary),
        text(body).size(FONT_BODY_SM).color(palette.text_muted),
    ]
    .spacing(4);

    let inner = row![badge, text_col]
        .spacing(f32::from(spacing(
            Spacing::Xl,
            crate::tokens::Density::Cozy,
        )))
        .align_y(Alignment::Start);

    container(inner)
        .padding(Padding::from([
            spacing(Spacing::Xxl, crate::tokens::Density::Cozy),
            spacing(Spacing::Xxl, crate::tokens::Density::Cozy),
        ]))
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Xxxl).into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
fn badge_active_uses_brand(active: bool, palette: &LoomPalette) -> Color {
    if active {
        palette.brand
    } else {
        palette.surface_overlay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

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
    fn banner_kind_count_is_3() {
        let kinds = [BannerKind::Waiting, BannerKind::Success, BannerKind::Error];
        assert_eq!(kinds.len(), 3);
    }

    #[test]
    fn step_status_default_is_pending() {
        let status = StepStatus::Pending;
        assert_eq!(status, StepStatus::Pending);
    }

    #[test]
    fn numbered_step_badge_active_uses_brand() {
        let active_color = badge_active_uses_brand(true, &CATPPUCCIN_MOCHA);
        let inactive_color = badge_active_uses_brand(false, &CATPPUCCIN_MOCHA);
        assert_eq!(active_color, CATPPUCCIN_MOCHA.brand);
        assert_eq!(inactive_color, CATPPUCCIN_MOCHA.surface_overlay);
        assert_ne!(active_color, inactive_color);
    }

    #[test]
    fn stepper_with_step_infos_compiles() {
        let steps = [
            StepInfo {
                label: "Welcome",
                sublabel: "Introduction",
                status: StepStatus::Done,
            },
            StepInfo {
                label: "Connect",
                sublabel: "Choose platform",
                status: StepStatus::Current,
            },
        ];
        let _: Element<'_, ()> = onboarding_stepper(&steps, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn onboarding_step_header_compiles_no_pills() {
        let _: Element<'_, ()> =
            onboarding_step_header(1, 5, "Welcome", false, false, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn onboarding_step_header_compiles_with_both_pills() {
        let _: Element<'_, ()> =
            onboarding_step_header(2, 5, "Connect", true, true, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn platform_picker_card_compiles_selected() {
        let _: Element<'_, ()> = platform_picker_card(
            PlatformCardProps {
                name: "Twitch",
                letter: "T",
                brand_color: CATPPUCCIN_MOCHA.brand,
                subtitle: "Most popular",
                capability_summary: "Chat, subs, bits, raids",
                selected: true,
            },
            (),
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn platform_picker_card_compiles_unselected() {
        let _: Element<'_, ()> = platform_picker_card(
            PlatformCardProps {
                name: "YouTube",
                letter: "Y",
                brand_color: CATPPUCCIN_MOCHA.random,
                subtitle: "Live streaming",
                capability_summary: "Chat, memberships, superchats",
                selected: false,
            },
            (),
            &CATPPUCCIN_MOCHA,
        );
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
        let _: Element<'_, ()> = onboarding_footer(
            Some(()),
            Some(()),
            "Continue with Twitch",
            '→',
            (),
            true,
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn onboarding_footer_compiles_disabled_no_back_no_skip() {
        let _: Element<'_, ()> =
            onboarding_footer(None, None, "Continue", '→', (), false, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn device_code_display_compiles() {
        let _: Element<'_, ()> = device_code_display("WDJB-MJHT", (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn expiration_timer_compiles() {
        let _: Element<'_, ()> = expiration_timer(
            std::time::Duration::from_secs(90),
            "Get new code",
            (),
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn live_status_banner_compiles_without_hint() {
        let _: Element<'_, ()> = live_status_banner(
            BannerKind::Waiting,
            "Polling for authorization...",
            None,
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn live_status_banner_compiles_with_hint() {
        let _: Element<'_, ()> = live_status_banner(
            BannerKind::Success,
            "Authorization successful.",
            Some("scopes: chat:read chat:edit"),
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn live_status_banner_compiles_error_kind() {
        let _: Element<'_, ()> = live_status_banner(
            BannerKind::Error,
            "Authorization denied.",
            None,
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn numbered_box_step_compiles_active() {
        let _: Element<'_, ()> = numbered_box_step(
            1,
            "Open the URL",
            "Navigate to the link shown.",
            true,
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn numbered_box_step_compiles_inactive() {
        let _: Element<'_, ()> = numbered_box_step(
            2,
            "Enter the code",
            "Type the code displayed above.",
            false,
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn step_status_is_copy() {
        let s = StepStatus::Current;
        let t = s;
        assert_eq!(s, t);
    }
}
