use std::borrow::Cow;

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, Density, FONT_LG, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius,
    spacing, spf,
};

fn card_style(
    bg: iced::Color,
    border_color: iced::Color,
    r: f32,
) -> impl Fn(&iced::Theme) -> container::Style {
    move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            color: border_color,
            width: BORDER_THIN,
            radius: r.into(),
        },
        ..container::Style::default()
    }
}

pub fn card<'a, Msg: 'a>(
    children: impl IntoIterator<Item = Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    card_with_radius(children, palette, Radius::Md)
}

/// Variant of `card` for callers that need a non-default corner radius.
pub fn card_with_radius<'a, Msg: 'a>(
    children: impl IntoIterator<Item = Element<'a, Msg>>,
    palette: &ForgePalette,
    r: Radius,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;

    let col = iced::widget::column(children).spacing(8);

    container(col)
        .padding(spacing(Spacing::Md, Density::default()))
        .style(card_style(bg, border_color, radius(r)))
        .into()
}

pub fn metric_card<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    value: impl Into<Cow<'a, str>>,
    sublabel: Option<impl Into<Cow<'a, str>>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let label_color = palette.text_muted;
    let value_color = palette.text_primary;
    let sublabel_color = palette.text_faint;

    let label_str: Cow<'a, str> = label.into();
    let value_str: Cow<'a, str> = value.into();

    let mut col = iced::widget::column![
        iced::widget::text(label_str)
            .size(FONT_XS)
            .color(label_color),
        iced::widget::text(value_str)
            .size(FONT_LG)
            .color(value_color),
    ]
    .spacing(4);

    if let Some(sub) = sublabel {
        let sub_str: Cow<'a, str> = sub.into();
        col = col.push(
            iced::widget::text(sub_str)
                .size(FONT_XS)
                .color(sublabel_color),
        );
    }

    container(col)
        .padding(spacing(Spacing::Md, Density::default()))
        .style(card_style(bg, border_color, radius(Radius::Md)))
        .into()
}

pub fn stat_row<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    value: impl Into<Cow<'a, str>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let label_color = palette.text_muted;
    let value_color = palette.text_primary;

    let label_str: Cow<'a, str> = label.into();
    let value_str: Cow<'a, str> = value.into();

    iced::widget::row![
        iced::widget::text(label_str)
            .size(FONT_SM)
            .color(label_color),
        iced::widget::Space::new().width(iced::Length::Fill),
        iced::widget::text(value_str)
            .size(FONT_SM)
            .color(value_color),
    ]
    .align_y(iced::alignment::Vertical::Center)
    .into()
}

pub fn hero_card<'a, Msg: 'a>(
    title: impl Into<Cow<'a, str>>,
    subtitle: impl Into<Cow<'a, str>>,
    children: impl IntoIterator<Item = Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let title_color = palette.text_primary;
    let subtitle_color = palette.text_secondary;

    let title_str: Cow<'a, str> = title.into();
    let subtitle_str: Cow<'a, str> = subtitle.into();

    let header = iced::widget::column![
        iced::widget::text(title_str)
            .size(FONT_LG)
            .color(title_color),
        iced::widget::text(subtitle_str)
            .size(FONT_SM)
            .color(subtitle_color),
    ]
    .spacing(4);

    let mut col = iced::widget::column![header].spacing(16);
    for child in children {
        col = col.push(child);
    }

    container(col)
        .padding(spacing(Spacing::Lg, Density::default()))
        .style(card_style(bg, border_color, radius(Radius::Lg)))
        .into()
}

pub struct BigJumpCardProps<'a, Msg> {
    pub icon: Icon,
    pub icon_color: iced::Color,
    pub section_label: &'a str,
    pub title: &'a str,
    pub stat: String,
    pub stat_label: String,
    pub hint: &'a str,
    pub on_press: Msg,
    pub warn: bool,
}

pub fn big_jump_card<'a, Msg: Clone + 'a>(
    props: BigJumpCardProps<'a, Msg>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let surface_overlay = palette.surface_overlay;
    let text_faint = palette.text_faint;
    let text_muted = palette.text_muted;
    let text_primary = palette.text_primary;
    let icon_color = props.icon_color;
    let warning = palette.warning;
    let border_regular = palette.border_regular;
    let border_input = palette.border_input;
    let elevated = palette.elevated;

    let icon_box = container(tabler_icon(props.icon, 18.0, icon_color))
        .width(34.0)
        .height(34.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(surface_overlay)),
            border: Border {
                radius: radius(Radius::Md).into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let label_col = column![
        text(props.section_label)
            .size(FONT_XS)
            .color(text_faint)
            .font(font(FontRole::Monospace)),
        text(props.title).size(FONT_SM).color(text_primary),
    ]
    .spacing(1.0);

    let warn_el: Element<'a, Msg> = if props.warn {
        tabler_icon(Icon::AlertTriangle, 14.0, warning)
    } else {
        iced::widget::Space::new().into()
    };

    let top_row = row![icon_box, label_col, warn_el]
        .spacing(10.0)
        .align_y(Alignment::Center);

    let mono_font = iced::Font {
        family: iced::font::Family::Name("JetBrains Mono"),
        weight: iced::font::Weight::Normal,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    };

    let stat_row = row![
        text(props.stat)
            .size(24.0)
            .color(icon_color)
            .font(mono_font),
        text(props.stat_label).size(FONT_SM).color(text_muted),
    ]
    .spacing(8.0)
    .align_y(Alignment::Center);

    let hint_row = row![
        text(props.hint)
            .size(FONT_XS)
            .color(text_faint)
            .width(Length::Fill),
        tabler_icon(Icon::ArrowRight, 12.0, text_faint),
    ]
    .spacing(4.0)
    .align_y(Alignment::Center);

    let content = column![top_row, stat_row, hint_row].spacing(10.0);

    button(content)
        .on_press(props.on_press)
        .padding(iced::Padding {
            top: spf(Spacing::Md),
            right: spf(Spacing::Md),
            bottom: spf(Spacing::Md),
            left: spf(Spacing::Md),
        })
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| button::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: if matches!(status, button::Status::Hovered) {
                    border_input
                } else {
                    border_regular
                },
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            text_color: text_primary,
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use crate::tokens::{BORDER_THIN, Radius, radius};
    use iced::widget::text;

    #[test]
    fn card_compiles_with_unit_msg() {
        let _: Element<'_, ()> = card([text("content").into()], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn card_with_radius_uses_lg_radius() {
        let _: Element<'_, ()> =
            card_with_radius([text("content").into()], &CATPPUCCIN_MOCHA, Radius::Lg);
    }

    #[test]
    fn card_with_radius_uses_md_radius() {
        let _: Element<'_, ()> =
            card_with_radius([text("x").into()], &CATPPUCCIN_MOCHA, Radius::Md);
    }

    #[test]
    fn metric_card_compiles_without_sublabel() {
        let _: Element<'_, ()> = metric_card("Latency", "42 ms", None::<&str>, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn metric_card_compiles_with_sublabel() {
        let _: Element<'_, ()> =
            metric_card("Latency", "42 ms", Some("avg last 60s"), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn stat_row_compiles_with_unit_msg() {
        let _: Element<'_, ()> = stat_row("Scene", "Live Scene", &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn hero_card_compiles_with_content_slot() {
        let _: Element<'_, ()> = hero_card(
            "OBS Studio",
            "Connected · v31.0.0",
            [text("status content").into()],
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn hero_card_compiles_with_empty_children() {
        let _: Element<'_, ()> =
            hero_card("Title", "Subtitle", std::iter::empty(), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn card_accepts_multiple_children() {
        let _: Element<'_, u32> = card(
            [
                text("row 1").into(),
                text("row 2").into(),
                text("row 3").into(),
            ],
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn stat_row_accepts_owned_strings() {
        let label = String::from("Followers");
        let value = String::from("12 345");
        let _: Element<'_, ()> = stat_row(label.as_str(), value.as_str(), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn border_thin_constant_is_half() {
        assert_eq!(BORDER_THIN, 0.5);
    }

    #[test]
    fn metric_card_radius_token_is_md() {
        assert_eq!(radius(Radius::Md), 8.0);
    }

    #[test]
    fn hero_card_radius_token_is_lg() {
        assert_eq!(radius(Radius::Lg), 12.0);
    }

    #[test]
    fn card_default_radius_token_is_md() {
        assert_eq!(radius(Radius::Md), 8.0);
    }

    #[test]
    fn big_jump_card_compiles_no_warn() {
        let _: Element<'_, ()> = big_jump_card(
            BigJumpCardProps {
                icon: Icon::MessageCircle,
                icon_color: CATPPUCCIN_MOCHA.brand,
                section_label: "AUDIENCE",
                title: "Chat",
                stat: "1,284".to_string(),
                stat_label: "viewers tracked".to_string(),
                hint: "Talk to your audience and see who's watching",
                on_press: (),
                warn: false,
            },
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn big_jump_card_compiles_with_warn() {
        let _: Element<'_, ()> = big_jump_card(
            BigJumpCardProps {
                icon: Icon::Plug,
                icon_color: CATPPUCCIN_MOCHA.success,
                section_label: "CONNECTIONS",
                title: "Connections",
                stat: "1/6".to_string(),
                stat_label: "connected".to_string(),
                hint: "Manage platforms, apps and modules",
                on_press: (),
                warn: true,
            },
            &CATPPUCCIN_MOCHA,
        );
    }
}
