use forge_platform_core::{
    ActiveRow, BannerLevel, ContentList, ContentListItem, DetailSection, HealthBar, HealthLevel,
    InfoField, KeyValueRow, ListFooter, RowAction, SectionIcon, StatColumn, SubscriptionRow,
    SubscriptionStatus, TokenColor, TrailingToken,
};
use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Column, Row, Space, container, text},
};

use crate::{
    icons::{Icon, tabler_icon},
    palette::ForgePalette,
    tokens::{
        BORDER_THIN, Density, FONT_LG, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius,
        sp, spacing,
    },
};

pub fn builtin_content_renderer<'a, Msg: 'a>(
    sections: &'a [DetailSection],
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let gap = spacing(Spacing::Lg, Density::Cozy) as f32;
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
    let gap = spacing(Spacing::Md, Density::Cozy) as f32;
    Row::new()
        .spacing(gap)
        .push(container(content_list_panel(left, palette)).width(Length::FillPortion(10)))
        .push(container(content_list_panel(right, palette)).width(Length::FillPortion(13)))
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
    let r = radius(Radius::Md);

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
    let r = radius(Radius::Md);
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
    let r = radius(Radius::Md);

    let icon_char = match level {
        BannerLevel::Warning => '\u{26A0}',
        BannerLevel::Info => '\u{2139}',
        BannerLevel::Error => '\u{2715}',
    };

    let icon_elem = text(icon_char.to_string()).size(FONT_SM).color(level_color);
    let title_elem = text(title.to_owned())
        .size(FONT_SM)
        .color(palette.text_primary);
    let body_elem = text(body.to_owned())
        .size(FONT_SM)
        .color(palette.text_muted);

    let mut text_col = Column::new()
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
        .push(title_elem)
        .push(body_elem);

    if let Some(cta_label) = cta {
        let brand = palette.brand;
        text_col = text_col.push(
            text(format!("{cta_label} \u{2192}"))
                .size(FONT_SM)
                .color(brand),
        );
    }

    let inner = Row::new()
        .spacing(spacing(Spacing::Md, Density::Cozy) as f32)
        .align_y(Alignment::Start)
        .push(icon_elem)
        .push(container(text_col).width(Length::Fill));

    container(inner)
        .padding([
            spacing(Spacing::Sm, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
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
    let r = radius(Radius::Md);
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
    let r = radius(Radius::Md);
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
    let r = radius(Radius::Md);

    let header = info_card_header(title, live, palette);
    let divider = horizontal_divider(border_color);

    let fields_grid: Element<'a, Msg> = fields
        .chunks(2)
        .fold(
            Column::new().spacing(spacing(Spacing::Md, Density::Cozy) as f32),
            |col, chunk| {
                let mut row = Row::new().spacing(spacing(Spacing::Md, Density::Cozy) as f32);
                for field in chunk {
                    row = row.push(info_field_cell(field, palette));
                }
                col.push(row)
            },
        )
        .into();

    let mut content_col = Column::new()
        .spacing(spacing(Spacing::Md, Density::Cozy) as f32)
        .push(fields_grid);

    if let Some(bar) = health_bar {
        content_col = content_col.push(health_bar_section(bar, palette));
    }

    let content_padded = container(content_col)
        .padding([
            spacing(Spacing::Md, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
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
    let r = radius(Radius::Md);
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
    let r = radius(Radius::Md);

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

    let icon_elem = tabler_icon(Icon::from_name(item.icon.as_str()), FONT_SM, icon_color);

    let name_elem: Element<'a, Msg> = if item.monospace_name {
        text(item.name.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(text_color)
            .into()
    } else {
        text(item.name.clone())
            .size(FONT_SM)
            .color(text_color)
            .into()
    };

    let mut trailing: Row<'a, Msg> = Row::new()
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
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
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
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
    let icon_elem = tabler_icon(
        Icon::from_name(item.icon.as_str()),
        FONT_SM,
        palette.text_secondary,
    );

    let name_elem = text(item.name.clone())
        .font(font(FontRole::Monospace))
        .size(FONT_SM)
        .color(palette.text_primary);

    let mut row = Row::new()
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(icon_elem)
        .push(container(name_elem).width(Length::Fill));

    if let Some(tag) = &item.tag {
        row = row.push(
            text(tag.clone())
                .font(font(FontRole::Monospace))
                .size(FONT_XS)
                .color(palette.text_faint),
        );
    }

    if let Some(action) = &item.action {
        let icon = match action {
            RowAction::Play => Icon::PlayerPlay,
        };
        row = row.push(tabler_icon(icon, FONT_SM, palette.success));
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
        .size(FONT_SM)
        .color(text_color);

    let mut row = Row::new()
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(container(name_elem).width(Length::Fill));

    if item.active {
        row = row.push(active_badge("ACTIVE", palette));
    } else if let Some(mode) = &item.mode_label {
        row = row.push(text(mode.clone()).size(FONT_XS).color(palette.text_faint));
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
        .size(FONT_XS)
        .color(palette.text_primary);

    let mut row = Row::new()
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(dot)
        .push(container(name_elem).width(Length::Fill));

    if let Some(ver) = &item.version {
        row = row.push(
            text(ver.clone())
                .font(font(FontRole::Monospace))
                .size(FONT_XS)
                .color(palette.text_faint),
        );
    }

    let trailing: Element<'a, Msg> = if let Some(err) = &item.error_label {
        text(err.clone()).size(FONT_XS).color(palette.random).into()
    } else if let Some(count) = item.event_count {
        let label = if count == 1 {
            format!("{count} event")
        } else {
            format!("{count} events")
        };
        text(label).size(FONT_XS).color(palette.text_muted).into()
    } else {
        Space::new().into()
    };

    row = row.push(trailing);
    plain_row_wrapper(row.into(), palette.elevated)
}

fn scope_row_elem<'a, Msg: 'a>(scope: &str, palette: &'a ForgePalette) -> Element<'a, Msg> {
    let check = tabler_icon(Icon::CircleCheck, FONT_SM, palette.success);
    let scope_text = text(scope.to_owned())
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(palette.text_primary);

    plain_row_wrapper(
        Row::new()
            .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
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
        .size(FONT_XS)
        .color(palette.text_muted);

    let value_elem: Element<'a, Msg> = if field.monospace_value {
        text(field.value.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.text_primary)
            .into()
    } else {
        text(field.value.clone())
            .size(FONT_SM)
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
        .size(FONT_XS)
        .color(palette.text_muted);

    let bar_row = Row::new()
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(container(bar_track).width(Length::Fill))
        .push(
            text(bar.label.clone())
                .font(font(FontRole::Monospace))
                .size(FONT_XS)
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
                    .size(FONT_XS)
                    .color(palette.text_muted),
            )
            .push(
                text(col.value.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_LG)
                    .color(palette.text_primary),
            )
            .push(
                text(col.subtitle.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_XS)
                    .color(palette.success),
            ),
    )
    .padding([
        spacing(Spacing::Sm, Density::Cozy),
        spacing(Spacing::Md, Density::Cozy),
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
    let icon_elem = tabler_icon(Icon::from_name(icon_str), FONT_SM, palette.text_secondary);

    let left = Row::new()
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
        .align_y(Alignment::Center)
        .push(icon_elem)
        .push(
            text(title.to_owned())
                .size(FONT_SM)
                .color(palette.text_primary),
        );

    let mut outer = Row::new()
        .align_y(Alignment::Center)
        .push(container(left).width(Length::Fill));

    if let Some(c) = count {
        outer = outer.push(
            text(c.to_owned())
                .font(font(FontRole::Monospace))
                .size(FONT_XS)
                .color(palette.text_faint),
        );
    }

    container(outer)
        .padding([
            spacing(Spacing::Sm, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
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
        .size(FONT_XS)
        .color(palette.text_faint);

    container(
        Row::new()
            .align_y(Alignment::Center)
            .push(
                container(
                    text(title.to_owned())
                        .size(FONT_SM)
                        .color(palette.text_primary),
                )
                .width(Length::Fill),
            )
            .push(count_elem),
    )
    .padding([
        spacing(Spacing::Sm, Density::Cozy),
        spacing(Spacing::Md, Density::Cozy),
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
                .size(FONT_SM)
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
                .push(text("LIVE").size(FONT_XS).color(success)),
        )
        .padding([0, sp(Spacing::Xs)])
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
            spacing(Spacing::Sm, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
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
        .spacing(spacing(Spacing::Sm, Density::Cozy) as f32)
        .align_y(Alignment::Center);

    if let Some(cta) = &footer.cta_label {
        let brand = palette.brand;
        row = row.push(container(text(cta.clone()).size(FONT_XS).color(brand)).width(Length::Fill));
    } else {
        row = row.push(container(Space::new()).width(Length::Fill));
    }

    if let Some(trail) = &footer.trailing_label {
        row = row.push(
            text(trail.clone())
                .font(font(FontRole::Monospace))
                .size(FONT_XS)
                .color(palette.text_faint),
        );
    }

    container(row)
        .padding([
            spacing(Spacing::Xs, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
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
            spacing(Spacing::Xs, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
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
            spacing(Spacing::Xs, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
        ])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            ..container::Style::default()
        })
        .into()
}

pub(crate) fn card_container<'a, Msg: 'a>(
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

pub(crate) fn horizontal_divider<'a, Msg: 'a>(color: Color) -> Element<'a, Msg> {
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
    let r = radius(Radius::Md);
    container(text(label.to_uppercase()).size(FONT_XS).color(success))
        .padding([0, sp(Spacing::Xs)])
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
            let r = radius(Radius::Md);
            container(text(label.clone()).size(FONT_XS).color(tc))
                .padding([0, sp(Spacing::Xs)])
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
        TrailingToken::Icon(icon) => {
            tabler_icon(Icon::from_name(icon.as_str()), FONT_SM, icon_color)
        }
        TrailingToken::Label(label) => text(label.clone())
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
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
