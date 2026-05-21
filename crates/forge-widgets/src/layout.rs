use iced::{
    Border, Element, Length,
    widget::{Row, Space, column, container, row, text},
};

use crate::icons::{BOOTSTRAP_FONT, ICON_CLOCK};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, Density, Radius, Spacing, radius, spacing};

pub fn title_bar<'a, Msg: 'a>(
    title: &str,
    actions: Vec<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let shell = palette.shell;
    let text_primary = palette.text_primary;

    let title_text = text(title.to_owned()).size(14).color(text_primary);

    let mut action_row: Row<'a, Msg> = row([]).spacing(4);
    for action in actions {
        action_row = action_row.push(action);
    }

    let content = row![title_text, Space::new().width(Length::Fill), action_row,]
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

pub(crate) fn logo_box<'a, Msg: 'a>(letter: char, palette: &ForgePalette) -> Element<'a, Msg> {
    let bg = palette.brand;
    let fg = palette.shell;
    container(text(letter.to_string()).size(11).color(fg))
        .width(18)
        .height(18)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                radius: radius(Radius::Xs).into(),
                ..Border::default()
            },
            ..Default::default()
        })
        .into()
}

pub fn title_bar_with_logo<'a, Msg: 'a>(
    title: &str,
    subtitle: &str,
    logo_letter: char,
    actions: Vec<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let shell = palette.shell;
    let border_color = palette.border_regular;
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;
    let horiz = spacing(Spacing::Md, Density::Cozy);

    let logo = logo_box(logo_letter, palette);
    let title_text = text(title.to_owned()).size(14).color(text_primary);
    let subtitle_text = text(format!("— {subtitle}")).size(12).color(text_muted);

    let mut action_row: Row<'a, Msg> = row([]).spacing(4);
    for action in actions {
        action_row = action_row.push(action);
    }

    let left = row![logo, title_text, subtitle_text]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    let content = row![left, Space::new().width(Length::Fill), action_row]
        .align_y(iced::Alignment::Center)
        .padding([10, horiz]);

    container(content)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(shell)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub struct TitleBarV2<'a, Msg> {
    pub breadcrumb_icon: char,
    pub breadcrumb_label: &'a str,
    pub connected: (u8, u8),
    pub uptime: String,
    pub _msg: std::marker::PhantomData<Msg>,
}

pub fn title_bar_v2<'a, Msg: 'a>(
    palette: &'a ForgePalette,
    props: TitleBarV2<'a, Msg>,
) -> Element<'a, Msg> {
    let shell = palette.shell;
    let border_color = palette.border_regular;
    let text_primary = palette.text_primary;
    let text_secondary = palette.text_secondary;
    let text_muted = palette.text_muted;
    let text_faint = palette.text_faint;
    let success = palette.success;

    let icon_el = text(props.breadcrumb_icon)
        .font(BOOTSTRAP_FONT)
        .size(13)
        .color(text_primary);
    let label_el = text(props.breadcrumb_label).size(12).color(text_primary);
    let left = row![icon_el, label_el]
        .spacing(8)
        .align_y(iced::Alignment::Center);

    let is_empty = props.connected.0 == 0;
    let dot_color = if is_empty { text_muted } else { success };
    let connected_text = if is_empty {
        "No connections".to_owned()
    } else {
        format_connected(props.connected)
    };
    let dot = crate::status::status_dot(dot_color, 7.0);
    let connected_label = text(connected_text).size(11).color(text_secondary);
    let connected_pill = row![dot, connected_label]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    let sep = text("·").size(11).color(text_faint);

    let clock_icon = text(ICON_CLOCK)
        .font(BOOTSTRAP_FONT)
        .size(12)
        .color(text_muted);
    let uptime_label = text(props.uptime).size(11).color(text_muted);
    let uptime_row = row![clock_icon, uptime_label]
        .spacing(5)
        .align_y(iced::Alignment::Center);

    let right = row![connected_pill, sep, uptime_row]
        .spacing(14)
        .align_y(iced::Alignment::Center);

    let content = row![left, Space::new().width(Length::Fill), right]
        .align_y(iced::Alignment::Center)
        .padding([10, 16]);

    container(content)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(shell)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn format_connected(connected: (u8, u8)) -> String {
    format!("Connected ({}/{})", connected.0, connected.1)
}

pub fn toolbar<'a, Msg: 'a>(
    left: Vec<Element<'a, Msg>>,
    right: Vec<Element<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let elevated = palette.elevated;
    let border_color = palette.border_regular;

    let mut left_row: Row<'a, Msg> = row([]).spacing(4).align_y(iced::Alignment::Center);
    for item in left {
        left_row = left_row.push(item);
    }

    let mut right_row: Row<'a, Msg> = row([]).spacing(4).align_y(iced::Alignment::Center);
    for item in right {
        right_row = right_row.push(item);
    }

    let content = row![left_row, Space::new().width(Length::Fill), right_row,]
        .align_y(iced::Alignment::Center)
        .padding([4, 8]);

    container(content)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(elevated)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

pub fn breadcrumb<'a, Msg: 'a + Clone>(
    segments: Vec<(String, Option<Msg>)>,
    palette: &ForgePalette,
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
                            snap: false,
                        }
                    })
                    .into()
            }
            None => text(label).size(12).color(text_secondary).into(),
        };

        content = content.push(segment_element);

        if i < last_idx {
            // Tabler ti-chevron-right deferred until icon font wiring
            content = content.push(text(" / ").size(12).color(sep_color));
        }
    }

    content.into()
}

pub fn page_shell<'a, Msg: 'a>(
    title_bar_el: Element<'a, Msg>,
    toolbar_el: Option<Element<'a, Msg>>,
    sidebar_el: Element<'a, Msg>,
    content_el: Element<'a, Msg>,
) -> Element<'a, Msg> {
    let body = row![sidebar_el, content_el].height(Length::Fill);

    let mut shell_col = column![title_bar_el];
    if let Some(tb) = toolbar_el {
        shell_col = shell_col.push(tb);
    }
    shell_col = shell_col.push(body);

    shell_col.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::ICON_HOME;
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
    fn title_bar_with_logo_compiles() {
        let _: Element<'_, ()> =
            title_bar_with_logo("forge", "Home", 'S', vec![], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn title_bar_with_logo_compiles_with_actions() {
        let action: Element<'_, ()> = iced::widget::button("X").on_press(()).into();
        let _: Element<'_, ()> =
            title_bar_with_logo("forge", "Settings", 'S', vec![action], &CATPPUCCIN_MOCHA);
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
        let segments = vec![
            ("Home".to_string(), Some(())),
            ("Actions".to_string(), None),
        ];
        let _: Element<'_, ()> = breadcrumb(segments, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn breadcrumb_compiles_with_all_clickable() {
        let segments = vec![
            ("Home".to_string(), Some(())),
            ("Platforms".to_string(), Some(())),
            ("Twitch".to_string(), Some(())),
        ];
        let _: Element<'_, ()> = breadcrumb(segments, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn breadcrumb_compiles_with_empty_segments() {
        let _: Element<'_, ()> = breadcrumb(vec![], &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn page_shell_without_toolbar_compiles() {
        let tb: Element<'_, ()> = iced::widget::text("title").into();
        let sidebar: Element<'_, ()> = iced::widget::text("sidebar").into();
        let content: Element<'_, ()> = iced::widget::text("content").into();
        let _: Element<'_, ()> = page_shell(tb, None, sidebar, content);
    }

    #[test]
    fn page_shell_with_toolbar_compiles() {
        let tb: Element<'_, ()> = iced::widget::text("title").into();
        let bar: Element<'_, ()> = iced::widget::text("toolbar").into();
        let sidebar: Element<'_, ()> = iced::widget::text("sidebar").into();
        let content: Element<'_, ()> = iced::widget::text("content").into();
        let _: Element<'_, ()> = page_shell(tb, Some(bar), sidebar, content);
    }

    #[test]
    fn format_connected_all_connected() {
        assert_eq!(format_connected((8, 8)), "Connected (8/8)");
    }

    #[test]
    fn format_connected_partial() {
        assert_eq!(format_connected((2, 5)), "Connected (2/5)");
    }

    #[test]
    fn format_connected_none() {
        assert_eq!(format_connected((0, 3)), "Connected (0/3)");
    }

    #[test]
    fn title_bar_v2_compiles() {
        let _: Element<'_, ()> = title_bar_v2(
            &CATPPUCCIN_MOCHA,
            TitleBarV2 {
                breadcrumb_icon: ICON_HOME,
                breadcrumb_label: "Home",
                connected: (8, 8),
                uptime: "2h 14m".to_string(),
                _msg: std::marker::PhantomData,
            },
        );
    }

    #[test]
    fn title_bar_v2_partial_connected_compiles() {
        let _: Element<'_, ()> = title_bar_v2(
            &CATPPUCCIN_MOCHA,
            TitleBarV2 {
                breadcrumb_icon: ICON_HOME,
                breadcrumb_label: "Settings",
                connected: (3, 5),
                uptime: "0h 4m".to_string(),
                _msg: std::marker::PhantomData,
            },
        );
    }
}
