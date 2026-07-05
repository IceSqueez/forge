use iced::{
    Border, Element, Length,
    widget::{button, container, row, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, FONT_SM, Spacing, sp, spf};

pub struct BreadcrumbCrumb<Msg> {
    pub label: String,
    pub on_press: Option<Msg>,
}

impl<Msg> BreadcrumbCrumb<Msg> {
    pub fn leaf(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_press: None,
        }
    }

    pub fn link(label: impl Into<String>, on_press: Msg) -> Self {
        Self {
            label: label.into(),
            on_press: Some(on_press),
        }
    }
}

pub fn breadcrumb<'a, Msg: 'a + Clone>(
    crumbs: Vec<BreadcrumbCrumb<Msg>>,
    right: Option<Element<'a, Msg>>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let last_idx = crumbs.len().saturating_sub(1);

    let mut crumb_row = row![tabler_icon(Icon::Home, 13.0, p.text_faint)]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::alignment::Vertical::Center);

    for (i, crumb) in crumbs.into_iter().enumerate() {
        let is_last = i == last_idx;
        crumb_row = crumb_row.push(tabler_icon(Icon::ChevronRight, 11.0, p.text_faint));

        let color = if is_last {
            p.text_primary
        } else {
            p.text_muted
        };
        let label_el: Element<'a, Msg> = match crumb.on_press {
            Some(msg) => button(text(crumb.label).size(FONT_SM).color(color))
                .on_press(msg)
                .padding(0)
                .style(move |_theme: &iced::Theme, status| {
                    let text_color = match status {
                        button::Status::Hovered => p.text_primary,
                        _ => color,
                    };
                    button::Style {
                        background: None,
                        text_color,
                        border: Border::default(),
                        shadow: iced::Shadow::default(),
                        snap: false,
                    }
                })
                .into(),
            None => text(crumb.label).size(FONT_SM).color(color).into(),
        };
        crumb_row = crumb_row.push(label_el);
    }

    let inner: Element<'a, Msg> = match right {
        Some(right_el) => row![
            crumb_row,
            iced::widget::Space::new().width(Length::Fill),
            right_el,
        ]
        .align_y(iced::alignment::Vertical::Center)
        .into(),
        None => crumb_row.into(),
    };

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(p.shell)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}
