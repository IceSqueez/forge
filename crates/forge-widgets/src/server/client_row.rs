use forge_events::EventSource;
use iced::{
    Alignment, Border, Color, Element, Length,
    font::Style as FontStyle,
    widget::{Row, Space, button, column, container, row, text},
};

use crate::{
    events::color_for_source,
    icons::{Icon, tabler_icon},
    palette::ForgePalette,
    tokens::{FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp},
};

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
            radius: radius(Radius::Md).into(),
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
    .padding([0, sp(Spacing::Xxs)])
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
    .padding([0, sp(Spacing::Xxs)])
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

    let x_btn = button(tabler_icon(Icon::X, 13.0, palette.text_faint))
        .on_press(on_disconnect)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
        .style(super::ghost_icon_style(
            palette.text_faint,
            palette.text_secondary,
        ));

    let x_cell = container(x_btn).width(Length::Fixed(22.0));

    let content = row![dot_cell, id_cell, subs_cell, evs_cell, uptime_cell, x_cell]
        .align_y(Alignment::Center)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)]);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use forge_events::EventSource;

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
}
