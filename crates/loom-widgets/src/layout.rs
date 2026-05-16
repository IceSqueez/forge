use iced::{
    Element, Length,
    widget::{Row, Space, container, row, text},
};

use crate::palette::LoomPalette;

pub fn title_bar<'a, Msg: 'a>(
    title: &str,
    actions: Vec<Element<'a, Msg>>,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let shell = palette.shell;
    let text_primary = palette.text_primary;

    let title_text = text(title.to_owned()).size(14).color(text_primary);

    let mut action_row: Row<'a, Msg> = row([]).spacing(4);
    for action in actions {
        action_row = action_row.push(action);
    }

    let content = row![title_text, Space::with_width(Length::Fill), action_row,]
        .align_y(iced::Alignment::Center)
        .padding([0, 12]);

    container(content)
        .width(Length::Fill)
        .height(48)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(shell)),
            ..Default::default()
        })
        .into()
}

pub fn toolbar<'a, Msg: 'a>(
    left: Vec<Element<'a, Msg>>,
    right: Vec<Element<'a, Msg>>,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let shell = palette.shell;

    let mut left_row: Row<'a, Msg> = row([]).spacing(4).align_y(iced::Alignment::Center);
    for item in left {
        left_row = left_row.push(item);
    }

    let mut right_row: Row<'a, Msg> = row([]).spacing(4).align_y(iced::Alignment::Center);
    for item in right {
        right_row = right_row.push(item);
    }

    let content = row![left_row, Space::with_width(Length::Fill), right_row,]
        .align_y(iced::Alignment::Center)
        .padding([4, 8]);

    container(content)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(shell)),
            ..Default::default()
        })
        .into()
}

pub fn breadcrumb<'a, Msg: 'a + Clone>(
    segments: Vec<(String, Option<Msg>)>,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let text_muted = palette.text_muted;
    let text_secondary = palette.text_secondary;
    let sep_color = palette.text_faint;

    let mut content: Row<'a, Msg> = row([]).spacing(4).align_y(iced::Alignment::Center);
    let last_idx = segments.len().saturating_sub(1);

    for (i, (label, on_press)) in segments.into_iter().enumerate() {
        let segment_element: Element<'a, Msg> = match on_press {
            Some(msg) => {
                let fg = text_muted;
                let fg_hover = text_secondary;
                iced::widget::button(text(label).size(12).color(fg))
                    .on_press(msg)
                    .padding(0)
                    .style(move |_theme: &iced::Theme, status| {
                        let fg_actual = match status {
                            iced::widget::button::Status::Hovered => fg_hover,
                            _ => fg,
                        };
                        iced::widget::button::Style {
                            background: None,
                            text_color: fg_actual,
                            border: iced::Border::default(),
                            shadow: iced::Shadow::default(),
                        }
                    })
                    .into()
            }
            None => text(label).size(12).color(text_secondary).into(),
        };

        content = content.push(segment_element);

        if i < last_idx {
            content = content.push(text(" / ").size(12).color(sep_color));
        }
    }

    content.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn title_bar_compiles_with_no_actions() {
        let _: Element<'_, ()> = title_bar("Dashboard", vec![], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn title_bar_compiles_with_actions() {
        let action: Element<'_, ()> = iced::widget::button("X").on_press(()).into();
        let _: Element<'_, ()> = title_bar("Settings", vec![action], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn toolbar_compiles_with_empty_sides() {
        let _: Element<'_, ()> = toolbar(vec![], vec![], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn toolbar_compiles_with_items() {
        let left: Element<'_, ()> = iced::widget::text("left").into();
        let right: Element<'_, ()> = iced::widget::text("right").into();
        let _: Element<'_, ()> = toolbar(vec![left], vec![right], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn breadcrumb_compiles_with_static_terminal() {
        let segments = vec![("Hub".to_string(), Some(())), ("Actions".to_string(), None)];
        let _: Element<'_, ()> = breadcrumb(segments, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn breadcrumb_compiles_with_all_clickable() {
        let segments = vec![
            ("Hub".to_string(), Some(())),
            ("Platforms".to_string(), Some(())),
            ("Twitch".to_string(), Some(())),
        ];
        let _: Element<'_, ()> = breadcrumb(segments, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn breadcrumb_compiles_with_empty_segments() {
        let _: Element<'_, ()> = breadcrumb(vec![], &CATPPUCCIN_MOCHA);
    }
}
