use std::time::Duration;

use forge_platform_core::{
    ActiveRow, BannerLevel, CapabilityFlags, ConnectionState, ContentList, ContentListItem,
    DetailSection, HeaderAction, HealthBar, HealthLevel, InfoField, KeyValueRow, ListFooter,
    QuickAction, RowAction, SectionIcon, StatColumn, SubscriptionRow, SubscriptionStatus,
    TokenColor, TrailingToken,
};
use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Column, Row, Space, container, text},
};

use crate::{
    BOOTSTRAP_FONT,
    palette::ForgePalette,
    tokens::{
        BORDER_THIN, Density, FONT_BODY, FONT_BODY_LG, FONT_BODY_MD, FONT_BODY_SM, FONT_CAPS,
        FONT_CAPS_SM, FONT_CAPS_XS, FONT_HEADING, FONT_HEADING_SM, FONT_VALUE, FontRole, Radius,
        Spacing, font, radius, spacing,
    },
};

pub struct HeaderCardParams<'a> {
    pub display_name: &'a str,
    pub version: Option<&'a str>,
    pub endpoint: Option<&'a str>,
    pub uptime: Option<Duration>,
    pub capability_flags: &'a CapabilityFlags,
    pub header_actions: &'a [HeaderAction],
    pub connection: ConnectionState,
    pub icon: SectionIcon,
}

pub fn integration_header_card<'a, Msg: Clone + 'a>(
    params: HeaderCardParams<'a>,
    on_action: impl Fn(HeaderAction) -> Msg + 'a,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let icon_str = params.icon.as_str().to_owned();
    let icon_elem = icon_box(icon_str, params.connection, palette);
    let info_elem = info_column(&params, palette);
    let actions_elem = action_buttons(params.header_actions, on_action, palette);

    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxxl);
    let v_pad = spacing(Spacing::Xl, Density::Cozy);
    let h_pad = spacing(Spacing::Xxxl, Density::Cozy);

    let inner = iced::widget::row![
        icon_elem,
        container(info_elem).width(Length::Fill),
        actions_elem,
    ]
    .spacing(spacing(Spacing::Xxl, Density::Cozy) as f32)
    .align_y(Alignment::Center);

    container(inner)
        .padding([v_pad, h_pad])
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn icon_box<'a, Msg: 'a>(
    icon_str: String,
    connection: ConnectionState,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let icon_color = match connection {
        ConnectionState::Connected => palette.success,
        ConnectionState::Connecting | ConnectionState::Reconnecting => palette.warning,
        ConnectionState::Disconnected => palette.disabled,
    };
    let box_bg = palette.surface_overlay;
    let r = radius(Radius::Xxxl);

    let icon_text = iced::widget::text(icon_str)
        .font(BOOTSTRAP_FONT)
        .size(24.0)
        .color(icon_color);

    container(icon_text)
        .width(48.0)
        .height(48.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(box_bg)),
            border: Border {
                radius: r.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn info_column<'a, Msg: 'a>(
    params: &HeaderCardParams<'a>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;
    let surface_overlay = palette.surface_overlay;
    let warning = palette.warning;

    let mut name_row: Row<'a, Msg> = Row::new()
        .spacing(spacing(Spacing::Md, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(
            iced::widget::text(params.display_name)
                .size(FONT_HEADING_SM)
                .color(text_primary),
        );

    if let Some(version) = params.version {
        name_row = name_row.push(version_pill(version, surface_overlay, text_muted));
    }

    if params.capability_flags.limited {
        let label = params
            .capability_flags
            .label
            .as_deref()
            .unwrap_or("Limited");
        name_row = name_row.push(limited_badge(label, surface_overlay, warning));
    }

    let sub = sub_line(params.endpoint, params.uptime);
    let sub_text = iced::widget::text(sub)
        .font(font(FontRole::Monospace))
        .size(FONT_BODY)
        .color(text_muted);

    iced::widget::column![name_row, sub_text]
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
        .into()
}

fn version_pill<'a, Msg: 'a>(version: &'a str, bg: Color, text_color: Color) -> Element<'a, Msg> {
    let r = radius(Radius::Xxl);
    container(
        iced::widget::text(version)
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS_XS)
            .color(text_color),
    )
    .padding([1, 7])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            radius: r.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn limited_badge<'a, Msg: 'a>(label: &'a str, bg: Color, text_color: Color) -> Element<'a, Msg> {
    let r = radius(Radius::Xxl);
    container(
        iced::widget::text(label.to_uppercase())
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS_XS)
            .color(text_color),
    )
    .padding([1, 7])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            radius: r.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn action_buttons<'a, Msg: Clone + 'a>(
    actions: &'a [HeaderAction],
    on_action: impl Fn(HeaderAction) -> Msg + 'a,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let border_color = palette.border_regular;
    let text_secondary = palette.text_secondary;
    let text_disconnect = palette.random;
    let r = radius(Radius::Sm);
    let v_pad = spacing(Spacing::Sm, Density::Cozy);
    let h_pad = spacing(Spacing::Xl, Density::Cozy);

    let mut row: Row<'a, Msg> = Row::new().spacing(spacing(Spacing::Sm, Density::Cozy) as f32);

    for action in actions {
        let label = action_label(action);
        let text_color = match action {
            HeaderAction::Disconnect => text_disconnect,
            _ => text_secondary,
        };
        let msg = on_action(action.clone());

        let btn = iced::widget::button(
            iced::widget::text(label)
                .size(FONT_BODY_SM)
                .color(text_color),
        )
        .on_press(msg)
        .padding([v_pad, h_pad])
        .style(move |_theme: &iced::Theme, status| {
            use iced::widget::button::Status;
            let bg_color = match status {
                Status::Hovered => Some(iced::Background::Color(Color {
                    a: 0.06,
                    ..border_color
                })),
                Status::Active | Status::Pressed | Status::Disabled => None,
            };
            iced::widget::button::Style {
                background: bg_color,
                text_color,
                border: Border {
                    color: border_color,
                    width: BORDER_THIN,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

        row = row.push(btn);
    }

    row.into()
}

fn action_label(action: &HeaderAction) -> &'static str {
    match action {
        HeaderAction::Reconnect => "Reconnect",
        HeaderAction::RefreshToken => "Refresh Token",
        HeaderAction::Disconnect => "Disconnect",
        HeaderAction::Settings => "Settings",
    }
}

fn sub_line(endpoint: Option<&str>, uptime: Option<Duration>) -> String {
    match (endpoint, uptime) {
        (Some(ep), Some(d)) => format!("{} · uptime {}", ep, format_uptime(d)),
        (Some(ep), None) => ep.to_owned(),
        (None, Some(d)) => format!("uptime {}", format_uptime(d)),
        (None, None) => String::new(),
    }
}

fn format_uptime(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

pub fn integration_health_grid<'a, Msg: 'a>(
    metrics: &[forge_platform_core::HealthMetric; 4],
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let gap = spacing(Spacing::Md, Density::Cozy) as f32;

    iced::widget::Row::new()
        .spacing(gap)
        .push(health_metric_card(&metrics[0], palette))
        .push(health_metric_card(&metrics[1], palette))
        .push(health_metric_card(&metrics[2], palette))
        .push(health_metric_card(&metrics[3], palette))
        .into()
}

fn health_metric_card<'a, Msg: 'a>(
    metric: &forge_platform_core::HealthMetric,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    use forge_platform_core::HealthValue;

    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xl);
    let v_pad = spacing(Spacing::Lg, Density::Cozy);
    let h_pad = spacing(Spacing::Xl, Density::Cozy);

    let cap_label = iced::widget::text(metric.label.to_uppercase())
        .font(font(FontRole::Monospace))
        .size(FONT_CAPS_SM)
        .color(palette.text_muted);

    let value_col: Element<'a, Msg> = match &metric.value {
        HealthValue::Status {
            label: val_label,
            active,
        } => {
            let color = if *active {
                palette.success
            } else {
                palette.disabled
            };
            let dot = crate::status::status_dot(color, 7.0);
            let val_text = iced::widget::text(val_label.clone())
                .size(FONT_VALUE)
                .color(color);
            iced::widget::row![dot, val_text]
                .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
                .align_y(Alignment::Center)
                .into()
        }
        HealthValue::Text { primary, secondary } => {
            let primary_text = iced::widget::text(primary.clone())
                .size(FONT_VALUE)
                .color(palette.text_primary);
            if let Some(sec) = secondary {
                let sub = iced::widget::text(sec.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_CAPS_SM)
                    .color(palette.text_faint);
                iced::widget::column![primary_text, sub]
                    .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
                    .into()
            } else {
                iced::widget::column![primary_text].into()
            }
        }
        HealthValue::Pair { left, right } => iced::widget::text(format!("{left} · {right}"))
            .font(font(FontRole::Monospace))
            .size(FONT_VALUE)
            .color(palette.text_primary)
            .into(),
        HealthValue::Ratio {
            used,
            total,
            reset_hint,
        } => {
            let ratio_text = iced::widget::text(format!("{used} / {total}"))
                .size(FONT_VALUE)
                .color(palette.text_primary);
            if let Some(hint) = reset_hint {
                let hint_text = iced::widget::text(hint.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_CAPS_SM)
                    .color(palette.text_faint);
                iced::widget::column![ratio_text, hint_text]
                    .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
                    .into()
            } else {
                iced::widget::column![ratio_text].into()
            }
        }
    };

    let inner = iced::widget::column![cap_label, value_col]
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32);

    container(inner)
        .padding([v_pad, h_pad])
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn integration_content_renderer<'a, Msg: 'a>(
    sections: &'a [DetailSection],
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let gap = spacing(Spacing::Huge, Density::Cozy) as f32;
    sections
        .iter()
        .fold(
            Column::new().spacing(gap).width(Length::Fill),
            |col, section| col.push(dispatch_section(section, palette)),
        )
        .into()
}

fn dispatch_section<'a, Msg: 'a>(
    section: &'a DetailSection,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    match section {
        DetailSection::TwoColumnLists { left, right } => {
            render_two_column_lists(left, right, palette)
        }
        DetailSection::KeyValueList { title, icon, items } => {
            render_key_value_list(title, icon, items, palette)
        }
        DetailSection::ActiveItemList { title, icon, items } => {
            render_active_item_list(title, icon, items, palette)
        }
        DetailSection::WarningBanner {
            level,
            title,
            body,
            cta,
        } => render_warning_banner(level, title, body, cta.as_deref(), palette),
        DetailSection::SubscriptionList {
            title,
            icon,
            items,
            footer,
        } => render_subscription_list(title, icon, items, footer.as_ref(), palette),
        DetailSection::ScopesList {
            title,
            scopes,
            footer,
        } => render_scopes_list(title, scopes, footer.as_ref(), palette),
        DetailSection::InfoCard {
            title,
            live,
            fields,
            health_bar,
        } => render_info_card(title, *live, fields, health_bar.as_ref(), palette),
        DetailSection::StatsGrid {
            title,
            icon,
            columns,
        } => render_stats_grid(title, icon, columns, palette),
    }
}

pub(crate) fn render_two_column_lists<'a, Msg: 'a>(
    left: &'a ContentList,
    right: &'a ContentList,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let gap = spacing(Spacing::Xl, Density::Cozy) as f32;
    Row::new()
        .spacing(gap)
        .push(content_list_panel(left, palette))
        .push(content_list_panel(right, palette))
        .into()
}

pub(crate) fn render_key_value_list<'a, Msg: 'a>(
    title: &str,
    icon: &SectionIcon,
    items: &'a [KeyValueRow],
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxl);

    let header = panel_header_row(icon.as_str(), title, None, palette);
    let divider = horizontal_divider(border_color);
    let rows: Element<'a, Msg> = items
        .iter()
        .fold(Column::new(), |col, item| {
            col.push(key_value_row_elem(item, palette))
        })
        .into();

    card_container(
        Column::new().push(header).push(divider).push(rows),
        bg,
        border_color,
        r,
    )
}

pub(crate) fn render_active_item_list<'a, Msg: 'a>(
    title: &str,
    icon: &SectionIcon,
    items: &'a [ActiveRow],
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxl);
    let count = if items.is_empty() {
        None
    } else {
        Some(items.len().to_string())
    };

    let header = panel_header_row(icon.as_str(), title, count.as_deref(), palette);
    let divider = horizontal_divider(border_color);
    let rows: Element<'a, Msg> = items
        .iter()
        .fold(Column::new(), |col, item| {
            col.push(active_item_row_elem(item, palette))
        })
        .into();

    card_container(
        Column::new().push(header).push(divider).push(rows),
        bg,
        border_color,
        r,
    )
}

pub(crate) fn render_warning_banner<'a, Msg: 'a>(
    level: &BannerLevel,
    title: &str,
    body: &str,
    cta: Option<&str>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let level_color = banner_level_color(level, palette);
    let bg = palette.elevated;
    let r = radius(Radius::Xxl);

    let icon_char = match level {
        BannerLevel::Warning => '\u{26A0}',
        BannerLevel::Info => '\u{2139}',
        BannerLevel::Error => '\u{2715}',
    };

    let icon_elem = text(icon_char.to_string())
        .size(FONT_BODY_LG)
        .color(level_color);
    let title_elem = text(title.to_owned())
        .size(FONT_BODY_MD)
        .color(palette.text_primary);
    let body_elem = text(body.to_owned())
        .size(FONT_BODY_SM)
        .color(palette.text_muted);

    let mut text_col = Column::new()
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
        .push(title_elem)
        .push(body_elem);

    if let Some(cta_label) = cta {
        let brand = palette.brand;
        text_col = text_col.push(
            text(format!("{cta_label} \u{2192}"))
                .size(FONT_BODY_SM)
                .color(brand),
        );
    }

    let inner = Row::new()
        .spacing(spacing(Spacing::Xl, Density::Cozy) as f32)
        .align_y(Alignment::Start)
        .push(icon_elem)
        .push(container(text_col).width(Length::Fill));

    container(inner)
        .padding([
            spacing(Spacing::Lg, Density::Cozy),
            spacing(Spacing::Xxl, Density::Cozy),
        ])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: level_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub(crate) fn render_subscription_list<'a, Msg: 'a>(
    title: &str,
    icon: &SectionIcon,
    items: &'a [SubscriptionRow],
    footer: Option<&'a ListFooter>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxl);
    let count_str = format!("{} active", items.len());

    let header = panel_header_row(icon.as_str(), title, Some(&count_str), palette);
    let divider = horizontal_divider(border_color);
    let rows: Element<'a, Msg> = items
        .iter()
        .fold(Column::new(), |col, item| {
            col.push(subscription_row_elem(item, palette))
        })
        .into();

    let mut col = Column::new().push(header).push(divider).push(rows);
    if let Some(f) = footer {
        col = col.push(list_footer_bar(f, palette));
    }

    card_container(col, bg, border_color, r)
}

pub(crate) fn render_scopes_list<'a, Msg: 'a>(
    title: &str,
    scopes: &'a [String],
    footer: Option<&'a ListFooter>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxl);
    let count_str = scopes.len().to_string();

    let header = scopes_list_header(title, &count_str, palette);
    let divider = horizontal_divider(border_color);
    let rows: Element<'a, Msg> = scopes
        .iter()
        .fold(Column::new(), |col, scope| {
            col.push(scope_row_elem(scope, palette))
        })
        .into();

    let mut col = Column::new().push(header).push(divider).push(rows);
    if let Some(f) = footer {
        col = col.push(list_footer_bar(f, palette));
    }

    card_container(col, bg, border_color, r)
}

pub(crate) fn render_info_card<'a, Msg: 'a>(
    title: &str,
    live: bool,
    fields: &'a [InfoField],
    health_bar: Option<&'a HealthBar>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxl);

    let header = info_card_header(title, live, palette);
    let divider = horizontal_divider(border_color);

    let fields_grid: Element<'a, Msg> = fields
        .chunks(2)
        .fold(
            Column::new().spacing(spacing(Spacing::Xl, Density::Cozy) as f32),
            |col, chunk| {
                let mut row = Row::new().spacing(spacing(Spacing::Xl, Density::Cozy) as f32);
                for field in chunk {
                    row = row.push(info_field_cell(field, palette));
                }
                col.push(row)
            },
        )
        .into();

    let mut content_col = Column::new()
        .spacing(spacing(Spacing::Xl, Density::Cozy) as f32)
        .push(fields_grid);

    if let Some(bar) = health_bar {
        content_col = content_col.push(health_bar_section(bar, palette));
    }

    let content_padded = container(content_col)
        .padding([
            spacing(Spacing::Xl, Density::Cozy),
            spacing(Spacing::Xxl, Density::Cozy),
        ])
        .width(Length::Fill);

    card_container(
        Column::new()
            .push(header)
            .push(divider)
            .push(content_padded),
        bg,
        border_color,
        r,
    )
}

pub(crate) fn render_stats_grid<'a, Msg: 'a>(
    title: &str,
    icon: &SectionIcon,
    columns: &'a [StatColumn],
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxl);
    let sep_color = border_color;

    let header = panel_header_row(icon.as_str(), title, None, palette);
    let divider = horizontal_divider(border_color);

    let stats_row: Element<'a, Msg> = columns
        .iter()
        .enumerate()
        .fold(Row::new(), |row, (i, col)| {
            let row = if i > 0 {
                row.push(
                    container(Space::new())
                        .width(1.0)
                        .height(Length::Fill)
                        .style(move |_: &iced::Theme| container::Style {
                            background: Some(iced::Background::Color(sep_color)),
                            ..container::Style::default()
                        }),
                )
            } else {
                row
            };
            row.push(stat_column_cell(col, palette))
        })
        .into();

    card_container(
        Column::new().push(header).push(divider).push(stats_row),
        bg,
        border_color,
        r,
    )
}

fn content_list_panel<'a, Msg: 'a>(
    list: &'a ContentList,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxl);

    let header = panel_header_row(
        list.icon.as_str(),
        &list.title,
        list.count_label.as_deref(),
        palette,
    );
    let divider = horizontal_divider(border_color);
    let rows: Element<'a, Msg> = list
        .items
        .iter()
        .fold(Column::new(), |col, item| {
            col.push(content_list_item_row(item, palette))
        })
        .into();

    let mut col = Column::new().push(header).push(divider).push(rows);
    if let Some(f) = &list.footer {
        col = col.push(list_footer_bar(f, palette));
    }

    card_container(col, bg, border_color, r)
}

fn content_list_item_row<'a, Msg: 'a>(
    item: &'a ContentListItem,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let text_color = if !item.enabled {
        palette.disabled
    } else if item.active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    let icon_color = if !item.enabled {
        palette.disabled
    } else if item.active {
        palette.success
    } else {
        palette.text_faint
    };

    let icon_elem = text(item.icon.as_str().to_owned())
        .font(BOOTSTRAP_FONT)
        .size(FONT_BODY_LG)
        .color(icon_color);

    let name_elem: Element<'a, Msg> = if item.monospace_name {
        text(item.name.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_BODY_SM)
            .color(text_color)
            .into()
    } else {
        text(item.name.clone())
            .size(FONT_BODY)
            .color(text_color)
            .into()
    };

    let mut trailing: Row<'a, Msg> = Row::new()
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
        .align_y(Alignment::Center);

    if item.active
        && let Some(label) = &item.active_label
    {
        trailing = trailing.push(active_badge(label, palette));
    }

    for token in &item.trailing {
        trailing = trailing.push(trailing_token_elem(token, icon_color, palette));
    }

    let content: Element<'a, Msg> = Row::new()
        .spacing(spacing(Spacing::Lg, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(icon_elem)
        .push(container(name_elem).width(Length::Fill))
        .push(trailing)
        .into();

    if item.active {
        active_row_wrapper(content, palette)
    } else {
        plain_row_wrapper(content, palette.elevated)
    }
}

fn key_value_row_elem<'a, Msg: 'a>(
    item: &'a KeyValueRow,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let icon_elem = text(item.icon.as_str().to_owned())
        .font(BOOTSTRAP_FONT)
        .size(FONT_BODY_LG)
        .color(palette.text_secondary);

    let name_elem = text(item.name.clone())
        .font(font(FontRole::Monospace))
        .size(FONT_BODY_SM)
        .color(palette.text_primary);

    let mut row = Row::new()
        .spacing(spacing(Spacing::Lg, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(icon_elem)
        .push(container(name_elem).width(Length::Fill));

    if let Some(tag) = &item.tag {
        row = row.push(
            text(tag.clone())
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS)
                .color(palette.text_faint),
        );
    }

    if let Some(action) = &item.action {
        let ch = match action {
            RowAction::Play => '\u{25B6}',
        };
        row = row.push(
            text(ch.to_string())
                .size(FONT_BODY_SM)
                .color(palette.success),
        );
    }

    plain_row_wrapper(row.into(), palette.elevated)
}

fn active_item_row_elem<'a, Msg: 'a>(
    item: &'a ActiveRow,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let text_color = if item.active {
        palette.text_primary
    } else {
        palette.text_secondary
    };

    let name_elem = text(item.name.clone())
        .font(font(FontRole::Monospace))
        .size(FONT_BODY_SM)
        .color(text_color);

    let mut row = Row::new()
        .spacing(spacing(Spacing::Lg, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(container(name_elem).width(Length::Fill));

    if item.active {
        row = row.push(active_badge("ACTIVE", palette));
    } else if let Some(mode) = &item.mode_label {
        row = row.push(text(mode.clone()).size(FONT_CAPS).color(palette.text_faint));
    }

    if item.active {
        active_row_wrapper(row.into(), palette)
    } else {
        plain_row_wrapper(row.into(), palette.elevated)
    }
}

fn subscription_row_elem<'a, Msg: 'a>(
    item: &'a SubscriptionRow,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let dot_color = subscription_status_color(&item.status, palette);
    let dot = crate::status::status_dot(dot_color, 6.0);

    let name_elem = text(item.name.clone())
        .font(font(FontRole::Monospace))
        .size(FONT_CAPS)
        .color(palette.text_primary);

    let mut row = Row::new()
        .spacing(spacing(Spacing::Lg, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(dot)
        .push(container(name_elem).width(Length::Fill));

    if let Some(ver) = &item.version {
        row = row.push(
            text(ver.clone())
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS)
                .color(palette.text_faint),
        );
    }

    let trailing: Element<'a, Msg> = if let Some(err) = &item.error_label {
        text(err.clone())
            .size(FONT_CAPS)
            .color(palette.random)
            .into()
    } else if let Some(count) = item.event_count {
        let label = if count == 1 {
            format!("{count} event")
        } else {
            format!("{count} events")
        };
        text(label).size(FONT_CAPS).color(palette.text_muted).into()
    } else {
        Space::new().into()
    };

    row = row.push(trailing);
    plain_row_wrapper(row.into(), palette.elevated)
}

fn scope_row_elem<'a, Msg: 'a>(scope: &str, palette: &'a ForgePalette) -> Element<'a, Msg> {
    let check = text("\u{2713}").size(FONT_BODY_SM).color(palette.success);
    let scope_text = text(scope.to_owned())
        .font(font(FontRole::Monospace))
        .size(FONT_CAPS)
        .color(palette.text_primary);

    plain_row_wrapper(
        Row::new()
            .spacing(spacing(Spacing::Lg, Density::Cozy) as f32)
            .align_y(Alignment::Center)
            .push(check)
            .push(container(scope_text).width(Length::Fill))
            .into(),
        palette.elevated,
    )
}

fn info_field_cell<'a, Msg: 'a>(
    field: &'a InfoField,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let label_elem = text(field.label.to_uppercase())
        .font(font(FontRole::Monospace))
        .size(FONT_CAPS_SM)
        .color(palette.text_muted);

    let value_elem: Element<'a, Msg> = if field.monospace_value {
        text(field.value.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_BODY)
            .color(palette.text_primary)
            .into()
    } else {
        text(field.value.clone())
            .size(FONT_BODY)
            .color(palette.text_primary)
            .into()
    };

    container(
        Column::new()
            .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
            .push(label_elem)
            .push(value_elem),
    )
    .width(Length::Fill)
    .into()
}

fn health_bar_section<'a, Msg: 'a>(
    bar: &'a HealthBar,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let level_color = health_level_color(&bar.level, palette);
    let shell_bg = palette.shell;

    let filled = (bar.fraction.clamp(0.0, 1.0) * 1000.0) as u16;
    let remaining = 1000u16.saturating_sub(filled);

    let mut bar_inner = Row::new();
    if filled > 0 {
        bar_inner = bar_inner.push(
            container(Space::new())
                .width(Length::FillPortion(filled))
                .height(6.0)
                .style(move |_: &iced::Theme| container::Style {
                    background: Some(iced::Background::Color(level_color)),
                    ..container::Style::default()
                }),
        );
    }
    if remaining > 0 {
        bar_inner = bar_inner.push(
            container(Space::new())
                .width(Length::FillPortion(remaining))
                .height(6.0),
        );
    }

    let bar_track =
        container(bar_inner)
            .width(Length::Fill)
            .height(6.0)
            .style(move |_: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(shell_bg)),
                border: Border {
                    radius: 5.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });

    let health_label = text("STREAM HEALTH")
        .font(font(FontRole::Monospace))
        .size(FONT_CAPS_SM)
        .color(palette.text_muted);

    let bar_row = Row::new()
        .spacing(spacing(Spacing::Lg, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(container(bar_track).width(Length::Fill))
        .push(
            text(bar.label.clone())
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS)
                .color(level_color),
        );

    Column::new()
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
        .push(health_label)
        .push(bar_row)
        .into()
}

fn stat_column_cell<'a, Msg: 'a>(
    col: &'a StatColumn,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    container(
        Column::new()
            .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
            .push(
                text(col.label.to_uppercase())
                    .font(font(FontRole::Monospace))
                    .size(FONT_CAPS_SM)
                    .color(palette.text_muted),
            )
            .push(
                text(col.value.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_HEADING)
                    .color(palette.text_primary),
            )
            .push(
                text(col.subtitle.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_CAPS_SM)
                    .color(palette.success),
            ),
    )
    .padding([
        spacing(Spacing::Lg, Density::Cozy),
        spacing(Spacing::Xxl, Density::Cozy),
    ])
    .width(Length::Fill)
    .into()
}

fn panel_header_row<'a, Msg: 'a>(
    icon_str: &str,
    title: &str,
    count: Option<&str>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let icon_elem = text(icon_str.to_owned())
        .font(BOOTSTRAP_FONT)
        .size(FONT_BODY_LG)
        .color(palette.text_secondary);

    let left = Row::new()
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(icon_elem)
        .push(
            text(title.to_owned())
                .size(FONT_BODY_MD)
                .color(palette.text_primary),
        );

    let mut outer = Row::new()
        .align_y(Alignment::Center)
        .push(container(left).width(Length::Fill));

    if let Some(c) = count {
        outer = outer.push(
            text(c.to_owned())
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS_SM)
                .color(palette.text_faint),
        );
    }

    container(outer)
        .padding([
            spacing(Spacing::Lg, Density::Cozy),
            spacing(Spacing::Xxl, Density::Cozy),
        ])
        .width(Length::Fill)
        .into()
}

fn scopes_list_header<'a, Msg: 'a>(
    title: &str,
    count: &str,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let count_elem = text(count.to_owned())
        .font(font(FontRole::Monospace))
        .size(FONT_CAPS_SM)
        .color(palette.text_faint);

    container(
        Row::new()
            .align_y(Alignment::Center)
            .push(
                container(
                    text(title.to_owned())
                        .size(FONT_BODY_MD)
                        .color(palette.text_primary),
                )
                .width(Length::Fill),
            )
            .push(count_elem),
    )
    .padding([
        spacing(Spacing::Lg, Density::Cozy),
        spacing(Spacing::Xxl, Density::Cozy),
    ])
    .width(Length::Fill)
    .into()
}

fn info_card_header<'a, Msg: 'a>(
    title: &str,
    live: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let mut row = Row::new().align_y(Alignment::Center).push(
        container(
            text(title.to_owned())
                .size(FONT_BODY_MD)
                .color(palette.text_primary),
        )
        .width(Length::Fill),
    );

    if live {
        let success = palette.success;
        let surface = palette.surface_overlay;
        let dot = crate::status::status_dot(success, 5.0);
        let badge = container(
            Row::new()
                .spacing(5.0)
                .align_y(Alignment::Center)
                .push(dot)
                .push(text("LIVE").size(FONT_CAPS_XS).color(success)),
        )
        .padding([1, 7])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(surface)),
            border: Border {
                radius: 8.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });
        row = row.push(badge);
    }

    container(row)
        .padding([
            spacing(Spacing::Lg, Density::Cozy),
            spacing(Spacing::Xxl, Density::Cozy),
        ])
        .width(Length::Fill)
        .into()
}

fn list_footer_bar<'a, Msg: 'a>(
    footer: &ListFooter,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let shell_bg = palette.shell;
    let border_col = palette.border_regular;

    let mut row = Row::new()
        .spacing(spacing(Spacing::Lg, Density::Cozy) as f32)
        .align_y(Alignment::Center);

    if let Some(cta) = &footer.cta_label {
        let brand = palette.brand;
        row =
            row.push(container(text(cta.clone()).size(FONT_CAPS).color(brand)).width(Length::Fill));
    } else {
        row = row.push(container(Space::new()).width(Length::Fill));
    }

    if let Some(trail) = &footer.trailing_label {
        row = row.push(
            text(trail.clone())
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS)
                .color(palette.text_faint),
        );
    }

    container(row)
        .padding([
            spacing(Spacing::Sm, Density::Cozy),
            spacing(Spacing::Xxl, Density::Cozy),
        ])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(shell_bg)),
            border: Border {
                color: border_col,
                width: BORDER_THIN,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn active_row_wrapper<'a, Msg: 'a>(
    content: Element<'a, Msg>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let accent = palette.success;
    let bg = palette.shell;

    let strip = container(Space::new())
        .width(2.0)
        .height(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(accent)),
            ..container::Style::default()
        });

    let padded = container(content)
        .padding([
            spacing(Spacing::Sm, Density::Cozy),
            spacing(Spacing::Xxl, Density::Cozy),
        ])
        .width(Length::Fill);

    container(Row::new().push(strip).push(padded))
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            ..container::Style::default()
        })
        .into()
}

fn plain_row_wrapper<'a, Msg: 'a>(content: Element<'a, Msg>, bg: Color) -> Element<'a, Msg> {
    container(content)
        .padding([
            spacing(Spacing::Sm, Density::Cozy),
            spacing(Spacing::Xxl, Density::Cozy),
        ])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            ..container::Style::default()
        })
        .into()
}

fn card_container<'a, Msg: 'a>(
    content: Column<'a, Msg>,
    bg: Color,
    border_color: Color,
    r: f32,
) -> Element<'a, Msg> {
    container(content)
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn horizontal_divider<'a, Msg: 'a>(color: Color) -> Element<'a, Msg> {
    container(Space::new())
        .width(Length::Fill)
        .height(1.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(color)),
            ..container::Style::default()
        })
        .into()
}

fn active_badge<'a, Msg: 'a>(label: &str, palette: &'a ForgePalette) -> Element<'a, Msg> {
    let success = palette.success;
    let surface = palette.surface_overlay;
    let r = radius(Radius::Xxl);
    container(text(label.to_uppercase()).size(FONT_CAPS_XS).color(success))
        .padding([1, 6])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(surface)),
            border: Border {
                radius: r.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn trailing_token_elem<'a, Msg: 'a>(
    token: &'a TrailingToken,
    icon_color: Color,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    match token {
        TrailingToken::Badge(label, color) => {
            let tc = token_color_value(color, palette);
            let surface = palette.surface_overlay;
            let r = radius(Radius::Xxl);
            container(text(label.clone()).size(FONT_CAPS_XS).color(tc))
                .padding([1, 6])
                .style(move |_: &iced::Theme| container::Style {
                    background: Some(iced::Background::Color(surface)),
                    border: Border {
                        radius: r.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                })
                .into()
        }
        TrailingToken::Icon(icon) => text(icon.as_str().to_owned())
            .font(BOOTSTRAP_FONT)
            .size(FONT_BODY_SM)
            .color(icon_color)
            .into(),
        TrailingToken::Label(label) => text(label.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS_SM)
            .color(palette.text_faint)
            .into(),
    }
}

fn token_color_value(color: &TokenColor, palette: &ForgePalette) -> Color {
    match color {
        TokenColor::Green => palette.success,
        TokenColor::Yellow => palette.warning,
        TokenColor::Red => palette.random,
        TokenColor::Muted => palette.text_faint,
    }
}

fn subscription_status_color(status: &SubscriptionStatus, palette: &ForgePalette) -> Color {
    match status {
        SubscriptionStatus::Active => palette.success,
        SubscriptionStatus::Degraded => palette.warning,
        SubscriptionStatus::Error => palette.random,
    }
}

fn banner_level_color(level: &BannerLevel, palette: &ForgePalette) -> Color {
    match level {
        BannerLevel::Warning => palette.warning,
        BannerLevel::Info => palette.info,
        BannerLevel::Error => palette.random,
    }
}

fn health_level_color(level: &HealthLevel, palette: &ForgePalette) -> Color {
    match level {
        HealthLevel::Good => palette.success,
        HealthLevel::Ok => palette.warning,
        HealthLevel::Bad => palette.random,
        HealthLevel::NoData => palette.disabled,
    }
}

pub fn integration_quick_actions_grid<'a, Msg: Clone + 'a>(
    actions: &'a [QuickAction],
    on_click: impl Fn(usize) -> Msg + 'a,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Xxl);

    let header = quick_actions_section_header(palette);
    let divider = horizontal_divider(border_color);

    let capped: &[QuickAction] = if actions.len() > 4 {
        &actions[..4]
    } else {
        actions
    };

    let gap = spacing(Spacing::Sm, Density::Cozy) as f32;
    let mut btn_row: Row<'a, Msg> = Row::new().spacing(gap);
    for (i, action) in capped.iter().enumerate() {
        let msg = if action.enabled {
            Some(on_click(i))
        } else {
            None
        };
        btn_row = btn_row.push(quick_action_btn(action, msg, palette));
    }
    for _ in capped.len()..4 {
        btn_row = btn_row.push(Space::new().width(Length::Fill));
    }

    let grid_container = container(btn_row)
        .padding([
            spacing(Spacing::Lg, Density::Cozy),
            spacing(Spacing::Xxl, Density::Cozy),
        ])
        .width(Length::Fill);

    card_container(
        Column::new()
            .push(header)
            .push(divider)
            .push(grid_container),
        bg,
        border_color,
        r,
    )
}

fn quick_actions_section_header<'a, Msg: 'a>(palette: &ForgePalette) -> Element<'a, Msg> {
    use crate::icons::ICON_LIGHTNING;

    let icon_elem = text(ICON_LIGHTNING.to_string())
        .font(BOOTSTRAP_FONT)
        .size(FONT_BODY_LG)
        .color(palette.warning);

    let title_elem = text("Quick actions")
        .size(FONT_BODY_MD)
        .color(palette.text_primary);

    let left = Row::new()
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(icon_elem)
        .push(title_elem);

    container(left)
        .padding([
            spacing(Spacing::Lg, Density::Cozy),
            spacing(Spacing::Xxl, Density::Cozy),
        ])
        .width(Length::Fill)
        .into()
}

fn quick_action_btn<'a, Msg: Clone + 'a>(
    action: &'a QuickAction,
    msg: Option<Msg>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let shell = palette.shell;
    let border_color = palette.border_regular;
    let r = radius(Radius::Sm);
    let enabled = msg.is_some();

    let (icon_color, label_color, bg_color, bdr_color) = if enabled {
        (
            palette.text_secondary,
            palette.text_primary,
            shell,
            border_color,
        )
    } else {
        (
            Color {
                a: 0.5,
                ..palette.text_faint
            },
            Color {
                a: 0.5,
                ..palette.text_faint
            },
            Color { a: 0.5, ..shell },
            Color {
                a: 0.5,
                ..border_color
            },
        )
    };

    let icon_elem: Element<'a, Msg> = text(action.icon.as_str().to_owned())
        .font(BOOTSTRAP_FONT)
        .size(FONT_BODY)
        .color(icon_color)
        .into();

    let label_elem: Element<'a, Msg> = text(action.label.clone())
        .size(FONT_BODY_SM)
        .color(label_color)
        .into();

    let mut content_row: Row<'a, Msg> = Row::new()
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(icon_elem)
        .push(label_elem);

    if !enabled {
        let na_color = Color {
            a: 0.5,
            ..palette.text_faint
        };
        content_row = content_row
            .push(Space::new().width(Length::Fill))
            .push(text("N/A").size(FONT_CAPS_XS).color(na_color));
    }

    let mut btn = iced::widget::button(container(content_row).width(Length::Fill))
        .padding([
            spacing(Spacing::Sm, Density::Cozy),
            spacing(Spacing::Lg, Density::Cozy),
        ])
        .width(Length::Fill)
        .style(move |_: &iced::Theme, status| {
            use iced::widget::button::Status;
            let bg = if enabled && matches!(status, Status::Hovered) {
                Color {
                    a: (bg_color.a + 0.06).min(1.0),
                    ..bg_color
                }
            } else {
                bg_color
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: label_color,
                border: Border {
                    color: bdr_color,
                    width: BORDER_THIN,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

    if let Some(m) = msg {
        btn = btn.on_press(m);
    }

    btn.into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use forge_platform_core::{CapabilityFlags, ConnectionState, HeaderAction, SectionIcon};

    fn sample_flags() -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn sample_actions() -> Vec<HeaderAction> {
        vec![HeaderAction::Reconnect, HeaderAction::Settings]
    }

    #[test]
    fn integration_header_card_compiles_with_unit_msg() {
        let flags = sample_flags();
        let actions = sample_actions();
        let params = HeaderCardParams {
            display_name: "OBS Studio",
            version: Some("v31.0.2"),
            endpoint: Some("obs-websocket v5.5.0"),
            uptime: Some(Duration::from_secs(8040)),
            capability_flags: &flags,
            header_actions: &actions,
            connection: ConnectionState::Connected,
            icon: SectionIcon::new("\u{F1D6}"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_header_card_with_limited_flag() {
        let flags = CapabilityFlags {
            limited: true,
            label: Some("Chat only".to_owned()),
        };
        let actions = vec![HeaderAction::Reconnect, HeaderAction::Disconnect];
        let params = HeaderCardParams {
            display_name: "Kick",
            version: Some("channel 1247813"),
            endpoint: Some("pusher.kick.com"),
            uptime: Some(Duration::from_secs(8040)),
            capability_flags: &flags,
            header_actions: &actions,
            connection: ConnectionState::Connected,
            icon: SectionIcon::new("K"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_header_card_disconnected_no_optional_fields() {
        let flags = sample_flags();
        let actions = vec![HeaderAction::Reconnect];
        let params = HeaderCardParams {
            display_name: "OBS Studio",
            version: None,
            endpoint: None,
            uptime: None,
            capability_flags: &flags,
            header_actions: &actions,
            connection: ConnectionState::Disconnected,
            icon: SectionIcon::new("\u{F1D6}"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_header_card_empty_actions() {
        let flags = sample_flags();
        let params = HeaderCardParams {
            display_name: "OBS Studio",
            version: None,
            endpoint: None,
            uptime: None,
            capability_flags: &flags,
            header_actions: &[],
            connection: ConnectionState::Connecting,
            icon: SectionIcon::new("\u{F1D6}"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn format_uptime_seconds_only() {
        assert_eq!(format_uptime(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn format_uptime_minutes_and_seconds() {
        assert_eq!(format_uptime(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn format_uptime_hours_and_minutes() {
        assert_eq!(format_uptime(Duration::from_secs(8040)), "2h 14m");
    }

    #[test]
    fn format_uptime_exactly_one_hour() {
        assert_eq!(format_uptime(Duration::from_secs(3600)), "1h 0m");
    }

    #[test]
    fn sub_line_both_present() {
        let result = sub_line(Some("obs-websocket v5"), Some(Duration::from_secs(3600)));
        assert_eq!(result, "obs-websocket v5 · uptime 1h 0m");
    }

    #[test]
    fn sub_line_endpoint_only() {
        let result = sub_line(Some("pusher.kick.com"), None);
        assert_eq!(result, "pusher.kick.com");
    }

    #[test]
    fn sub_line_uptime_only() {
        let result = sub_line(None, Some(Duration::from_secs(60)));
        assert_eq!(result, "uptime 1m 0s");
    }

    #[test]
    fn sub_line_neither() {
        let result = sub_line(None, None);
        assert_eq!(result, "");
    }

    #[test]
    fn action_label_maps_all_variants() {
        assert_eq!(action_label(&HeaderAction::Reconnect), "Reconnect");
        assert_eq!(action_label(&HeaderAction::RefreshToken), "Refresh Token");
        assert_eq!(action_label(&HeaderAction::Disconnect), "Disconnect");
        assert_eq!(action_label(&HeaderAction::Settings), "Settings");
    }

    #[test]
    fn limited_badge_defaults_to_limited_when_no_label() {
        let flags = CapabilityFlags {
            limited: true,
            label: None,
        };
        let actions = sample_actions();
        let params = HeaderCardParams {
            display_name: "Kick",
            version: None,
            endpoint: None,
            uptime: None,
            capability_flags: &flags,
            header_actions: &actions,
            connection: ConnectionState::Connected,
            icon: SectionIcon::new("K"),
        };
        let _: Element<'_, ()> = integration_header_card(params, |_action| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_health_grid_renders_all_value_variants() {
        use forge_platform_core::{HealthMetric, HealthValue};

        let metrics: [HealthMetric; 4] = [
            HealthMetric {
                label: "Stream".to_owned(),
                value: HealthValue::Status {
                    label: "Live".to_owned(),
                    active: true,
                },
            },
            HealthMetric {
                label: "CPU / FPS".to_owned(),
                value: HealthValue::Pair {
                    left: "8.2%".to_owned(),
                    right: "60.0".to_owned(),
                },
            },
            HealthMetric {
                label: "Dropped".to_owned(),
                value: HealthValue::Text {
                    primary: "0 frames".to_owned(),
                    secondary: Some("0.00%".to_owned()),
                },
            },
            HealthMetric {
                label: "API Calls".to_owned(),
                value: HealthValue::Ratio {
                    used: 142,
                    total: 800,
                    reset_hint: Some("resets in 47s".to_owned()),
                },
            },
        ];

        let _: Element<'_, ()> = integration_health_grid(&metrics, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn integration_health_grid_renders_inactive_status_and_bare_text() {
        use forge_platform_core::{HealthMetric, HealthValue};

        let metrics: [HealthMetric; 4] = [
            HealthMetric {
                label: "Recording".to_owned(),
                value: HealthValue::Status {
                    label: "Off".to_owned(),
                    active: false,
                },
            },
            HealthMetric {
                label: "Viewers".to_owned(),
                value: HealthValue::Text {
                    primary: "1,284".to_owned(),
                    secondary: None,
                },
            },
            HealthMetric {
                label: "EventSub".to_owned(),
                value: HealthValue::Status {
                    label: "11 subs".to_owned(),
                    active: true,
                },
            },
            HealthMetric {
                label: "Quota".to_owned(),
                value: HealthValue::Ratio {
                    used: 142,
                    total: 10_000,
                    reset_hint: None,
                },
            },
        ];

        let _: Element<'_, ()> = integration_health_grid(&metrics, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn renderer_two_column_lists_compiles() {
        use forge_platform_core::{ContentList, ContentListItem, DetailSection, SectionIcon};

        let make_item = |name: &str, active: bool| ContentListItem {
            icon: SectionIcon::new("e"),
            name: name.to_owned(),
            monospace_name: false,
            active,
            active_label: if active {
                Some("LIVE".to_owned())
            } else {
                None
            },
            trailing: vec![],
            enabled: true,
        };
        let left = ContentList {
            title: "Scenes".to_owned(),
            icon: SectionIcon::new("L"),
            count_label: Some("2".to_owned()),
            items: vec![make_item("Gameplay", true), make_item("BRB", false)],
            footer: None,
        };
        let right = ContentList {
            title: "Sources".to_owned(),
            icon: SectionIcon::new("S"),
            count_label: None,
            items: vec![],
            footer: None,
        };
        let sections = [DetailSection::TwoColumnLists { left, right }];
        let _: Element<'_, ()> = integration_content_renderer(&sections, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn renderer_key_value_list_compiles() {
        use forge_platform_core::{DetailSection, KeyValueRow, RowAction, SectionIcon};

        let sections = [DetailSection::KeyValueList {
            title: "Hotkeys".to_owned(),
            icon: SectionIcon::new("K"),
            items: vec![
                KeyValueRow {
                    icon: SectionIcon::new("b"),
                    name: "Wave".to_owned(),
                    tag: Some("TriggerAnimation".to_owned()),
                    action: Some(RowAction::Play),
                },
                KeyValueRow {
                    icon: SectionIcon::new("b"),
                    name: "Zoom Close".to_owned(),
                    tag: None,
                    action: None,
                },
            ],
        }];
        let _: Element<'_, ()> = integration_content_renderer(&sections, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn renderer_active_item_list_compiles() {
        use forge_platform_core::{ActiveRow, DetailSection, SectionIcon};

        let sections = [DetailSection::ActiveItemList {
            title: "Expressions".to_owned(),
            icon: SectionIcon::new("E"),
            items: vec![
                ActiveRow {
                    name: "smile_big".to_owned(),
                    active: true,
                    mode_label: None,
                },
                ActiveRow {
                    name: "blush".to_owned(),
                    active: false,
                    mode_label: Some("hold".to_owned()),
                },
            ],
        }];
        let _: Element<'_, ()> = integration_content_renderer(&sections, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn renderer_warning_banner_compiles() {
        use forge_platform_core::{BannerLevel, DetailSection};

        let sections = [DetailSection::WarningBanner {
            level: BannerLevel::Warning,
            title: "Limited integration".to_owned(),
            body: "Kick has no public OAuth API.".to_owned(),
            cta: Some("Read more".to_owned()),
        }];
        let _: Element<'_, ()> = integration_content_renderer(&sections, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn renderer_subscription_list_compiles() {
        use forge_platform_core::{
            DetailSection, ListFooter, SectionIcon, SubscriptionRow, SubscriptionStatus,
        };

        let sections = [DetailSection::SubscriptionList {
            title: "EventSub subscriptions".to_owned(),
            icon: SectionIcon::new("R"),
            items: vec![
                SubscriptionRow {
                    name: "channel.subscribe".to_owned(),
                    status: SubscriptionStatus::Active,
                    version: Some("v1".to_owned()),
                    event_count: Some(18),
                    error_label: None,
                },
                SubscriptionRow {
                    name: "channel.ban".to_owned(),
                    status: SubscriptionStatus::Error,
                    version: Some("v1".to_owned()),
                    event_count: None,
                    error_label: Some("retry pending".to_owned()),
                },
            ],
            footer: Some(ListFooter {
                cta_label: Some("Subscribe to event".to_owned()),
                trailing_label: Some("3 hidden".to_owned()),
            }),
        }];
        let _: Element<'_, ()> = integration_content_renderer(&sections, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn renderer_scopes_list_compiles() {
        use forge_platform_core::{DetailSection, ListFooter};

        let sections = [DetailSection::ScopesList {
            title: "OAuth scopes".to_owned(),
            scopes: vec![
                "chat:read".to_owned(),
                "chat:edit".to_owned(),
                "bits:read".to_owned(),
            ],
            footer: Some(ListFooter {
                cta_label: Some("Request more scopes".to_owned()),
                trailing_label: None,
            }),
        }];
        let _: Element<'_, ()> = integration_content_renderer(&sections, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn renderer_info_card_compiles() {
        use forge_platform_core::{DetailSection, HealthBar, HealthLevel, InfoField};

        let sections = [DetailSection::InfoCard {
            title: "Current broadcast".to_owned(),
            live: true,
            fields: vec![
                InfoField {
                    label: "Privacy".to_owned(),
                    value: "Public".to_owned(),
                    monospace_value: false,
                },
                InfoField {
                    label: "Started".to_owned(),
                    value: "14:08:32 UTC".to_owned(),
                    monospace_value: true,
                },
            ],
            health_bar: Some(HealthBar {
                fraction: 0.96,
                label: "excellent · 1080p60".to_owned(),
                level: HealthLevel::Good,
            }),
        }];
        let _: Element<'_, ()> = integration_content_renderer(&sections, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn renderer_stats_grid_compiles() {
        use forge_platform_core::{DetailSection, SectionIcon, StatColumn};

        let sections = [DetailSection::StatsGrid {
            title: "Monetization events".to_owned(),
            icon: SectionIcon::new("C"),
            columns: vec![
                StatColumn {
                    label: "Super Chats".to_owned(),
                    value: "7".to_owned(),
                    subtitle: "$48.20 USD".to_owned(),
                },
                StatColumn {
                    label: "New Members".to_owned(),
                    value: "3".to_owned(),
                    subtitle: "2 new, 1 upgrade".to_owned(),
                },
                StatColumn {
                    label: "Super Stickers".to_owned(),
                    value: "2".to_owned(),
                    subtitle: "$4.00 USD".to_owned(),
                },
            ],
        }];
        let _: Element<'_, ()> = integration_content_renderer(&sections, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn quick_actions_grid_four_enabled_compiles() {
        use forge_platform_core::{QuickAction, SectionIcon};
        use forge_types::SubActionSpec;

        let actions: Vec<QuickAction> = (0..4)
            .map(|i| QuickAction {
                label: format!("Action {i}"),
                icon: SectionIcon::new("b"),
                enabled: true,
                subaction_template: SubActionSpec::Delay { ms: 0 },
                picker: None,
            })
            .collect();

        let _: Element<'_, ()> =
            integration_quick_actions_grid(&actions, |_idx| (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn quick_actions_grid_mixed_enabled_disabled_compiles() {
        use forge_platform_core::{QuickAction, SectionIcon};
        use forge_types::SubActionSpec;

        let actions = vec![
            QuickAction {
                label: "Resync".to_owned(),
                icon: SectionIcon::new("r"),
                enabled: true,
                subaction_template: SubActionSpec::Delay { ms: 0 },
                picker: None,
            },
            QuickAction {
                label: "Send message".to_owned(),
                icon: SectionIcon::new("s"),
                enabled: false,
                subaction_template: SubActionSpec::Delay { ms: 0 },
                picker: None,
            },
            QuickAction {
                label: "Scrape info".to_owned(),
                icon: SectionIcon::new("i"),
                enabled: true,
                subaction_template: SubActionSpec::Delay { ms: 0 },
                picker: None,
            },
        ];

        let _: Element<'_, ()> =
            integration_quick_actions_grid(&actions, |_idx| (), &CATPPUCCIN_MOCHA);
    }
}
