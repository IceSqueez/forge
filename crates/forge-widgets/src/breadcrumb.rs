use iced::{
    Border, Element, Length,
    widget::{container, row, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, FONT_SM, Spacing, sp};

pub struct BreadcrumbCrumb<'a, Msg> {
    pub icon: Option<Icon>,
    pub label: &'a str,
    pub on_press: Option<Msg>,
}

pub fn breadcrumb<'a, Msg: 'a + Clone>(
    crumbs: Vec<BreadcrumbCrumb<'a, Msg>>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let shell = palette.shell;
    let border_color = palette.border_regular;
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;
    let text_faint = palette.text_faint;

    let last_idx = crumbs.len().saturating_sub(1);
    let mut content = row([]).spacing(6).align_y(iced::Alignment::Center);

    for (i, crumb) in crumbs.into_iter().enumerate() {
        let is_last = i == last_idx;
        let label_color = if is_last { text_primary } else { text_muted };

        if let Some(icon) = crumb.icon {
            content = content.push(tabler_icon(icon, 13.0, text_faint));
        }

        let label_el: Element<'a, Msg> = match crumb.on_press {
            Some(msg) => {
                let fg = text_muted;
                let fg_hover = text_primary;
                iced::widget::button(text(crumb.label).size(FONT_SM).color(fg))
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
            None => text(crumb.label).size(FONT_SM).color(label_color).into(),
        };
        content = content.push(label_el);

        if !is_last {
            content = content.push(text(" /").size(FONT_SM).color(text_faint));
        }
    }

    container(content)
        .width(Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::Icon;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn breadcrumb_compiles_single_terminal() {
        let crumbs: Vec<BreadcrumbCrumb<'_, ()>> = vec![BreadcrumbCrumb {
            icon: Some(Icon::Home),
            label: "Home",
            on_press: None,
        }];
        let _: Element<'_, ()> = breadcrumb(crumbs, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn breadcrumb_compiles_with_clickable_parent() {
        let crumbs: Vec<BreadcrumbCrumb<'_, ()>> = vec![
            BreadcrumbCrumb {
                icon: Some(Icon::Home),
                label: "Actions",
                on_press: Some(()),
            },
            BreadcrumbCrumb {
                icon: None,
                label: "My Action",
                on_press: None,
            },
        ];
        let _: Element<'_, ()> = breadcrumb(crumbs, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn breadcrumb_compiles_empty() {
        let _: Element<'_, ()> = breadcrumb(vec![], &CATPPUCCIN_MOCHA);
    }
}
