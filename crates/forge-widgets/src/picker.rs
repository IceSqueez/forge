use forge_platform_core::SectionIcon;
use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::{Space, button, column, container, row, scrollable, stack, text},
};

use crate::{
    buttons::secondary_button,
    icons::{Icon, tabler_icon},
    inputs::search_input,
    palette::ForgePalette,
    tokens::{BORDER_THIN, FONT_MD, FONT_SM, FontRole, Radius, Spacing, font, radius, sp},
};

#[derive(Debug, Clone)]
pub struct PickerItem {
    pub id: String,
    pub label: String,
    pub sublabel: Option<String>,
    pub icon: SectionIcon,
}

pub struct PickerModalProps<'a> {
    pub title: &'a str,
    pub search_value: &'a str,
    pub items: &'a [PickerItem],
    pub loading: bool,
}

pub fn picker_modal<'a, Msg: 'a + Clone>(
    props: PickerModalProps<'a>,
    on_search_change: impl Fn(String) -> Msg + 'a,
    on_select: impl Fn(usize) -> Msg + 'a,
    on_cancel: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let PickerModalProps {
        title,
        search_value,
        items,
        loading,
    } = props;
    let p = *palette;
    let cancel_for_backdrop = on_cancel.clone();

    let header = container(
        text(title)
            .size(FONT_MD)
            .color(p.text_primary)
            .font(font(FontRole::Body)),
    )
    .padding([sp(Spacing::Md), sp(Spacing::Md)])
    .width(Length::Fill)
    .style(move |_theme: &iced::Theme| container::Style {
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let search_row = container(search_input(
        crate::tr!("widget.picker.search_placeholder"),
        search_value,
        on_search_change,
        palette,
    ))
    .padding([sp(Spacing::Sm), sp(Spacing::Md)])
    .width(Length::Fill)
    .style(move |_theme: &iced::Theme| container::Style {
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let list_area: Element<'a, Msg> = if loading {
        container(
            text(crate::tr!("widget.picker.loading"))
                .size(FONT_SM)
                .color(p.text_muted)
                .font(font(FontRole::Body)),
        )
        .width(Length::Fill)
        .height(200.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    } else {
        let lower_search = search_value.to_lowercase();
        let filtered: Vec<(usize, &PickerItem)> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                lower_search.is_empty()
                    || item.label.to_lowercase().contains(&lower_search)
                    || item
                        .sublabel
                        .as_deref()
                        .map(|s| s.to_lowercase().contains(&lower_search))
                        .unwrap_or(false)
            })
            .collect();

        if filtered.is_empty() {
            container(
                text(crate::tr!("widget.picker.no_results"))
                    .size(FONT_SM)
                    .color(p.text_muted)
                    .font(font(FontRole::Body)),
            )
            .width(Length::Fill)
            .height(120.0)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
        } else {
            let list_col = filtered
                .into_iter()
                .fold(column![].spacing(0), |col, (idx, item)| {
                    col.push(item_row(idx, item, p, &on_select))
                });
            scrollable(list_col).height(320.0).into()
        }
    };

    let footer = container(
        row![
            Space::new().width(Length::Fill),
            secondary_button(crate::tr!("widget.confirm.cancel"), on_cancel, palette),
        ]
        .align_y(Alignment::Center),
    )
    .padding([sp(Spacing::Sm), sp(Spacing::Md)])
    .width(Length::Fill)
    .style(move |_theme: &iced::Theme| container::Style {
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let card = container(column![header, search_row, list_area, footer])
        .max_width(480)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    let centered_card = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(cancel_for_backdrop)
        .padding(0)
        .style(|_theme: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.6,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    stack![backdrop, centered_card].into()
}

fn item_row<'a, Msg: Clone + 'a>(
    idx: usize,
    item: &'a PickerItem,
    p: ForgePalette,
    on_select: &impl Fn(usize) -> Msg,
) -> Element<'a, Msg> {
    let icon_el = container(tabler_icon(
        Icon::from_name(item.icon.as_str()),
        14.0,
        p.text_secondary,
    ))
    .width(28)
    .height(28)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(p.surface_overlay)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let mut label_col = column![
        text(item.label.as_str())
            .size(FONT_SM)
            .color(p.text_primary)
            .font(font(FontRole::Body))
    ]
    .spacing(2);

    if let Some(sub) = item.sublabel.as_deref() {
        label_col = label_col.push(
            text(sub)
                .size(FONT_SM)
                .color(p.text_muted)
                .font(font(FontRole::Body)),
        );
    }

    let row_inner = row![icon_el, container(label_col).width(Length::Fill)]
        .spacing(10)
        .align_y(Alignment::Center);

    let msg = on_select(idx);
    button(row_inner)
        .on_press(msg)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(Background::Color(Color { a: 0.08, ..p.brand }))
                }
                _ => None,
            };
            button::Style {
                background: bg,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(Radius::Sm).into(),
                },
                text_color: p.text_primary,
                shadow: iced::Shadow::default(),
                snap: false,
            }
        })
        .into()
}
