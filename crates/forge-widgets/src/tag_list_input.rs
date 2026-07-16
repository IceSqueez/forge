use iced::{
    Background, Border, Color, Element, Length, Padding,
    widget::{button, column, container, row, text, text_input},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, FONT_SM, FontRole, Radius, Spacing, font, radius, sp, spf};

#[derive(Debug, Clone, Default)]
pub struct TagListInputState {
    pub draft: String,
}

#[derive(Debug, Clone)]
pub enum TagListInputMessage {
    DraftChanged(String),
    AddPressed,
    RemoveTag(usize),
}

/// Caller owns `tags`; widget never mutates state - handle messages externally.
pub fn tag_list_input<'a, Msg: Clone + 'a>(
    state: &'a TagListInputState,
    tags: &'a [String],
    placeholder: String,
    on_message: impl Fn(TagListInputMessage) -> Msg + 'static + Copy,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;

    let chips: Vec<Element<'a, Msg>> = tags
        .iter()
        .enumerate()
        .map(|(idx, tag)| build_chip(tag, idx, on_message, p))
        .collect();

    let chips_row = row(chips).spacing(spf(Spacing::Xxs)).wrap();

    let input = text_input(&placeholder, &state.draft)
        .on_input(move |s| on_message(TagListInputMessage::DraftChanged(s)))
        .on_submit(on_message(TagListInputMessage::AddPressed))
        .padding(Padding::from([sp(Spacing::Xs), sp(Spacing::Sm)]))
        .width(Length::Fill)
        .style(move |_theme, status| {
            let border_color = match status {
                text_input::Status::Focused { .. } => p.border_input,
                text_input::Status::Disabled => p.disabled,
                _ => p.border_input,
            };
            let value_color = match status {
                text_input::Status::Disabled => p.text_muted,
                _ => p.text_primary,
            };
            text_input::Style {
                background: Background::Color(p.shell),
                border: Border {
                    color: border_color,
                    width: BORDER_THIN,
                    radius: radius(Radius::Md).into(),
                },
                icon: p.text_muted,
                placeholder: p.text_muted,
                value: value_color,
                selection: Color { a: 0.25, ..p.brand },
            }
        });

    let add_icon = tabler_icon::<Msg>(Icon::Plus, FONT_SM, p.text_secondary);
    let add_btn = button(add_icon)
        .on_press(on_message(TagListInputMessage::AddPressed))
        .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
        .style(move |_theme: &iced::Theme, status| match status {
            button::Status::Hovered => button::Style {
                background: Some(Background::Color(Color { a: 0.08, ..p.brand })),
                text_color: p.text_primary,
                border: Border {
                    color: p.border_regular,
                    width: BORDER_THIN,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            _ => button::Style {
                background: None,
                text_color: p.text_secondary,
                border: Border {
                    color: p.border_regular,
                    width: BORDER_THIN,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        });

    let input_row = row![input, add_btn]
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::Alignment::Center);

    column![chips_row, input_row]
        .spacing(spf(Spacing::Xs))
        .into()
}

fn build_chip<'a, Msg: Clone + 'a>(
    tag: &str,
    idx: usize,
    on_message: impl Fn(TagListInputMessage) -> Msg + 'static + Copy,
    p: ForgePalette,
) -> Element<'a, Msg> {
    let x_icon = tabler_icon::<Msg>(Icon::X, 11.0, p.text_muted);
    let x_btn = button(x_icon)
        .on_press(on_message(TagListInputMessage::RemoveTag(idx)))
        .padding([0u16, 0u16])
        .style(move |_theme: &iced::Theme, _status| button::Style {
            background: None,
            text_color: p.text_muted,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let label = text(tag.to_owned())
        .size(FONT_SM)
        .color(p.text_primary)
        .font(font(FontRole::Body));

    let content = row![label, x_btn]
        .spacing(4)
        .align_y(iced::Alignment::Center);

    container(content)
        .padding(Padding::from([sp(Spacing::Xxs), sp(Spacing::Sm)]))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(p.surface_overlay)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius(Radius::Pill).into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_list_input_state_default_draft_empty() {
        let state = TagListInputState::default();
        assert!(state.draft.is_empty());
    }
}
