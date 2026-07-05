use iced::{
    Alignment, Background, Element, Length,
    widget::{Row, Space, column, container, rule},
};

use crate::palette::ForgePalette;
use crate::tokens::{Spacing, spf};

pub fn status_footer<'a, Msg: 'a>(
    left: Vec<Element<'a, Msg>>,
    right: Vec<Element<'a, Msg>>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let border_color = palette.border_regular;
    let shell_bg = palette.shell;

    let mut left_row = Row::new()
        .spacing(spf(Spacing::Sm))
        .align_y(Alignment::Center);
    for el in left {
        left_row = left_row.push(el);
    }

    let mut right_row = Row::new()
        .spacing(spf(Spacing::Sm))
        .align_y(Alignment::Center);
    for el in right {
        right_row = right_row.push(el);
    }

    let content = Row::new()
        .push(left_row)
        .push(Space::new().width(Length::Fill))
        .push(right_row)
        .align_y(Alignment::Center);

    column![
        rule::horizontal(1.0_f32).style(move |_: &iced::Theme| rule::Style {
            color: border_color,
            radius: 0.0.into(),
            fill_mode: rule::FillMode::Full,
            snap: true,
        }),
        container(content)
            .padding([spf(Spacing::Xs), spf(Spacing::Md)])
            .width(Length::Fill)
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(shell_bg)),
                ..container::Style::default()
            }),
    ]
    .into()
}
