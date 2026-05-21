use forge_events::EventSource;
use iced::{
    Alignment, Border, Color, Element, Length,
    font::Style as FontStyle,
    widget::Column,
    widget::Row,
    widget::button::{Status, Style},
    widget::{Space, button, column, container, row, scrollable, stack, text, text_input},
};

use crate::{
    events::color_for_source,
    icons::{
        BOOTSTRAP_FONT, ICON_ALERT_TRIANGLE, ICON_CHECK_CIRCLE, ICON_COPY, ICON_EXTERNAL_LINK,
        ICON_EYE, ICON_EYE_SLASH, ICON_FILE_CODE, ICON_FILE_IMAGE, ICON_FOLDER, ICON_FOLDER_OPEN,
        ICON_INFO_CIRCLE, ICON_KEYBOARD, ICON_LOCK, ICON_REFRESH, ICON_X,
    },
    palette::ForgePalette,
    tokens::{BORDER_THIN, FONT_BODY, FONT_SM, FONT_XS, FontRole, Radius, font, radius},
};

fn token_box_style(bg: Color, border_color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            color: border_color,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    }
}

fn outline_btn_style(
    border_color: Color,
    normal_text: Color,
    hover_text: Color,
) -> impl Fn(&iced::Theme, Status) -> Style {
    let r = radius(Radius::Md);
    move |_theme, status| match status {
        Status::Active | Status::Pressed => Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            text_color: normal_text,
            border: Border {
                color: border_color,
                width: 0.5,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
        Status::Hovered => Style {
            background: Some(iced::Background::Color(Color {
                a: 0.06,
                ..border_color
            })),
            text_color: hover_text,
            border: Border {
                color: border_color,
                width: 0.5,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
        Status::Disabled => Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            text_color: Color {
                a: 0.4,
                ..normal_text
            },
            border: Border {
                color: Color {
                    a: 0.4,
                    ..border_color
                },
                width: 0.5,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
    }
}

fn ghost_icon_style(normal: Color, hover: Color) -> impl Fn(&iced::Theme, Status) -> Style {
    move |_theme, status| Style {
        background: match status {
            Status::Hovered => Some(iced::Background::Color(Color { a: 0.06, ..hover })),
            _ => None,
        },
        text_color: match status {
            Status::Hovered => hover,
            _ => normal,
        },
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

fn mask_token(token: &str) -> String {
    const PREFIX: &str = "fg_";
    const SUFFIX_LEN: usize = 4;

    let (prefix, body) = if let Some(rest) = token.strip_prefix(PREFIX) {
        (PREFIX, rest)
    } else {
        ("", token)
    };

    let chars: Vec<char> = body.chars().collect();
    if chars.len() <= SUFFIX_LEN {
        return token.to_owned();
    }

    let mask_len = chars.len() - SUFFIX_LEN;
    let suffix: String = chars[chars.len() - SUFFIX_LEN..].iter().collect();
    let bullets = "•".repeat(mask_len);
    format!("{prefix}{bullets}{suffix}")
}

pub fn bearer_token_display<'a, Msg: Clone + 'a>(
    token: &'a str,
    revealed: bool,
    on_toggle_reveal: Msg,
    on_copy: Msg,
    on_regenerate: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let display: String = if revealed {
        token.to_owned()
    } else {
        mask_token(token)
    };

    let eye_char = if revealed { ICON_EYE_SLASH } else { ICON_EYE };

    let icon_normal = palette.text_faint;
    let icon_hover = palette.text_secondary;

    let eye_btn = button(text(eye_char.to_string()).font(BOOTSTRAP_FONT).size(13))
        .on_press(on_toggle_reveal)
        .padding([2, 4])
        .style(ghost_icon_style(icon_normal, icon_hover));

    let token_inner = row![
        text(display)
            .font(font(FontRole::Monospace))
            .size(FONT_BODY)
            .color(palette.text_primary),
        Space::new().width(Length::Fill),
        eye_btn,
    ]
    .align_y(Alignment::Center);

    let token_box = container(token_inner)
        .width(Length::Fill)
        .padding([6, 12])
        .style(token_box_style(palette.shell, palette.border_regular));

    let border = palette.border_regular;
    let copy_normal = palette.text_secondary;
    let copy_hover = palette.text_primary;

    let copy_btn = button(
        row![
            text(ICON_COPY.to_string()).font(BOOTSTRAP_FONT).size(12),
            text("COPY").font(font(FontRole::Monospace)).size(FONT_XS),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .on_press(on_copy)
    .padding([7, 10])
    .style(outline_btn_style(border, copy_normal, copy_hover));

    let warn_color = palette.warning;

    let regen_btn = button(
        row![
            text(ICON_REFRESH.to_string()).font(BOOTSTRAP_FONT).size(12),
            text("REGENERATE")
                .font(font(FontRole::Monospace))
                .size(FONT_XS),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .on_press(on_regenerate)
    .padding([7, 10])
    .style(outline_btn_style(border, warn_color, warn_color));

    let controls = row![token_box, copy_btn, regen_btn]
        .spacing(8)
        .align_y(Alignment::Center);

    let warning_row = row![
        text(ICON_ALERT_TRIANGLE.to_string())
            .font(BOOTSTRAP_FONT)
            .size(11)
            .color(palette.warning),
        text("Regenerating disconnects all clients")
            .size(FONT_XS)
            .color(palette.text_faint),
    ]
    .spacing(5)
    .align_y(Alignment::Center);

    column![controls, warning_row].spacing(4).into()
}

const MAX_VISIBLE_CHIPS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientStatus {
    Active,
    Idle,
    Disconnecting,
}

pub struct SubscriptionChipData<'a> {
    pub label: &'a str,
    pub source: EventSource,
}

pub struct ClientRowData<'a> {
    pub status: ClientStatus,
    pub identification: &'a str,
    pub client_type_label: &'a str,
    pub subscriptions: &'a [SubscriptionChipData<'a>],
    pub events_per_second: f32,
    pub uptime_short: &'a str,
    pub highlight: bool,
}

fn dot_style(color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(color)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 3.0.into(),
        },
        ..container::Style::default()
    }
}

fn chip_bg_style(bg: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Xxl).into(),
        },
        ..container::Style::default()
    }
}

fn separator_style(color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(color)),
        ..container::Style::default()
    }
}

fn chip_element<'a, Msg: 'a>(label: &'a str, fg: Color, bg: Color) -> Element<'a, Msg> {
    container(
        text(label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(fg),
    )
    .padding([1u16, 5u16])
    .style(chip_bg_style(bg))
    .into()
}

fn more_badge<'a, Msg: 'a>(n: usize, palette: &ForgePalette) -> Element<'a, Msg> {
    let label = format!("+{n} more");
    let fg = palette.text_faint;
    let bg = palette.surface_overlay;
    container(
        text(label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(fg),
    )
    .padding([1u16, 5u16])
    .style(chip_bg_style(bg))
    .into()
}

fn chips_cell_row<'a, Msg: 'a>(
    subscriptions: &'a [SubscriptionChipData<'a>],
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.surface_overlay;

    if subscriptions.len() == 1 && subscriptions[0].label == "*" {
        let wildcard = chip_element("*", palette.success, bg);
        let hint = text("all events")
            .font(iced::Font {
                style: FontStyle::Italic,
                ..font(FontRole::Monospace)
            })
            .size(FONT_XS)
            .color(palette.text_faint);
        return row![wildcard, hint]
            .spacing(4)
            .align_y(Alignment::Center)
            .into();
    }

    let visible = subscriptions.len().min(MAX_VISIBLE_CHIPS);
    let overflow = subscriptions.len().saturating_sub(MAX_VISIBLE_CHIPS);

    let mut chips: Vec<Element<'a, Msg>> = subscriptions[..visible]
        .iter()
        .map(|c| chip_element(c.label, color_for_source(c.source, palette), bg))
        .collect();

    if overflow > 0 {
        chips.push(more_badge(overflow, palette));
    }

    Row::with_children(chips)
        .spacing(3)
        .align_y(Alignment::Center)
        .into()
}

pub fn client_table_row<'a, Msg: Clone + 'a>(
    row: &'a ClientRowData<'a>,
    on_disconnect: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let dot_color = match row.status {
        ClientStatus::Active => palette.success,
        ClientStatus::Idle => palette.warning,
        ClientStatus::Disconnecting => palette.random,
    };

    let dot = container(Space::new().width(6.0f32).height(6.0f32)).style(dot_style(dot_color));

    let dot_cell = container(dot).width(Length::Fixed(24.0));

    let id_col = column![
        text(row.identification)
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.text_primary),
        text(row.client_type_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_faint),
    ]
    .spacing(2);

    let id_cell = container(id_col).width(Length::FillPortion(14));

    let subs_cell =
        container(chips_cell_row::<Msg>(row.subscriptions, palette)).width(Length::FillPortion(16));

    let evs_label = format!("{:.1}", row.events_per_second);
    let evs_cell = container(
        text(evs_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_primary),
    )
    .width(Length::Fixed(80.0));

    let uptime_cell = container(
        text(row.uptime_short)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_muted),
    )
    .width(Length::Fixed(70.0));

    let x_btn = button(text(ICON_X.to_string()).font(BOOTSTRAP_FONT).size(13))
        .on_press(on_disconnect)
        .padding([2u16, 3u16])
        .style(ghost_icon_style(palette.text_faint, palette.text_secondary));

    let x_cell = container(x_btn).width(Length::Fixed(22.0));

    let content = row![dot_cell, id_cell, subs_cell, evs_cell, uptime_cell, x_cell]
        .align_y(Alignment::Center)
        .padding([8u16, 14u16]);

    let highlight = row.highlight;
    let shell = palette.shell;
    let elevated = palette.elevated;

    let separator =
        container(Space::new().width(Length::Fill).height(1.0f32)).style(separator_style(elevated));

    container(column![content, separator])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: if highlight {
                Some(iced::Background::Color(shell))
            } else {
                None
            },
            ..container::Style::default()
        })
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindBadge {
    Recommended,
    RequiresConfirmation,
}

pub struct BindAddressCardParams<'a> {
    pub title: &'a str,
    pub tech_label: &'a str,
    pub badge: BindBadge,
    pub description: &'a str,
    pub selected: bool,
}

fn badge_color(badge: BindBadge, palette: &ForgePalette) -> Color {
    match badge {
        BindBadge::Recommended => palette.success,
        BindBadge::RequiresConfirmation => palette.warning,
    }
}

fn badge_icon(badge: BindBadge) -> char {
    match badge {
        BindBadge::Recommended => ICON_LOCK,
        BindBadge::RequiresConfirmation => ICON_ALERT_TRIANGLE,
    }
}

fn badge_label(badge: BindBadge) -> &'static str {
    match badge {
        BindBadge::Recommended => "Recommended",
        BindBadge::RequiresConfirmation => "Requires confirmation",
    }
}

fn radio_ring_style(ring_color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(Color::TRANSPARENT)),
        border: Border {
            color: ring_color,
            width: 2.0,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

fn radio_fill_style(fill_color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(fill_color)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

fn radio_dot<'a, Msg: 'a>(selected: bool, palette: &ForgePalette) -> Element<'a, Msg> {
    let ring_color = if selected {
        palette.brand
    } else {
        palette.border_input
    };

    let inner: Element<'a, Msg> = if selected {
        container(Space::new().width(7.0f32).height(7.0f32))
            .style(radio_fill_style(palette.brand))
            .into()
    } else {
        Space::new().into()
    };

    container(inner)
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(radio_ring_style(ring_color))
        .into()
}

fn bind_badge_element<'a, Msg: 'a>(badge: BindBadge, palette: &ForgePalette) -> Element<'a, Msg> {
    let color = badge_color(badge, palette);
    let surface = palette.surface_overlay;

    let badge_row = row![
        text(badge_icon(badge).to_string())
            .font(BOOTSTRAP_FONT)
            .size(10.0f32)
            .color(color),
        text(badge_label(badge))
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(color),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    container(badge_row)
        .padding([1u16, 6u16])
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(surface)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn bind_card_style(
    selected: bool,
    brand: Color,
    border_regular: Color,
    elevated: Color,
    text_primary: Color,
) -> impl Fn(&iced::Theme, Status) -> Style {
    move |_theme, status| {
        let (border_color, border_width) = if selected {
            (brand, BORDER_THIN)
        } else {
            (border_regular, 0.5)
        };
        let bg = match status {
            Status::Hovered => Color {
                a: if selected { 0.08 } else { 0.04 },
                ..brand
            },
            Status::Pressed => Color { a: 0.12, ..brand },
            _ => elevated,
        };
        Style {
            background: Some(iced::Background::Color(bg)),
            text_color: text_primary,
            border: Border {
                color: border_color,
                width: border_width,
                radius: radius(Radius::Xl).into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    }
}

pub fn bind_address_card<'a, Msg: Clone + 'a>(
    params: BindAddressCardParams<'a>,
    on_click: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let dot = radio_dot::<Msg>(params.selected, palette);

    let title_row = row![
        text(params.title)
            .size(FONT_BODY)
            .color(palette.text_primary),
        bind_badge_element::<Msg>(params.badge, palette),
        Space::new().width(Length::Fill),
        text(params.tech_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_faint),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let description = text(params.description)
        .size(FONT_SM)
        .color(palette.text_muted);

    let content = column![title_row, description].spacing(2);

    let dot_padded = container(dot).padding(iced::Padding {
        top: 1.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    });

    let card_row = row![dot_padded, content,]
        .spacing(11)
        .align_y(Alignment::Start)
        .padding([12u16, 14u16]);

    button(card_row)
        .on_press(on_click)
        .width(Length::Fill)
        .style(bind_card_style(
            params.selected,
            palette.brand,
            palette.border_regular,
            palette.elevated,
            palette.text_primary,
        ))
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletKind {
    Check,
    Warning,
    Info,
}

pub struct BulletItem<'a> {
    pub kind: BulletKind,
    pub text: &'a str,
}

pub struct TypeToConfirmModalParams<'a> {
    pub title: &'a str,
    pub explanation: &'a str,
    pub bullets: &'a [BulletItem<'a>],
    pub confirmation_phrase: &'a str,
    pub current_input: &'a str,
    pub confirm_label: &'a str,
}

fn section_divider<'a, Msg: 'a>(border_color: Color) -> Element<'a, Msg> {
    container(Space::new().width(Length::Fill).height(0.5f32))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(border_color)),
            ..container::Style::default()
        })
        .into()
}

fn bullet_icon_and_color(kind: BulletKind, p: ForgePalette) -> (char, Color) {
    match kind {
        BulletKind::Check => (ICON_CHECK_CIRCLE, p.success),
        BulletKind::Warning => (ICON_ALERT_TRIANGLE, p.warning),
        BulletKind::Info => (ICON_INFO_CIRCLE, p.info),
    }
}

fn confirm_active_btn_style(bg: Color, fg: Color) -> impl Fn(&iced::Theme, Status) -> Style {
    let r = radius(Radius::Md);
    move |_theme, status| {
        let adjusted_bg = match status {
            Status::Hovered => Color { a: 0.85, ..bg },
            Status::Pressed => Color { a: 0.7, ..bg },
            _ => bg,
        };
        Style {
            background: Some(iced::Background::Color(adjusted_bg)),
            text_color: fg,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    }
}

fn confirm_disabled_btn_style(bg: Color, fg: Color) -> impl Fn(&iced::Theme, Status) -> Style {
    let r = radius(Radius::Md);
    move |_theme, _status| Style {
        background: Some(iced::Background::Color(bg)),
        text_color: fg,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: r.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

pub fn type_to_confirm_modal<'a, Msg: Clone + 'a>(
    params: TypeToConfirmModalParams<'a>,
    on_input_change: impl Fn(String) -> Msg + 'a,
    on_cancel: Msg,
    on_confirm: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let cancel_for_backdrop = on_cancel.clone();

    let icon_bg = Color {
        a: 0.12,
        ..p.warning
    };
    let icon_box = container(
        text(ICON_ALERT_TRIANGLE.to_string())
            .font(BOOTSTRAP_FONT)
            .size(20.0f32)
            .color(p.warning),
    )
    .width(Length::Fixed(36.0))
    .height(Length::Fixed(36.0))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(icon_bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Xl).into(),
        },
        ..container::Style::default()
    });

    let title_row = row![
        icon_box,
        text(params.title)
            .size(15.0f32)
            .color(p.text_primary)
            .font(iced::Font {
                weight: iced::font::Weight::Medium,
                ..font(FontRole::Body)
            }),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let explanation = text(params.explanation).size(FONT_SM).color(p.text_muted);

    let header_section = container(column![title_row, explanation].spacing(8))
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 18.0,
            right: 20.0,
            bottom: 14.0,
            left: 20.0,
        });

    let section_cap = text("WHAT THIS MEANS")
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let mut bullets_col = column![section_cap].spacing(0);
    for item in params.bullets {
        let (icon_char, icon_color) = bullet_icon_and_color(item.kind, p);
        let bullet_row = row![
            text(icon_char.to_string())
                .font(BOOTSTRAP_FONT)
                .size(14.0f32)
                .color(icon_color),
            text(item.text).size(FONT_SM).color(p.text_primary),
        ]
        .spacing(10)
        .align_y(Alignment::Start)
        .padding([5u16, 0u16]);
        bullets_col = bullets_col.push(bullet_row);
    }

    let risk_section = container(bullets_col)
        .width(Length::Fill)
        .padding([14u16, 20u16])
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(p.shell)),
            ..container::Style::default()
        });

    let phrase_chip = container(
        text(params.confirmation_phrase)
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(p.warning),
    )
    .padding([1u16, 6u16])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(p.surface_overlay)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Xs).into(),
        },
        ..container::Style::default()
    });

    let confirm_label_row = row![
        text("Type ").size(FONT_SM).color(p.text_primary),
        phrase_chip,
        text(" to confirm:").size(FONT_SM).color(p.text_primary),
    ]
    .align_y(Alignment::Center);

    let phrase_matches = params.current_input == params.confirmation_phrase;
    let input_border_color = if phrase_matches {
        p.brand
    } else {
        p.border_input
    };

    let confirm_input = text_input("", params.current_input)
        .on_input(on_input_change)
        .padding(iced::Padding::from([8u16, 12u16]))
        .width(Length::Fill)
        .style(move |_theme, _status| text_input::Style {
            background: iced::Background::Color(p.shell),
            border: Border {
                color: input_border_color,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            icon: p.text_muted,
            placeholder: p.text_muted,
            value: p.text_primary,
            selection: Color { a: 0.25, ..p.brand },
        });

    let confirm_section = container(column![confirm_label_row, confirm_input].spacing(8))
        .width(Length::Fill)
        .padding([14u16, 20u16]);

    let esc_hint = row![
        text(ICON_KEYBOARD.to_string())
            .font(BOOTSTRAP_FONT)
            .size(12.0f32)
            .color(p.text_faint),
        text("Esc")
            .font(font(FontRole::Monospace))
            .size(11.0f32)
            .color(p.text_faint),
        text(" to cancel").size(11.0f32).color(p.text_faint),
    ]
    .spacing(5)
    .align_y(Alignment::Center);

    let cancel_btn = button(text("Cancel").size(FONT_BODY).color(p.text_secondary))
        .on_press(on_cancel)
        .padding([7u16, 14u16])
        .style(outline_btn_style(
            p.border_regular,
            p.text_secondary,
            p.text_primary,
        ));

    let confirm_btn: Element<'a, Msg> = if phrase_matches {
        button(
            text(params.confirm_label)
                .size(FONT_BODY)
                .color(p.shell)
                .font(iced::Font {
                    weight: iced::font::Weight::Medium,
                    ..font(FontRole::Body)
                }),
        )
        .on_press(on_confirm)
        .padding([7u16, 14u16])
        .style(confirm_active_btn_style(p.warning, p.shell))
        .into()
    } else {
        button(text(params.confirm_label).size(FONT_BODY).color(p.disabled))
            .padding([7u16, 14u16])
            .style(confirm_disabled_btn_style(p.surface_overlay, p.disabled))
            .into()
    };

    let btn_row = row![cancel_btn, confirm_btn].spacing(8);

    let footer_section = container(
        row![esc_hint, Space::new().width(Length::Fill), btn_row,].align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([12u16, 20u16])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(p.shell)),
        ..container::Style::default()
    });

    let divider_color = p.border_regular;

    let card_content = column![
        header_section,
        section_divider(divider_color),
        risk_section,
        section_divider(divider_color),
        confirm_section,
        section_divider(divider_color),
        footer_section,
    ]
    .spacing(0);

    let card = container(card_content)
        .width(Length::Fixed(520.0))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(p.elevated)),
            border: Border {
                color: p.border_input,
                width: 0.5,
                radius: radius(Radius::Hero).into(),
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
        .style(|_theme: &iced::Theme, _status| Style {
            background: Some(iced::Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.55,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    stack![backdrop, centered_card].into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMime {
    Html,
    Css,
    Js,
    Json,
    Image,
    Wasm,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    File { mime: FileMime },
    Dir,
}

pub struct OverlayEntry<'a> {
    pub name: &'a str,
    pub kind: OverlayKind,
    pub size_bytes: Option<u64>,
    pub child_count: Option<usize>,
}

pub struct OverlayFileListParams<'a> {
    pub root_path: &'a str,
    pub entries: &'a [OverlayEntry<'a>],
    pub bind_address: &'a str,
    pub selected_for_url: Option<&'a str>,
}

pub(crate) fn browser_url(bind_address: &str, file_name: &str) -> String {
    format!("http://{bind_address}/{file_name}")
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * 1_024;
    const GB: u64 = 1_024 * 1_024 * 1_024;
    if bytes == 0 {
        "0 B".to_owned()
    } else if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    }
}

fn entry_icon_and_color(kind: OverlayKind, p: ForgePalette) -> (char, Color) {
    match kind {
        OverlayKind::File {
            mime: FileMime::Html,
        } => (ICON_FILE_CODE, p.bits),
        OverlayKind::File {
            mime: FileMime::Css,
        } => (ICON_FILE_CODE, p.brand),
        OverlayKind::File { mime: FileMime::Js } => (ICON_FILE_CODE, p.brand),
        OverlayKind::File {
            mime: FileMime::Json,
        } => (ICON_FILE_CODE, p.success),
        OverlayKind::File {
            mime: FileMime::Image,
        } => (ICON_FILE_IMAGE, p.info),
        OverlayKind::File {
            mime: FileMime::Wasm,
        } => (ICON_FILE_CODE, p.text_muted),
        OverlayKind::File {
            mime: FileMime::Other,
        } => (ICON_FILE_CODE, p.text_muted),
        OverlayKind::Dir => (ICON_FOLDER, p.warning),
    }
}

fn right_label_for_entry(entry: &OverlayEntry<'_>) -> String {
    match entry.kind {
        OverlayKind::Dir => entry
            .child_count
            .map(|n| format!("{n} files"))
            .unwrap_or_default(),
        OverlayKind::File { .. } => entry.size_bytes.map(format_size).unwrap_or_default(),
    }
}

fn entry_row_style(is_selected: bool, p: ForgePalette) -> impl Fn(&iced::Theme, Status) -> Style {
    move |_theme, status| {
        let bg = if is_selected {
            Some(iced::Background::Color(p.surface_overlay))
        } else {
            match status {
                Status::Hovered => Some(iced::Background::Color(Color { a: 0.04, ..p.brand })),
                _ => None,
            }
        };
        Style {
            background: bg,
            text_color: p.text_primary,
            border: Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        }
    }
}

pub fn overlay_file_list<'a, Msg: Clone + 'a>(
    params: OverlayFileListParams<'a>,
    on_open_folder: Msg,
    on_copy_url: impl Fn(&'a str) -> Msg + 'a,
    on_select_file: impl Fn(usize) -> Msg + 'a,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;

    let effective_selected: Option<&'a str> = params.selected_for_url.or_else(|| {
        params
            .entries
            .iter()
            .find(|e| {
                matches!(
                    e.kind,
                    OverlayKind::File {
                        mime: FileMime::Html
                    }
                )
            })
            .map(|e| e.name)
    });

    let header_row = row![
        row![
            text(ICON_FOLDER.to_string())
                .font(BOOTSTRAP_FONT)
                .size(14.0f32)
                .color(p.warning),
            text("Overlay host root")
                .size(FONT_BODY)
                .font(iced::Font {
                    weight: iced::font::Weight::Medium,
                    ..font(FontRole::Body)
                })
                .color(p.text_primary),
        ]
        .spacing(7)
        .align_y(Alignment::Center),
        Space::new().width(Length::Fill),
        text(ICON_EXTERNAL_LINK.to_string())
            .font(BOOTSTRAP_FONT)
            .size(13.0f32)
            .color(p.text_faint),
    ]
    .align_y(Alignment::Center)
    .padding([10u16, 14u16]);

    let path_label = text("PATH")
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let path_box = container(
        text(params.root_path)
            .font(font(FontRole::Monospace))
            .size(11.0f32)
            .color(p.text_primary),
    )
    .width(Length::Fill)
    .padding([6u16, 10u16])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(p.shell)),
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    });

    let folder_open_btn = button(
        text(ICON_FOLDER_OPEN.to_string())
            .font(BOOTSTRAP_FONT)
            .size(13.0f32),
    )
    .on_press(on_open_folder)
    .padding([6u16, 8u16])
    .style(outline_btn_style(
        p.border_regular,
        p.text_secondary,
        p.text_primary,
    ));

    let path_row = row![path_box, folder_open_btn]
        .spacing(6)
        .align_y(Alignment::Center);

    let path_group: Element<'a, Msg> = column![path_label, path_row].spacing(5).into();

    let entry_count = params.entries.len();
    let files_label_row = row![
        text("FILES")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(p.text_muted),
        text(format!(" {entry_count}"))
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(p.text_faint),
    ]
    .align_y(Alignment::Center);

    let last_idx = params.entries.len().saturating_sub(1);
    let mut file_row_els: Vec<Element<'a, Msg>> = Vec::with_capacity(params.entries.len());

    for (i, entry) in params.entries.iter().enumerate() {
        let (icon_char, icon_color) = entry_icon_and_color(entry.kind, p);
        let is_selected = effective_selected == Some(entry.name);
        let is_last = i == last_idx;
        let right = right_label_for_entry(entry);

        let row_content = row![
            text(icon_char.to_string())
                .font(BOOTSTRAP_FONT)
                .size(12.0f32)
                .color(icon_color),
            text(entry.name)
                .font(font(FontRole::Monospace))
                .size(FONT_SM)
                .color(p.text_primary)
                .width(Length::Fill),
            text(right)
                .font(font(FontRole::Monospace))
                .size(10.0f32)
                .color(p.text_faint),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([5u16, 0u16]);

        let row_el: Element<'a, Msg> = if matches!(entry.kind, OverlayKind::File { .. }) {
            let btn = button(row_content)
                .on_press(on_select_file(i))
                .padding(0)
                .width(Length::Fill)
                .style(entry_row_style(is_selected, p));
            if is_last {
                btn.into()
            } else {
                column![btn, section_divider::<Msg>(p.border_regular)]
                    .spacing(0)
                    .into()
            }
        } else {
            let plain = container(row_content).width(Length::Fill);
            if is_last {
                plain.into()
            } else {
                column![plain, section_divider::<Msg>(p.border_regular)]
                    .spacing(0)
                    .into()
            }
        };
        file_row_els.push(row_el);
    }

    let file_list_col = Column::with_children(file_row_els).spacing(0);

    let list_content: Element<'a, Msg> = if entry_count > 5 {
        scrollable(file_list_col)
            .height(Length::Fixed(110.0))
            .into()
    } else {
        file_list_col.into()
    };

    let files_group: Element<'a, Msg> = column![files_label_row, list_content].spacing(5).into();

    let url_display = effective_selected
        .map(|name| browser_url(params.bind_address, name))
        .unwrap_or_else(|| format!("http://{}/", params.bind_address));

    let copy_icon_el = text(ICON_COPY.to_string())
        .font(BOOTSTRAP_FONT)
        .size(12.0f32)
        .color(p.text_faint);

    let copy_btn: Element<'a, Msg> = if let Some(name) = effective_selected {
        button(copy_icon_el)
            .on_press(on_copy_url(name))
            .padding([2u16, 4u16])
            .style(ghost_icon_style(p.text_faint, p.text_secondary))
            .into()
    } else {
        container(copy_icon_el).padding([2u16, 4u16]).into()
    };

    let url_box = container(
        row![
            text(url_display)
                .font(font(FontRole::Monospace))
                .size(10.5f32)
                .color(p.info)
                .width(Length::Fill),
            copy_btn,
        ]
        .align_y(Alignment::Center)
        .spacing(6),
    )
    .width(Length::Fill)
    .padding([6u16, 10u16])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(p.shell)),
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    });

    let url_label = text("BROWSER SOURCE URL")
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let url_group: Element<'a, Msg> = column![
        section_divider::<Msg>(p.border_regular),
        column![url_label, url_box].spacing(5),
    ]
    .spacing(10)
    .into();

    let body_col: Element<'a, Msg> = column![path_group, files_group, url_group]
        .spacing(10)
        .into();

    let body = container(body_col)
        .padding([12u16, 14u16])
        .width(Length::Fill);

    let card_content =
        column![header_row, section_divider::<Msg>(p.border_regular), body].spacing(0);

    container(card_content)
        .width(Length::Fill)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: radius(Radius::Xxl).into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn mask_token_replaces_middle_with_bullets() {
        let result = mask_token("fg_verylongtoken5L9k");
        assert!(result.starts_with("fg_"));
        assert!(result.contains('•'));
        assert!(result.ends_with("5L9k"));
    }

    #[test]
    fn mask_token_bullet_count_matches_hidden_chars() {
        let result = mask_token("fg_verylongtoken5L9k");
        let bullet_count = result.chars().filter(|&c| c == '•').count();
        assert_eq!(bullet_count, 13);
    }

    #[test]
    fn mask_token_short_body_returns_unchanged() {
        let short = "fg_abc";
        assert_eq!(mask_token(short), short);
    }

    #[test]
    fn mask_token_no_prefix_still_masks() {
        let result = mask_token("abcdefghij");
        assert!(result.contains('•'));
        assert!(result.ends_with("ghij"));
        assert!(!result.starts_with("fg_"));
    }

    #[test]
    fn mask_token_differs_from_original() {
        let token = "fg_abc12345xyz5L9k";
        assert_ne!(mask_token(token), token);
    }

    fn make_row<'a>(
        status: ClientStatus,
        subs: &'a [SubscriptionChipData<'a>],
        highlight: bool,
    ) -> ClientRowData<'a> {
        ClientRowData {
            status,
            identification: "test.html",
            client_type_label: "OBS browser · 127.0.0.1",
            subscriptions: subs,
            events_per_second: 0.4,
            uptime_short: "2h 14m",
            highlight,
        }
    }

    #[test]
    fn smoke_client_row_active() {
        let subs = [SubscriptionChipData {
            label: "twitch.sub",
            source: EventSource::Twitch,
        }];
        let data = make_row(ClientStatus::Active, &subs, false);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_client_row_idle() {
        let subs = [SubscriptionChipData {
            label: "twitch.cheer",
            source: EventSource::Twitch,
        }];
        let data = make_row(ClientStatus::Idle, &subs, false);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_client_row_disconnecting() {
        let subs: &[SubscriptionChipData<'_>] = &[];
        let data = make_row(ClientStatus::Disconnecting, subs, true);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_client_row_four_subscriptions() {
        let subs = [
            SubscriptionChipData {
                label: "twitch.sub",
                source: EventSource::Twitch,
            },
            SubscriptionChipData {
                label: "twitch.cheer",
                source: EventSource::Twitch,
            },
            SubscriptionChipData {
                label: "twitch.raid",
                source: EventSource::Twitch,
            },
            SubscriptionChipData {
                label: "yt.super_chat",
                source: EventSource::YouTube,
            },
        ];
        let data = make_row(ClientStatus::Active, &subs, false);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_client_row_wildcard_subscription() {
        let subs = [SubscriptionChipData {
            label: "*",
            source: EventSource::Core,
        }];
        let data = make_row(ClientStatus::Active, &subs, true);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn bearer_token_display_masked_smoke() {
        let _: Element<'_, ()> =
            bearer_token_display("fg_abc12345xyz5L9k", false, (), (), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn bearer_token_display_revealed_smoke() {
        let _: Element<'_, ()> =
            bearer_token_display("fg_abc12345xyz5L9k", true, (), (), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn masked_display_contains_bullets() {
        let token = "fg_abc12345xyz5L9k";
        let masked = mask_token(token);
        assert!(masked.contains('•'));
    }

    #[test]
    fn revealed_display_equals_original() {
        let token = "fg_abc12345xyz5L9k";
        assert_eq!(token, token);
        assert_ne!(mask_token(token), token);
    }

    fn make_bind_params(badge: BindBadge, selected: bool) -> BindAddressCardParams<'static> {
        match badge {
            BindBadge::Recommended => BindAddressCardParams {
                title: "Localhost only",
                tech_label: "127.0.0.1",
                badge,
                description: "Only apps on this machine can connect.",
                selected,
            },
            BindBadge::RequiresConfirmation => BindAddressCardParams {
                title: "All interfaces (LAN)",
                tech_label: "0.0.0.0",
                badge,
                description: "Lets other devices on your network connect.",
                selected,
            },
        }
    }

    #[test]
    fn smoke_bind_address_card_recommended_selected() {
        let params = make_bind_params(BindBadge::Recommended, true);
        let _: Element<'_, ()> = bind_address_card(params, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_bind_address_card_requires_confirmation_unselected() {
        let params = make_bind_params(BindBadge::RequiresConfirmation, false);
        let _: Element<'_, ()> = bind_address_card(params, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn badge_color_recommended_resolves_to_success() {
        let color = badge_color(BindBadge::Recommended, &CATPPUCCIN_MOCHA);
        assert_eq!(color, CATPPUCCIN_MOCHA.success);
    }

    #[test]
    fn badge_color_requires_confirmation_resolves_to_warning() {
        let color = badge_color(BindBadge::RequiresConfirmation, &CATPPUCCIN_MOCHA);
        assert_eq!(color, CATPPUCCIN_MOCHA.warning);
    }

    fn lan_bullets() -> Vec<BulletItem<'static>> {
        vec![
            BulletItem {
                kind: BulletKind::Check,
                text: "Phone / tablet / second PC can connect to overlays and the WS API",
            },
            BulletItem {
                kind: BulletKind::Warning,
                text: "Anyone on your network can read all events if they know your bearer token",
            },
            BulletItem {
                kind: BulletKind::Info,
                text: "Your firewall must also allow port 8081 for this to work",
            },
        ]
    }

    #[test]
    fn type_to_confirm_modal_empty_input_confirm_disabled() {
        let bullets = lan_bullets();
        let params = TypeToConfirmModalParams {
            title: "Expose Forge to your network?",
            explanation: "Switching from 127.0.0.1 to 0.0.0.0.",
            bullets: &bullets,
            confirmation_phrase: "expose to LAN",
            current_input: "",
            confirm_label: "Expose to LAN",
        };
        let _: Element<'_, ()> =
            type_to_confirm_modal(params, |_s: String| (), (), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn type_to_confirm_modal_matching_input_confirm_enabled() {
        let bullets = lan_bullets();
        let params = TypeToConfirmModalParams {
            title: "Expose Forge to your network?",
            explanation: "Switching from 127.0.0.1 to 0.0.0.0.",
            bullets: &bullets,
            confirmation_phrase: "expose to LAN",
            current_input: "expose to LAN",
            confirm_label: "Expose to LAN",
        };
        let _: Element<'_, ()> =
            type_to_confirm_modal(params, |_s: String| (), (), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn type_to_confirm_modal_all_bullet_kinds_render() {
        let bullets = [
            BulletItem {
                kind: BulletKind::Check,
                text: "This is fine",
            },
            BulletItem {
                kind: BulletKind::Warning,
                text: "Be careful here",
            },
            BulletItem {
                kind: BulletKind::Info,
                text: "Informational note",
            },
        ];
        let params = TypeToConfirmModalParams {
            title: "Confirm action",
            explanation: "Please read before confirming.",
            bullets: &bullets,
            confirmation_phrase: "confirm",
            current_input: "conf",
            confirm_label: "Confirm",
        };
        let _: Element<'_, ()> =
            type_to_confirm_modal(params, |_s: String| (), (), (), &CATPPUCCIN_MOCHA);
    }

    fn make_entries() -> [OverlayEntry<'static>; 4] {
        [
            OverlayEntry {
                name: "alerts.html",
                kind: OverlayKind::File {
                    mime: FileMime::Html,
                },
                size_bytes: Some(4_301),
                child_count: None,
            },
            OverlayEntry {
                name: "chat.html",
                kind: OverlayKind::File {
                    mime: FileMime::Html,
                },
                size_bytes: Some(2_867),
                child_count: None,
            },
            OverlayEntry {
                name: "logo.png",
                kind: OverlayKind::File {
                    mime: FileMime::Image,
                },
                size_bytes: Some(18_432),
                child_count: None,
            },
            OverlayEntry {
                name: "assets",
                kind: OverlayKind::Dir,
                size_bytes: None,
                child_count: Some(12),
            },
        ]
    }

    #[test]
    fn smoke_overlay_file_list_three_files_one_dir() {
        let entries = make_entries();
        let params = OverlayFileListParams {
            root_path: "~/.local/share/forge/overlays",
            entries: &entries,
            bind_address: "127.0.0.1:8081",
            selected_for_url: Some("alerts.html"),
        };
        let _: Element<'_, usize> =
            overlay_file_list(params, 0usize, |_name| 1usize, |idx| idx, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_overlay_file_list_no_selection_falls_back_to_first_html() {
        let entries = make_entries();
        let params = OverlayFileListParams {
            root_path: "~/.local/share/forge/overlays",
            entries: &entries,
            bind_address: "127.0.0.1:8081",
            selected_for_url: None,
        };
        let _: Element<'_, usize> =
            overlay_file_list(params, 0usize, |_name| 1usize, |idx| idx, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn format_size_zero_is_zero_bytes() {
        assert_eq!(format_size(0), "0 B");
    }

    #[test]
    fn format_size_4288_is_4_point_2_kb() {
        assert_eq!(format_size(4_288), "4.2 KB");
    }

    #[test]
    fn format_size_1_500_000_is_1_point_4_mb() {
        assert_eq!(format_size(1_500_000), "1.4 MB");
    }

    #[test]
    fn browser_url_formats_correctly() {
        assert_eq!(
            browser_url("127.0.0.1:8081", "alerts.html"),
            "http://127.0.0.1:8081/alerts.html"
        );
    }
}
