use forge_components::{
    BORDER_THIN, Density, FONT_LG, FONT_SM, FONT_XS, ForgePalette, Icon, Radius, Spacing,
    body_family, icon, mono_family, radius, spacing, status_dot, tr, with_alpha,
};
use forge_platform_core::{
    ActiveRow, BannerLevel, ContentList, ContentListItem, DetailSection, HealthBar, HealthLevel,
    HealthMetric, HealthValue, InfoField, KeyValueRow, ListFooter, RowAction, SectionIcon,
    StatColumn, SubscriptionRow, SubscriptionStatus, TokenColor, TrailingToken,
};
use gpui::{AnyElement, Div, Rgba, SharedString, div, prelude::*, px, relative};

fn mono(s: impl Into<SharedString>, size: gpui::Pixels, color: Rgba) -> Div {
    div()
        .font_family(mono_family())
        .text_size(size)
        .text_color(color)
        .child(s.into())
}

fn body(s: impl Into<SharedString>, size: gpui::Pixels, color: Rgba) -> Div {
    div()
        .font_family(body_family())
        .text_size(size)
        .text_color(color)
        .child(s.into())
}

fn divider(palette: &ForgePalette) -> Div {
    div().w_full().h(BORDER_THIN).bg(palette.border_regular)
}

fn card_shell(palette: &ForgePalette) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
}

fn grow_cell(el: impl IntoElement, grow: f32) -> Div {
    let mut cell = div().min_w(px(0.0)).child(el);
    let style = cell.style();
    style.flex_grow = Some(grow);
    style.flex_basis = Some(relative(0.0).into());
    cell
}

pub fn content_sections(
    sections: &[DetailSection],
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut col = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Lg, density));
    for section in sections {
        col = col.child(dispatch_section(section, palette, density));
    }
    col.into_any_element()
}

fn dispatch_section(
    section: &DetailSection,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    match section {
        DetailSection::TwoColumnLists { left, right } => {
            render_two_column_lists(left, right, palette, density)
        }
        DetailSection::KeyValueList { title, icon, items } => {
            render_key_value_list(title, icon, items, palette, density)
        }
        DetailSection::ActiveItemList { title, icon, items } => {
            render_active_item_list(title, icon, items, palette, density)
        }
        DetailSection::WarningBanner {
            level,
            title,
            body,
            cta,
        } => render_warning_banner(level, title, body, cta.as_deref(), palette, density),
        DetailSection::SubscriptionList {
            title,
            icon,
            items,
            footer,
        } => render_subscription_list(title, icon, items, footer.as_ref(), palette, density),
        DetailSection::ScopesList {
            title,
            scopes,
            footer,
        } => render_scopes_list(title, scopes, footer.as_ref(), palette, density),
        DetailSection::InfoCard {
            title,
            live,
            fields,
            health_bar,
        } => render_info_card(title, *live, fields, health_bar.as_ref(), palette, density),
        DetailSection::StatsGrid {
            title,
            icon,
            columns,
        } => render_stats_grid(title, icon, columns, palette, density),
    }
}

fn render_two_column_lists(
    left: &ContentList,
    right: &ContentList,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .gap(spacing(Spacing::Md, density))
        .child(grow_cell(content_list_panel(left, palette, density), 10.0))
        .child(grow_cell(content_list_panel(right, palette, density), 13.0))
        .into_any_element()
}

fn render_key_value_list(
    title: &str,
    icon: &SectionIcon,
    items: &[KeyValueRow],
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut rows = div().w_full().flex().flex_col();
    for item in items {
        rows = rows.child(key_value_row_elem(item, palette, density));
    }
    card_shell(palette)
        .child(panel_header_row(
            icon.as_str(),
            title,
            None,
            palette,
            density,
        ))
        .child(divider(palette))
        .child(rows)
        .into_any_element()
}

fn render_active_item_list(
    title: &str,
    icon: &SectionIcon,
    items: &[ActiveRow],
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let count = if items.is_empty() {
        None
    } else {
        Some(items.len().to_string())
    };
    let mut rows = div().w_full().flex().flex_col();
    for item in items {
        rows = rows.child(active_item_row_elem(item, palette, density));
    }
    card_shell(palette)
        .child(panel_header_row(
            icon.as_str(),
            title,
            count.as_deref(),
            palette,
            density,
        ))
        .child(divider(palette))
        .child(rows)
        .into_any_element()
}

fn render_warning_banner(
    level: &BannerLevel,
    title: &str,
    banner_body: &str,
    cta: Option<&str>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let level_color = banner_level_color(level, palette);
    let glyph = match level {
        BannerLevel::Warning => "\u{26A0}",
        BannerLevel::Info => "\u{2139}",
        BannerLevel::Error => "\u{2715}",
    };

    let mut text_col = div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(body(title.to_owned(), FONT_SM, palette.text_primary))
        .child(body(banner_body.to_owned(), FONT_SM, palette.text_muted));
    if let Some(cta_label) = cta {
        text_col = text_col.child(body(
            format!("{cta_label} \u{2192}"),
            FONT_SM,
            palette.brand,
        ));
    }

    div()
        .w_full()
        .flex()
        .items_start()
        .gap(spacing(Spacing::Md, density))
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(level_color)
        .bg(palette.elevated)
        .child(body(glyph, FONT_SM, level_color))
        .child(text_col)
        .into_any_element()
}

fn render_subscription_list(
    title: &str,
    icon: &SectionIcon,
    items: &[SubscriptionRow],
    footer: Option<&ListFooter>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let count = tr!("widget_builtin_active_count", count = items.len() as i64);
    let mut rows = div().w_full().flex().flex_col();
    for item in items {
        rows = rows.child(subscription_row_elem(item, palette, density));
    }
    let mut card = card_shell(palette)
        .child(panel_header_row(
            icon.as_str(),
            title,
            Some(&count),
            palette,
            density,
        ))
        .child(divider(palette))
        .child(rows);
    if let Some(f) = footer {
        card = card.child(list_footer_bar(f, palette, density));
    }
    card.into_any_element()
}

fn render_scopes_list(
    title: &str,
    scopes: &[String],
    footer: Option<&ListFooter>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let count = scopes.len().to_string();
    let mut rows = div().w_full().flex().flex_col();
    for scope in scopes {
        rows = rows.child(scope_row_elem(scope, palette, density));
    }
    let mut card = card_shell(palette)
        .child(scopes_list_header(title, &count, palette, density))
        .child(divider(palette))
        .child(rows);
    if let Some(f) = footer {
        card = card.child(list_footer_bar(f, palette, density));
    }
    card.into_any_element()
}

fn render_info_card(
    title: &str,
    live: bool,
    fields: &[InfoField],
    health_bar: Option<&HealthBar>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut fields_grid = div().flex().flex_col().gap(spacing(Spacing::Md, density));
    for chunk in fields.chunks(2) {
        let mut row = div().flex().gap(spacing(Spacing::Md, density));
        for field in chunk {
            row = row.child(info_field_cell(field, palette, density));
        }
        fields_grid = fields_grid.child(row);
    }

    let mut content_col = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Md, density))
        .child(fields_grid);
    if let Some(bar) = health_bar {
        content_col = content_col.child(health_bar_section(bar, palette, density));
    }

    let content_padded = div()
        .w_full()
        .py(spacing(Spacing::Md, density))
        .px(spacing(Spacing::Md, density))
        .child(content_col);

    card_shell(palette)
        .child(info_card_header(title, live, palette, density))
        .child(divider(palette))
        .child(content_padded)
        .into_any_element()
}

fn render_stats_grid(
    title: &str,
    icon: &SectionIcon,
    columns: &[StatColumn],
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut stats_row = div().w_full().flex();
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            stats_row =
                stats_row.child(div().flex_none().w(BORDER_THIN).bg(palette.border_regular));
        }
        stats_row = stats_row.child(stat_column_cell(col, palette, density));
    }
    card_shell(palette)
        .child(panel_header_row(
            icon.as_str(),
            title,
            None,
            palette,
            density,
        ))
        .child(divider(palette))
        .child(stats_row)
        .into_any_element()
}

fn content_list_panel(list: &ContentList, palette: &ForgePalette, density: Density) -> AnyElement {
    let mut rows = div().w_full().flex().flex_col();
    for item in &list.items {
        rows = rows.child(content_list_item_row(item, palette, density));
    }
    let mut card = card_shell(palette)
        .child(panel_header_row(
            list.icon.as_str(),
            &list.title,
            list.count_label.as_deref(),
            palette,
            density,
        ))
        .child(divider(palette))
        .child(rows);
    if let Some(f) = &list.footer {
        card = card.child(list_footer_bar(f, palette, density));
    }
    card.into_any_element()
}

fn content_list_item_row(
    item: &ContentListItem,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let dim = if item.enabled { 1.0 } else { 0.4 };
    let text_color = with_alpha(
        if item.active {
            palette.text_primary
        } else {
            palette.text_secondary
        },
        dim,
    );
    let icon_color = with_alpha(
        if item.active {
            palette.success
        } else {
            palette.text_faint
        },
        dim,
    );

    let name_family = if item.monospace_name {
        mono_family()
    } else {
        body_family()
    };
    let name_el = div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(name_family)
        .text_size(FONT_SM)
        .text_color(text_color)
        .child(item.name.clone());

    let mut trailing = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density));
    if item.active
        && let Some(label) = &item.active_label
    {
        trailing = trailing.child(active_badge(label, palette, density));
    }
    for token in &item.trailing {
        trailing = trailing.child(trailing_token_elem(
            token, icon_color, dim, palette, density,
        ));
    }

    let content = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(icon(
            Icon::from_name(item.icon.as_str()),
            FONT_SM,
            icon_color,
        ))
        .child(name_el)
        .child(trailing);

    if item.active {
        active_row_wrapper(content.into_any_element(), palette, density)
    } else {
        plain_row_wrapper(content.into_any_element(), palette, density)
    }
}

fn key_value_row_elem(item: &KeyValueRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let name_el = div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(mono_family())
        .text_size(FONT_SM)
        .text_color(palette.text_primary)
        .child(item.name.clone());

    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(icon(
            Icon::from_name(item.icon.as_str()),
            FONT_SM,
            palette.text_secondary,
        ))
        .child(name_el);
    if let Some(tag) = &item.tag {
        row = row.child(mono(tag.clone(), FONT_XS, palette.text_faint));
    }
    if let Some(action) = &item.action {
        let ic = match action {
            RowAction::Play => Icon::PlayerPlay,
        };
        row = row.child(icon(ic, FONT_SM, palette.success));
    }
    plain_row_wrapper(row.into_any_element(), palette, density)
}

fn active_item_row_elem(item: &ActiveRow, palette: &ForgePalette, density: Density) -> AnyElement {
    let text_color = if item.active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    let name_el = div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(mono_family())
        .text_size(FONT_SM)
        .text_color(text_color)
        .child(item.name.clone());

    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(name_el);
    if item.active {
        row = row.child(active_badge(
            &tr!("widget_builtin_active_badge"),
            palette,
            density,
        ));
    } else if let Some(mode) = &item.mode_label {
        row = row.child(body(mode.clone(), FONT_XS, palette.text_faint));
    }

    if item.active {
        active_row_wrapper(row.into_any_element(), palette, density)
    } else {
        plain_row_wrapper(row.into_any_element(), palette, density)
    }
}

fn subscription_row_elem(
    item: &SubscriptionRow,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let name_el = div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(mono_family())
        .text_size(FONT_XS)
        .text_color(palette.text_primary)
        .child(item.name.clone());

    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(status_dot(
            subscription_status_color(&item.status, palette),
            px(6.0),
        ))
        .child(name_el);
    if let Some(ver) = &item.version {
        row = row.child(mono(ver.clone(), FONT_XS, palette.text_faint));
    }
    let trailing: AnyElement = if let Some(err) = &item.error_label {
        body(err.clone(), FONT_XS, palette.random).into_any_element()
    } else if let Some(count) = item.event_count {
        body(
            tr!("widget_builtin_event_count", count = count as i64),
            FONT_XS,
            palette.text_muted,
        )
        .into_any_element()
    } else {
        div().into_any_element()
    };
    row = row.child(trailing);
    plain_row_wrapper(row.into_any_element(), palette, density)
}

fn scope_row_elem(scope: &str, palette: &ForgePalette, density: Density) -> AnyElement {
    let row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(icon(Icon::CircleCheck, FONT_SM, palette.success))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(scope.to_owned()),
        );
    plain_row_wrapper(row.into_any_element(), palette, density)
}

fn info_field_cell(field: &InfoField, palette: &ForgePalette, density: Density) -> AnyElement {
    let value_family = if field.monospace_value {
        mono_family()
    } else {
        body_family()
    };
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(mono(
            field.label.to_uppercase(),
            FONT_XS,
            palette.text_muted,
        ))
        .child(
            div()
                .font_family(value_family)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(field.value.clone()),
        )
        .into_any_element()
}

fn health_bar_section(bar: &HealthBar, palette: &ForgePalette, density: Density) -> AnyElement {
    let level_color = health_level_color(&bar.level, palette);
    let fraction = bar.fraction.clamp(0.0, 1.0);

    let track = div()
        .flex_1()
        .min_w(px(0.0))
        .h(px(6.0))
        .rounded(px(5.0))
        .bg(palette.shell)
        .child(
            div()
                .h(px(6.0))
                .w(relative(fraction))
                .rounded(px(5.0))
                .bg(level_color),
        );

    let bar_row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(track)
        .child(mono(bar.label.clone(), FONT_XS, level_color));

    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(mono(
            tr!("widget_builtin_stream_health"),
            FONT_XS,
            palette.text_muted,
        ))
        .child(bar_row)
        .into_any_element()
}

fn stat_column_cell(col: &StatColumn, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .child(mono(col.label.to_uppercase(), FONT_XS, palette.text_muted))
        .child(mono(col.value.clone(), FONT_LG, palette.text_primary))
        .child(mono(col.subtitle.clone(), FONT_XS, palette.success))
        .into_any_element()
}

fn panel_header_icon_color(icon_str: &str, palette: &ForgePalette) -> Rgba {
    match icon_str {
        "key" => palette.warning,
        "rss" => palette.brand,
        _ => palette.text_secondary,
    }
}

fn panel_header_row(
    icon_str: &str,
    title: &str,
    count: Option<&str>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let left = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .child(icon(
            Icon::from_name(icon_str),
            FONT_SM,
            panel_header_icon_color(icon_str, palette),
        ))
        .child(body(title.to_owned(), FONT_SM, palette.text_primary));

    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .child(left);
    if let Some(c) = count {
        row = row.child(mono(c.to_owned(), FONT_XS, palette.text_faint));
    }
    row.into_any_element()
}

fn scopes_list_header(
    title: &str,
    count: &str,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .child(div().flex_1().min_w(px(0.0)).child(body(
            title.to_owned(),
            FONT_SM,
            palette.text_primary,
        )))
        .child(mono(count.to_owned(), FONT_XS, palette.text_faint))
        .into_any_element()
}

fn info_card_header(
    title: &str,
    live: bool,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .child(div().flex_1().min_w(px(0.0)).child(body(
            title.to_owned(),
            FONT_SM,
            palette.text_primary,
        )));
    if live {
        let badge = div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .py(px(0.0))
            .px(spacing(Spacing::Xs, density))
            .rounded(px(8.0))
            .bg(palette.surface_overlay)
            .child(status_dot(palette.success, px(5.0)))
            .child(body(
                tr!("widget_builtin_live_badge"),
                FONT_XS,
                palette.success,
            ));
        row = row.child(badge);
    }
    row.into_any_element()
}

fn list_footer_bar(footer: &ListFooter, palette: &ForgePalette, density: Density) -> AnyElement {
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Md, density))
        .border_t(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.shell);

    let lead = div().flex_1().min_w(px(0.0));
    if let Some(cta) = &footer.cta_label {
        row = row.child(lead.child(body(cta.clone(), FONT_XS, palette.brand)));
    } else {
        row = row.child(lead);
    }
    if let Some(trail) = &footer.trailing_label {
        row = row.child(mono(trail.clone(), FONT_XS, palette.text_faint));
    }
    row.into_any_element()
}

fn active_row_wrapper(content: AnyElement, palette: &ForgePalette, density: Density) -> AnyElement {
    let padded = div()
        .flex_1()
        .min_w(px(0.0))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Md, density))
        .child(content);
    div()
        .w_full()
        .flex()
        .bg(palette.shell)
        .child(div().flex_none().w(px(2.0)).bg(palette.success))
        .child(padded)
        .into_any_element()
}

fn plain_row_wrapper(content: AnyElement, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .w_full()
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Md, density))
        .bg(palette.elevated)
        .child(content)
        .into_any_element()
}

fn active_badge(label: &str, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .flex_none()
        .py(px(0.0))
        .px(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Md))
        .bg(palette.surface_overlay)
        .child(body(label.to_uppercase(), FONT_XS, palette.success))
        .into_any_element()
}

fn trailing_token_elem(
    token: &TrailingToken,
    icon_color: Rgba,
    dim: f32,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    match token {
        TrailingToken::Badge(label, color) => {
            let tc = with_alpha(token_color_value(color, palette), dim);
            div()
                .flex_none()
                .py(px(0.0))
                .px(spacing(Spacing::Xs, density))
                .rounded(radius(Radius::Md))
                .bg(palette.surface_overlay)
                .child(body(label.clone(), FONT_XS, tc))
                .into_any_element()
        }
        TrailingToken::Icon(ic) => {
            icon(Icon::from_name(ic.as_str()), FONT_SM, icon_color).into_any_element()
        }
        TrailingToken::Label(label) => {
            mono(label.clone(), FONT_XS, with_alpha(palette.text_faint, dim)).into_any_element()
        }
    }
}

fn token_color_value(color: &TokenColor, palette: &ForgePalette) -> Rgba {
    match color {
        TokenColor::Green => palette.success,
        TokenColor::Yellow => palette.warning,
        TokenColor::Red => palette.random,
        TokenColor::Muted => palette.text_faint,
    }
}

fn subscription_status_color(status: &SubscriptionStatus, palette: &ForgePalette) -> Rgba {
    match status {
        SubscriptionStatus::Active => palette.success,
        SubscriptionStatus::Degraded => palette.warning,
        SubscriptionStatus::Error => palette.random,
    }
}

fn banner_level_color(level: &BannerLevel, palette: &ForgePalette) -> Rgba {
    match level {
        BannerLevel::Warning => palette.warning,
        BannerLevel::Info => palette.info,
        BannerLevel::Error => palette.random,
    }
}

fn health_level_color(level: &HealthLevel, palette: &ForgePalette) -> Rgba {
    match level {
        HealthLevel::Good => palette.success,
        HealthLevel::Ok => palette.warning,
        HealthLevel::Bad => palette.random,
        HealthLevel::NoData => palette.disabled,
    }
}

pub fn health_grid(
    metrics: &[HealthMetric; 4],
    loading: bool,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut row = div().w_full().flex().gap(spacing(Spacing::Sm, density));
    for metric in metrics.iter() {
        row = row.child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(health_metric_card(metric, loading, palette, density)),
        );
    }
    row.into_any_element()
}

fn health_metric_card(
    metric: &HealthMetric,
    loading: bool,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let value_col: AnyElement = if loading {
        body("-", FONT_SM, palette.text_faint).into_any_element()
    } else {
        health_value_col(&metric.value, palette, density)
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
        .child(mono(
            metric.label.to_uppercase(),
            FONT_XS,
            palette.text_muted,
        ))
        .child(value_col)
        .into_any_element()
}

fn health_value_col(value: &HealthValue, palette: &ForgePalette, density: Density) -> AnyElement {
    match value {
        HealthValue::Status {
            label,
            active,
            detail,
        } => {
            let color = if *active {
                palette.success
            } else {
                palette.disabled
            };
            let value_row = div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .child(status_dot(color, px(7.0)))
                .child(body(label.clone(), FONT_SM, color));
            if let Some(d) = detail {
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xs, density))
                    .child(value_row)
                    .child(mono(d.clone(), FONT_XS, palette.text_faint))
                    .into_any_element()
            } else {
                value_row.into_any_element()
            }
        }
        HealthValue::Text { primary, secondary } => {
            let primary_el = body(primary.clone(), FONT_SM, palette.text_primary);
            if let Some(sec) = secondary {
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xs, density))
                    .child(primary_el)
                    .child(mono(sec.clone(), FONT_XS, palette.text_faint))
                    .into_any_element()
            } else {
                primary_el.into_any_element()
            }
        }
        HealthValue::Pair { left, right } => mono(
            format!("{left} \u{00b7} {right}"),
            FONT_SM,
            palette.text_primary,
        )
        .into_any_element(),
        HealthValue::Ratio {
            used,
            total,
            reset_hint,
        } => {
            let ratio_el = body(format!("{used} / {total}"), FONT_SM, palette.text_primary);
            if let Some(hint) = reset_hint {
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xs, density))
                    .child(ratio_el)
                    .child(mono(hint.clone(), FONT_XS, palette.text_faint))
                    .into_any_element()
            } else {
                ratio_el.into_any_element()
            }
        }
    }
}
