use std::borrow::Cow;

use iced::{Border, Element, widget::container};

use crate::palette::LoomPalette;

pub fn card<'a, Msg: 'a>(
    children: impl IntoIterator<Item = Element<'a, Msg>>,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;

    let col = iced::widget::column(children).spacing(8);

    container(col)
        .padding(16)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn metric_card<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    value: impl Into<Cow<'a, str>>,
    sublabel: Option<impl Into<Cow<'a, str>>>,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let label_color = palette.text_muted;
    let value_color = palette.text_primary;
    let sublabel_color = palette.text_faint;

    let label_str: Cow<'a, str> = label.into();
    let value_str: Cow<'a, str> = value.into();

    let mut col = iced::widget::column![
        iced::widget::text(label_str).size(11).color(label_color),
        iced::widget::text(value_str).size(22).color(value_color),
    ]
    .spacing(4);

    if let Some(sub) = sublabel {
        let sub_str: Cow<'a, str> = sub.into();
        col = col.push(iced::widget::text(sub_str).size(10).color(sublabel_color));
    }

    container(col)
        .padding(12)
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

pub fn stat_row<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    value: impl Into<Cow<'a, str>>,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let label_color = palette.text_muted;
    let value_color = palette.text_primary;

    let label_str: Cow<'a, str> = label.into();
    let value_str: Cow<'a, str> = value.into();

    iced::widget::row![
        iced::widget::text(label_str).size(13).color(label_color),
        iced::widget::horizontal_space(),
        iced::widget::text(value_str).size(13).color(value_color),
    ]
    .align_y(iced::alignment::Vertical::Center)
    .into()
}

pub fn hero_card<'a, Msg: 'a>(
    title: impl Into<Cow<'a, str>>,
    subtitle: impl Into<Cow<'a, str>>,
    children: impl IntoIterator<Item = Element<'a, Msg>>,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let title_color = palette.text_primary;
    let subtitle_color = palette.text_secondary;

    let title_str: Cow<'a, str> = title.into();
    let subtitle_str: Cow<'a, str> = subtitle.into();

    let header = iced::widget::column![
        iced::widget::text(title_str).size(18).color(title_color),
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
        .padding(20)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 12.0.into(),
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
    fn card_compiles_with_unit_msg() {
        let _: Element<'_, ()> = card([text("content").into()], &CATPPUCCIN_MOCHA);
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
}
