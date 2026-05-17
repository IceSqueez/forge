use iced::{
    Border, Color, Element,
    widget::button::Status as ButtonStatus,
    widget::button::Style as ButtonStyle,
    widget::{Space, button, column, container, row, scrollable, text},
};

use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, FontRole, Radius, font, radius};

pub const SIDEBAR_WIDTH: u16 = 200;

pub fn sidebar<'a, Msg: 'a>(
    sections: Vec<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.shell;
    let border_color = palette.border_regular;
    let content = column(sections).spacing(4);

    container(scrollable(content).height(iced::Length::Fill))
        .width(u32::from(SIDEBAR_WIDTH))
        .height(iced::Length::Fill)
        .padding(12)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn sidebar_section<'a, Msg: 'a>(
    title: &str,
    items: Vec<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let header_color = palette.text_faint;
    let header = text(title.to_uppercase())
        .size(11)
        .font(font(FontRole::Monospace))
        .color(header_color);

    let mut col = column![header].spacing(2).padding([0_u16, 4]);
    for item in items {
        col = col.push(item);
    }

    col.into()
}

pub fn tree_node<'a, Msg: 'a + Clone>(
    label: &'a str,
    depth: u8,
    expanded: bool,
    on_toggle: Option<Msg>,
    on_select: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let indent = f32::from(depth) * 16.0;
    let text_color = palette.text_secondary;
    let text_hover = palette.text_primary;
    let bg_hover = Color {
        a: 0.08,
        ..palette.brand
    };
    let hover_radius = radius(Radius::Sm);

    let chevron: Element<'_, Msg> = match on_toggle {
        Some(toggle_msg) => {
            let ch = if expanded { '▾' } else { '▸' };
            let ch_color = palette.text_muted;
            let ch_hover = palette.text_secondary;
            button(text(ch.to_string()).size(12).color(ch_color))
                .on_press(toggle_msg)
                .padding([2, 4])
                .style(move |_theme: &iced::Theme, status| match status {
                    ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
                        background: None,
                        text_color: ch_hover,
                        border: Border::default(),
                        shadow: iced::Shadow::default(),
                        snap: false,
                    },
                    _ => ButtonStyle {
                        background: None,
                        text_color: ch_color,
                        border: Border::default(),
                        shadow: iced::Shadow::default(),
                        snap: false,
                    },
                })
                .into()
        }
        None => Space::new().width(16).into(),
    };

    let label_text = text(label).size(13).color(text_color);
    let inner = row![chevron, label_text]
        .spacing(2)
        .align_y(iced::Alignment::Center);

    let row_with_indent = row![Space::new().width(indent), inner].align_y(iced::Alignment::Center);

    button(row_with_indent)
        .on_press(on_select)
        .padding([4, 6])
        .width(iced::Length::Fill)
        .style(move |_theme: &iced::Theme, status| match status {
            ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
                background: Some(iced::Background::Color(bg_hover)),
                text_color: text_hover,
                border: Border {
                    radius: hover_radius.into(),
                    ..Border::default()
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            _ => ButtonStyle {
                background: None,
                text_color,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use crate::tokens::radius;

    #[test]
    fn sidebar_width_constant_matches_design() {
        assert_eq!(SIDEBAR_WIDTH, 200);
    }

    #[test]
    fn tree_node_hover_radius_matches_design() {
        assert_eq!(radius(Radius::Sm), 6.0);
    }

    #[test]
    fn sidebar_builds_with_unit_msg() {
        let _: Element<'_, ()> = sidebar(vec![], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn sidebar_section_builds_with_unit_msg() {
        let _: Element<'_, ()> = sidebar_section("Platforms", vec![], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn tree_node_collapsed_no_toggle() {
        let _: Element<'_, ()> = tree_node("Actions", 0, false, None, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn tree_node_expanded_with_toggle() {
        let _: Element<'_, ()> = tree_node("Triggers", 1, true, Some(()), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn tree_node_deep_indent() {
        let _: Element<'_, ()> = tree_node("Sub-action", 3, false, None, (), &CATPPUCCIN_MOCHA);
    }
}
