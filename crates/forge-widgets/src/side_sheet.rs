use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::{Space, container, mouse_area, stack},
};

use crate::palette::ForgePalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetEdge {
    Left,
    Right,
}

pub fn side_sheet<'a, Msg: Clone + 'a>(
    content: Element<'a, Msg>,
    on_dismiss: Msg,
    edge: SheetEdge,
    width: f32,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;

    let backdrop = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(Background::Color(Color { a: 0.45, ..p.shell })),
                ..container::Style::default()
            }),
    )
    .on_press(on_dismiss);

    let border_color = p.border_input;
    let panel = container(content)
        .width(Length::Fixed(width))
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(p.base)),
            border: Border {
                color: border_color,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    let align = match edge {
        SheetEdge::Left => Alignment::Start,
        SheetEdge::Right => Alignment::End,
    };

    let positioned = container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(align);

    stack![backdrop, positioned].into()
}
