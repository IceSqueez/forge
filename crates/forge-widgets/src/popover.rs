use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{button, column, container, row, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::tokens::{Density, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, spacing};

pub enum MenuItem<Msg> {
    Item {
        label: String,
        on_press: Msg,
        icon: Option<Icon>,
        shortcut: Option<String>,
        color: Option<Color>,
        disabled: bool,
    },
    Divider,
    Header(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPlacement {
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

pub struct RowAction<Msg> {
    pub icon: Icon,
    pub label: String,
    pub on_press: Msg,
    pub color: Option<Color>,
}

pub fn actionable_count<Msg>(items: &[MenuItem<Msg>]) -> usize {
    items
        .iter()
        .filter(|item| {
            matches!(
                **item,
                MenuItem::Item {
                    disabled: false,
                    ..
                }
            )
        })
        .count()
}

fn divider_el<'a, Msg: 'a>(palette: &'a ForgePalette) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let border_color = palette.border_regular;
    container(
        container(iced::widget::Space::new().width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(1.0))
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(Background::Color(border_color)),
                ..container::Style::default()
            }),
    )
    .padding(Padding {
        top: xs / 2.0,
        right: 0.0,
        bottom: xs / 2.0,
        left: 0.0,
    })
    .into()
}

fn header_el<'a, Msg: 'a>(label: String, palette: &'a ForgePalette) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let sm = spacing(Spacing::Sm, Density::Cozy) as f32;
    container(
        text(label)
            .size(FONT_XS)
            .font(font(FontRole::Monospace))
            .color(palette.text_muted),
    )
    .padding(Padding {
        top: xs,
        right: sm,
        bottom: xs / 2.0,
        left: sm,
    })
    .into()
}

fn item_el<'a, Msg: Clone + 'a>(
    label: String,
    on_press: Msg,
    icon: Option<Icon>,
    shortcut: Option<String>,
    item_color: Option<Color>,
    disabled: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let sm = spacing(Spacing::Sm, Density::Cozy) as f32;

    let text_color = if disabled {
        palette.text_faint
    } else {
        item_color.unwrap_or(palette.text_primary)
    };
    let icon_color = if disabled {
        palette.text_faint
    } else {
        item_color.unwrap_or(palette.text_secondary)
    };
    let faint = palette.text_faint;
    let surface_overlay = palette.surface_overlay;

    let mut children: Vec<Element<'a, Msg>> = Vec::new();

    if let Some(ic) = icon {
        children.push(tabler_icon(ic, FONT_SM, icon_color));
    }

    children.push(
        text(label)
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(text_color)
            .width(Length::Fill)
            .into(),
    );

    if let Some(sc) = shortcut {
        children.push(
            text(sc)
                .size(FONT_XS)
                .font(font(FontRole::Monospace))
                .color(faint)
                .into(),
        );
    }

    let content_row = row(children).spacing(sm).align_y(Alignment::Center);

    let inner = container(content_row).padding(Padding {
        top: xs,
        right: sm,
        bottom: xs,
        left: sm,
    });

    let mut btn =
        button(inner)
            .width(Length::Fill)
            .padding(0)
            .style(move |_theme: &iced::Theme, status| button::Style {
                background: if !disabled {
                    match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            Some(Background::Color(surface_overlay))
                        }
                        _ => None,
                    }
                } else {
                    None
                },
                text_color,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            });

    if !disabled {
        btn = btn.on_press(on_press);
    }

    btn.into()
}

fn panel_el<'a, Msg: Clone + 'a>(
    items: Vec<MenuItem<Msg>>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let elevated = palette.elevated;
    let border_color = palette.border_input;

    let item_els: Vec<Element<'a, Msg>> = items
        .into_iter()
        .map(|item| match item {
            MenuItem::Divider => divider_el(palette),
            MenuItem::Header(label) => header_el(label, palette),
            MenuItem::Item {
                label,
                on_press,
                icon,
                shortcut,
                color,
                disabled,
            } => item_el(label, on_press, icon, shortcut, color, disabled, palette),
        })
        .collect();

    let col = column(item_els).padding(Padding {
        top: xs,
        right: 0.0,
        bottom: xs,
        left: 0.0,
    });

    container(col)
        .width(Length::Fixed(200.0))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_color,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn menu_panel<'a, Msg: Clone + 'a>(
    items: Vec<MenuItem<Msg>>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    panel_el(items, palette)
}

pub fn menu_button_trigger<'a, Msg: Clone + 'a>(
    trigger_icon: Icon,
    open: bool,
    on_toggle: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let surface_overlay = palette.surface_overlay;
    let faint = palette.text_faint;

    button(
        container(tabler_icon(trigger_icon, FONT_SM, faint))
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .on_press(on_toggle)
    .padding(0)
    .style(move |_theme: &iced::Theme, status| button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(surface_overlay))
            }
            _ => {
                if open {
                    Some(Background::Color(surface_overlay))
                } else {
                    None
                }
            }
        },
        text_color: faint,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Sm).into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    })
    .into()
}

pub fn menu_button<'a, Msg: Clone + 'a>(
    trigger_icon: Icon,
    open: bool,
    on_toggle: Msg,
    _on_dismiss: Msg,
    items: Vec<MenuItem<Msg>>,
    placement: MenuPlacement,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let surface_overlay = palette.surface_overlay;
    let faint = palette.text_faint;

    let trigger_btn: Element<'a, Msg> = button(
        container(tabler_icon(trigger_icon, FONT_SM, faint))
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .on_press(on_toggle)
    .padding(0)
    .style(move |_theme: &iced::Theme, status| button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(surface_overlay))
            }
            _ => {
                if open {
                    Some(Background::Color(surface_overlay))
                } else {
                    None
                }
            }
        },
        text_color: faint,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Sm).into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    })
    .into();

    if !open {
        return trigger_btn;
    }

    let panel = panel_el(items, palette);

    match placement {
        MenuPlacement::BottomLeft => column![trigger_btn, panel].align_x(Alignment::Start).into(),
        MenuPlacement::BottomRight => column![trigger_btn, panel].align_x(Alignment::End).into(),
        MenuPlacement::TopLeft => column![panel, trigger_btn].align_x(Alignment::Start).into(),
        MenuPlacement::TopRight => column![panel, trigger_btn].align_x(Alignment::End).into(),
    }
}

pub fn row_actions<'a, Msg: Clone + 'a>(
    actions: Vec<RowAction<Msg>>,
    hovered: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let xs = spacing(Spacing::Xs, Density::Cozy) as f32;
    let surface_overlay = palette.surface_overlay;
    let primary = palette.text_primary;

    let btns: Vec<Element<'a, Msg>> = actions
        .into_iter()
        .map(|action| {
            let default_color = if hovered {
                action.color.unwrap_or(palette.text_secondary)
            } else {
                palette.text_faint
            };
            let hover_color = action.color.unwrap_or(primary);

            button(
                container(tabler_icon(action.icon, FONT_SM, default_color)).padding(Padding {
                    top: xs,
                    right: xs,
                    bottom: xs,
                    left: xs,
                }),
            )
            .on_press(action.on_press)
            .padding(0)
            .style(move |_theme: &iced::Theme, status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Some(Background::Color(surface_overlay))
                    }
                    _ => None,
                },
                text_color: match status {
                    button::Status::Hovered | button::Status::Pressed => hover_color,
                    _ => default_color,
                },
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(Radius::Sm).into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            })
            .into()
        })
        .collect();

    row(btns).spacing(xs).align_y(Alignment::Center).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::Icon;

    #[test]
    fn menu_item_divider_is_constructable() {
        let _d: MenuItem<()> = MenuItem::Divider;
    }

    #[test]
    fn menu_item_header_carries_label() {
        let h: MenuItem<()> = MenuItem::Header("Section".to_string());
        assert!(matches!(h, MenuItem::Header(ref s) if s == "Section"));
    }

    #[test]
    fn menu_item_enabled_item_matches_disabled_false() {
        let i: MenuItem<u32> = MenuItem::Item {
            label: "Rename".to_string(),
            on_press: 1,
            icon: Some(Icon::InfoCircle),
            shortcut: None,
            color: None,
            disabled: false,
        };
        assert!(matches!(
            i,
            MenuItem::Item {
                disabled: false,
                ..
            }
        ));
    }

    #[test]
    fn menu_item_disabled_item_matches_disabled_true() {
        let i: MenuItem<u32> = MenuItem::Item {
            label: "Delete".to_string(),
            on_press: 2,
            icon: None,
            shortcut: Some("Del".to_string()),
            color: None,
            disabled: true,
        };
        assert!(matches!(i, MenuItem::Item { disabled: true, .. }));
    }

    #[test]
    fn menu_placement_all_variants_are_distinct() {
        assert_ne!(MenuPlacement::BottomLeft, MenuPlacement::BottomRight);
        assert_ne!(MenuPlacement::TopLeft, MenuPlacement::TopRight);
        assert_ne!(MenuPlacement::BottomLeft, MenuPlacement::TopLeft);
        assert_ne!(MenuPlacement::BottomRight, MenuPlacement::TopRight);
    }

    #[test]
    fn actionable_count_excludes_dividers_headers_and_disabled() {
        let items: Vec<MenuItem<u32>> = vec![
            MenuItem::Header("Section".to_string()),
            MenuItem::Item {
                label: "a".to_string(),
                on_press: 1,
                icon: None,
                shortcut: None,
                color: None,
                disabled: false,
            },
            MenuItem::Divider,
            MenuItem::Item {
                label: "b".to_string(),
                on_press: 2,
                icon: None,
                shortcut: None,
                color: None,
                disabled: true,
            },
            MenuItem::Item {
                label: "c".to_string(),
                on_press: 3,
                icon: None,
                shortcut: None,
                color: None,
                disabled: false,
            },
        ];
        assert_eq!(actionable_count(&items), 2);
    }

    #[test]
    fn actionable_count_empty_is_zero() {
        let items: Vec<MenuItem<()>> = vec![];
        assert_eq!(actionable_count(&items), 0);
    }

    #[test]
    fn actionable_count_all_non_items_is_zero() {
        let items: Vec<MenuItem<()>> = vec![
            MenuItem::Divider,
            MenuItem::Header("x".to_string()),
            MenuItem::Divider,
        ];
        assert_eq!(actionable_count(&items), 0);
    }
}
