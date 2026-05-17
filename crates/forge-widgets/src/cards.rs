use std::borrow::Cow;

use iced::{Border, Element, widget::container};

use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, Density, FONT_CAPS_SM, FONT_HERO, Radius, Spacing, radius, spacing,
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
    card_with_radius(children, palette, Radius::Xl)
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
        .padding(spacing(Spacing::Xxl, Density::default()))
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
            .size(FONT_CAPS_SM)
            .color(label_color),
        iced::widget::text(value_str)
            .size(FONT_HERO)
            .color(value_color),
    ]
    .spacing(4);

    if let Some(sub) = sublabel {
        let sub_str: Cow<'a, str> = sub.into();
        col = col.push(iced::widget::text(sub_str).size(10).color(sublabel_color));
    }

    container(col)
        .padding(spacing(Spacing::Xxxl, Density::default()))
        .style(card_style(bg, border_color, radius(Radius::Xxl)))
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
        iced::widget::text(label_str).size(13).color(label_color),
        iced::widget::Space::new().width(iced::Length::Fill),
        iced::widget::text(value_str).size(13).color(value_color),
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
            .size(FONT_HERO)
            .color(title_color),
        iced::widget::text(subtitle_str)
            .size(13)
            .color(subtitle_color),
    ]
    .spacing(4);

    let mut col = iced::widget::column![header].spacing(16);
    for child in children {
        col = col.push(child);
    }

    container(col)
        .padding(spacing(Spacing::Huge, Density::default()))
        .style(card_style(bg, border_color, radius(Radius::Hero)))
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
    fn card_with_radius_uses_hero_radius() {
        let _: Element<'_, ()> =
            card_with_radius([text("content").into()], &CATPPUCCIN_MOCHA, Radius::Hero);
    }

    #[test]
    fn card_with_radius_uses_xl_radius() {
        let _: Element<'_, ()> =
            card_with_radius([text("x").into()], &CATPPUCCIN_MOCHA, Radius::Xl);
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
    fn border_thin_constant_is_one() {
        assert_eq!(BORDER_THIN, 1.0);
    }

    #[test]
    fn metric_card_radius_token_is_xxl() {
        assert_eq!(radius(Radius::Xxl), 10.0);
    }

    #[test]
    fn hero_card_radius_token_is_hero() {
        assert_eq!(radius(Radius::Hero), 14.0);
    }

    #[test]
    fn card_default_radius_token_is_xl() {
        assert_eq!(radius(Radius::Xl), 9.0);
    }
}
