use std::sync::Arc;

use forge_components::FONT_LABEL;
use forge_components::FONT_MICRO;
use forge_components::FONT_ROW;
use forge_components::FONT_SECTION_LABEL;
use forge_components::TINT_FILL_ALPHA;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_LG, FONT_SM, FONT_XS, FONT_XXS, ForgePalette,
    Icon, KeyActivated, Radius, Spacing, card_shell, divider, grow_cell, icon, radius, scaled,
    spacing, status_dot, tr, with_alpha,
};
use forge_platform_core::RowSpacing;
use forge_platform_core::{
    ActiveRow, BannerLevel, ContentList, ContentListItem, DetailSection, HealthBar, HealthLevel,
    HealthMetric, HealthValue, InfoField, KeyValueRow, ListFooter, RowAction, SectionIcon,
    StatColumn, SubscriptionRow, SubscriptionStatus, Tone, TrailingToken,
};
use forge_types::SubActionStep;

use crate::integrations::health_text::{health_metric_label, health_text};
use crate::shared::user_text::user_text_string;
use forge_components::OverflowRevealingFocus;
use gpui::{
    AnyElement, App, ClickEvent, Div, FontWeight, ListSizingBehavior, Pixels, Rgba, SharedString,
    Window, div, prelude::*, px, relative, svg, uniform_list,
};
use std::rc::Rc;

mod health;
mod panels;
mod rows;

pub(crate) use health::*;
pub(crate) use panels::*;
use rows::*;

const METER_RADIUS: Pixels = px(5.0);
const PILL_RADIUS: Pixels = px(8.0);
const LIVE_DOT: Pixels = px(5.0);
const ROW_DOT: Pixels = px(7.0);
const SUBSCRIPTION_DOT: Pixels = px(6.0);
const METER_H: Pixels = px(6.0);
const ACCENT_EDGE_W: Pixels = px(2.0);
const HEALTH_VALUE_HINT_GAP: Pixels = px(2.0);
const STATUS_BADGE_GAP: Pixels = px(5.0);
const PANEL_TITLE_GAP: Pixels = px(7.0);
const PANEL_SPLIT_GAP: Pixels = px(12.0);
const SCROLL_PANEL_PAD: Pixels = px(14.0);
const BADGE_PAD_V: Pixels = px(0.0);
const CHIP_PAD_V: Pixels = px(7.0);
const LIST_ROW_PAD_V: Pixels = px(8.0);
const HEALTH_TILE_PAD_H: Pixels = px(12.0);
const PANEL_ROW_PAD_H: Pixels = px(14.0);

const SCROLL_PANEL_MAX_H: f32 = 320.0;
const SCROLL_ROW_EST_H: f32 = 30.0;

fn panel_overflows(count: usize) -> bool {
    count as f32 * SCROLL_ROW_EST_H > SCROLL_PANEL_MAX_H
}

const PANEL_TITLE_FONT: Pixels = FONT_ROW;
const PANEL_BADGE_FONT: Pixels = FONT_SECTION_LABEL;
const PANEL_REFRESH_GLYPH: Pixels = px(13.0);
const ROW_GLYPH: Pixels = px(13.0);
const STANDARD_ROW_PAD_Y: f32 = 7.0;
const RELAXED_ROW_PAD_Y: f32 = 8.0;
const ROW_NAME_FONT: Pixels = FONT_XS;
const ROW_MONO_NAME_FONT: Pixels = FONT_LABEL;
const ROW_TRAILING_GLYPH: Pixels = px(12.0);
const ROW_TINTED_LABEL_FONT: Pixels = FONT_MICRO;
const ROW_PAD_X: Pixels = px(14.0);
const LINE_HEIGHT_RATIO: f32 = std::f32::consts::GOLDEN_RATIO;

pub(crate) type SectionRefresh = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

pub(crate) struct StepClick {
    pub row: SharedString,
    pub step: SubActionStep,
}

pub(crate) type SectionStepRun = Rc<dyn Fn(&StepClick, &mut Window, &mut App)>;

pub(crate) struct SectionHooks {
    pub refresh: Option<SectionRefresh>,
    pub run_step: Option<SectionStepRun>,
}

struct RowHooks {
    scope: SharedString,
    pad_y: f32,
    run_step: Option<SectionStepRun>,
}

impl RowHooks {
    fn element_id(&self, part: &str, row_index: usize) -> SharedString {
        SharedString::from(format!("{}-{part}-{row_index}", self.scope))
    }
}

struct TokenSite<'a> {
    hooks: &'a RowHooks,
    row: &'a str,
    row_index: usize,
    index: usize,
}

fn row_pad_y(list: &ContentList) -> f32 {
    match list.row_spacing {
        RowSpacing::Standard => STANDARD_ROW_PAD_Y,
        RowSpacing::Relaxed => RELAXED_ROW_PAD_Y,
    }
}

fn content_list_scope(list: &ContentList) -> SharedString {
    SharedString::from(format!("content-list-{}", list.title.code_or_text()))
}

fn content_list_row_h(list: &ContentList) -> f32 {
    row_pad_y(list) * 2.0 + (f32::from(ROW_NAME_FONT) * LINE_HEIGHT_RATIO).round()
}

fn content_list_region_h(list: &ContentList) -> Option<f32> {
    list.visible_rows
        .map(|rows| f32::from(rows) * content_list_row_h(list))
}

fn mono(s: impl Into<SharedString>, size: gpui::Pixels, color: Rgba, cx: &App) -> Div {
    div()
        .font_family(cx.mono_family())
        .text_size(size)
        .text_color(color)
        .child(s.into())
}

fn body(s: impl Into<SharedString>, size: gpui::Pixels, color: Rgba, cx: &App) -> Div {
    div()
        .font_family(cx.body_family())
        .text_size(size)
        .text_color(color)
        .child(s.into())
}

pub(crate) fn content_sections(
    sections: &[DetailSection],
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let mut col = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Lg, density));
    for section in sections {
        col = col.child(dispatch_section(section, hooks, palette, density, cx));
    }
    col.into_any_element()
}

fn dispatch_section(
    section: &DetailSection,
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    match section {
        DetailSection::TwoColumnLists { left, right } => {
            render_two_column_lists(left, right, hooks, palette, density, cx)
        }
        DetailSection::KeyValueList {
            title,
            icon,
            icon_tint,
            items,
        } => render_key_value_list(
            &user_text_string(title),
            PanelIcon::of(icon, icon_tint),
            items,
            palette,
            density,
            cx,
        ),
        DetailSection::ActiveItemList {
            title,
            icon,
            icon_tint,
            items,
        } => render_active_item_list(
            &user_text_string(title),
            PanelIcon::of(icon, icon_tint),
            items,
            palette,
            density,
            cx,
        ),
        DetailSection::WarningBanner {
            level,
            title,
            body,
            cta,
        } => render_warning_banner(
            level,
            &user_text_string(title),
            &user_text_string(body),
            cta.as_ref().map(user_text_string).as_deref(),
            palette,
            density,
            cx,
        ),
        DetailSection::SubscriptionList {
            title,
            icon,
            icon_tint,
            items,
            footer,
            banner,
        } => render_subscription_list(
            &user_text_string(title),
            PanelIcon::of(icon, icon_tint),
            items,
            footer.as_ref(),
            banner.as_ref().map(user_text_string).as_deref(),
            density,
            cx,
        ),
        DetailSection::ScopesList {
            title,
            icon,
            icon_tint,
            scopes,
            footer,
        } => render_scopes_list(
            &user_text_string(title),
            PanelIcon::of(icon, icon_tint),
            scopes,
            footer.as_ref(),
            palette,
            density,
            cx,
        ),
        DetailSection::TwoColumn { left, right } => {
            render_two_column(left, right, hooks, palette, density, cx)
        }
        DetailSection::InfoCard {
            title,
            live,
            fields,
            health_bar,
        } => render_info_card(
            &user_text_string(title),
            *live,
            fields,
            health_bar.as_ref(),
            palette,
            density,
            cx,
        ),
        DetailSection::StatsGrid {
            title,
            icon,
            icon_tint,
            columns,
        } => render_stats_grid(
            &user_text_string(title),
            PanelIcon::of(icon, icon_tint),
            columns,
            palette,
            density,
            cx,
        ),
        DetailSection::ChipGrid {
            title,
            icon,
            icon_tint,
            chip_icon,
            items,
        } => render_chip_grid(
            &user_text_string(title),
            PanelIcon::of(icon, icon_tint),
            chip_icon,
            items,
            palette,
            density,
            cx,
        ),
    }
}

fn render_two_column_lists(
    left: &ContentList,
    right: &ContentList,
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let (left_h, right_h) = match (content_list_region_h(left), content_list_region_h(right)) {
        (Some(a), Some(b)) => (Some(a.max(b)), Some(a.max(b))),
        pair => pair,
    };
    div()
        .w_full()
        .flex()
        .gap(spacing(Spacing::Md, density))
        .child(grow_cell(
            content_list_panel(left, left_h, hooks, palette, density, cx),
            10.0,
        ))
        .child(grow_cell(
            content_list_panel(right, right_h, hooks, palette, density, cx),
            13.0,
        ))
        .into_any_element()
}

fn render_key_value_list(
    title: &str,
    icon: PanelIcon<'_>,
    items: &[KeyValueRow],
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let mut rows = div().w_full().flex().flex_col();
    for item in items {
        rows = rows.child(key_value_row_elem(item, palette, density, cx));
    }
    card_shell(palette)
        .child(panel_header_row(icon, title, None, None, None, density, cx))
        .child(divider(palette))
        .child(rows)
        .into_any_element()
}

fn render_active_item_list(
    title: &str,
    icon: PanelIcon<'_>,
    items: &[ActiveRow],
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let count = if items.is_empty() {
        None
    } else {
        Some(items.len().to_string())
    };
    let mut rows = div().w_full().flex().flex_col();
    for item in items {
        rows = rows.child(active_item_row_elem(item, palette, density, cx));
    }
    card_shell(palette)
        .child(panel_header_row(
            icon,
            title,
            None,
            count.as_deref(),
            None,
            density,
            cx,
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
    cx: &App,
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
        .child(body(title.to_owned(), FONT_SM, palette.text_primary, cx))
        .child(body(
            banner_body.to_owned(),
            FONT_SM,
            palette.text_muted,
            cx,
        ));
    if let Some(cta_label) = cta {
        text_col = text_col.child(body(
            format!("{cta_label} \u{2192}"),
            FONT_SM,
            palette.brand,
            cx,
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
        .child(body(glyph, FONT_SM, level_color, cx))
        .child(text_col)
        .into_any_element()
}

fn render_subscription_list(
    title: &str,
    icon: PanelIcon<'_>,
    items: &Arc<[SubscriptionRow]>,
    footer: Option<&ListFooter>,
    banner: Option<&str>,
    density: Density,
    cx: &App,
) -> AnyElement {
    let palette = &cx.palette();
    let count = tr!(
        "widget_builtin_active_count",
        count = i64::try_from(items.len()).unwrap_or(i64::MAX)
    );
    let list_region: AnyElement = if panel_overflows(items.len()) {
        let owned = Arc::clone(items);
        let pal = *palette;
        uniform_list(
            SharedString::from(format!("eventsub-scroll-{title}")),
            owned.len(),
            move |range, _window, cx| {
                range
                    .map(|i| subscription_row_elem(&owned[i], &pal, density, cx))
                    .collect::<Vec<_>>()
            },
        )
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .occlude()
        .into_any_element()
    } else {
        let mut rows = div()
            .id(SharedString::from(format!("eventsub-scroll-{title}")))
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_revealing_focus()
            .flex()
            .flex_col();
        for item in items.iter() {
            rows = rows.child(subscription_row_elem(item, palette, density, cx));
        }
        rows.into_any_element()
    };
    let mut card = card_shell(palette)
        .max_h(px(SCROLL_PANEL_MAX_H))
        .child(panel_header_row(
            icon,
            title,
            None,
            Some(&count),
            None,
            density,
            cx,
        ))
        .child(divider(palette));
    if let Some(message) = banner {
        card = card.child(subscription_banner(message, palette, density, cx));
    }
    card = card.child(list_region);
    if let Some(f) = footer {
        card = card.child(list_footer_bar(f, palette, density, cx));
    }
    card.into_any_element()
}

fn subscription_banner(
    message: &str,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Md, density))
        .bg(with_alpha(palette.danger, TINT_FILL_ALPHA))
        .child(icon(Icon::AlertCircle, FONT_SM, palette.danger))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(cx.mono_family())
                .text_size(FONT_XS)
                .text_color(palette.danger)
                .child(message.to_owned()),
        )
        .into_any_element()
}

fn render_scopes_list(
    title: &str,
    icon_token: PanelIcon<'_>,
    scopes: &Arc<[String]>,
    footer: Option<&ListFooter>,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let count = scopes.len().to_string();
    let list_region: AnyElement = if panel_overflows(scopes.len()) {
        let owned = Arc::clone(scopes);
        let pal = *palette;
        uniform_list(
            SharedString::from(format!("scopes-scroll-{title}")),
            owned.len(),
            move |range, _window, cx| {
                range
                    .map(|i| scope_row_elem(&owned[i], &pal, density, cx))
                    .collect::<Vec<_>>()
            },
        )
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .occlude()
        .into_any_element()
    } else {
        let mut rows = div()
            .id(SharedString::from(format!("scopes-scroll-{title}")))
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_revealing_focus()
            .flex()
            .flex_col();
        for scope in scopes.iter() {
            rows = rows.child(scope_row_elem(scope, palette, density, cx));
        }
        rows.into_any_element()
    };
    let mut card = card_shell(palette)
        .max_h(px(SCROLL_PANEL_MAX_H))
        .child(panel_header_row(
            icon_token,
            title,
            None,
            Some(&count),
            None,
            density,
            cx,
        ))
        .child(divider(palette))
        .child(list_region);
    if let Some(f) = footer {
        card = card.child(list_footer_bar(f, palette, density, cx));
    }
    card.into_any_element()
}

fn render_two_column(
    left: &DetailSection,
    right: &DetailSection,
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_stretch()
        .gap(scaled(PANEL_SPLIT_GAP, density))
        .child(grow_cell(
            dispatch_section(left, hooks, palette, density, cx),
            10.0,
        ))
        .child(grow_cell(
            dispatch_section(right, hooks, palette, density, cx),
            13.0,
        ))
        .into_any_element()
}

fn render_info_card(
    title: &str,
    live: bool,
    fields: &[InfoField],
    health_bar: Option<&HealthBar>,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let mut fields_grid = div().flex().flex_col().gap(spacing(Spacing::Md, density));
    for chunk in fields.chunks(2) {
        let mut row = div().flex().gap(spacing(Spacing::Md, density));
        for field in chunk {
            row = row.child(info_field_cell(field, palette, density, cx));
        }
        fields_grid = fields_grid.child(row);
    }

    let mut content_col = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Md, density))
        .child(fields_grid);
    if let Some(bar) = health_bar {
        content_col = content_col.child(health_bar_section(bar, palette, density, cx));
    }

    let content_padded = div()
        .w_full()
        .py(spacing(Spacing::Md, density))
        .px(spacing(Spacing::Md, density))
        .child(content_col);

    card_shell(palette)
        .child(info_card_header(title, live, palette, density, cx))
        .child(divider(palette))
        .child(content_padded)
        .into_any_element()
}

fn render_stats_grid(
    title: &str,
    icon: PanelIcon<'_>,
    columns: &[StatColumn],
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let mut stats_row = div().w_full().flex();
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            stats_row =
                stats_row.child(div().flex_none().w(BORDER_THIN).bg(palette.border_regular));
        }
        stats_row = stats_row.child(stat_column_cell(col, palette, density, cx));
    }
    card_shell(palette)
        .child(panel_header_row(icon, title, None, None, None, density, cx))
        .child(divider(palette))
        .child(stats_row)
        .into_any_element()
}

const CHIP_GRID_COLUMNS: u16 = 4;
const CHIP_ICON_GLYPH: Pixels = px(11.0);
const CHIP_NAME_FONT: Pixels = FONT_LABEL;

fn render_chip_grid(
    title: &str,
    icon_token: PanelIcon<'_>,
    chip_icon: &SectionIcon,
    items: &[String],
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let panel_body: AnyElement = if items.is_empty() {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Lg, density))
            .child(body(
                tr!("widget_builtin_chip_grid_empty"),
                FONT_XS,
                palette.text_secondary,
                cx,
            ))
            .into_any_element()
    } else {
        let mut grid = div()
            .w_full()
            .grid()
            .grid_cols(CHIP_GRID_COLUMNS)
            .gap(spacing(Spacing::Xs, density));
        for item in items {
            grid = grid.child(chip_elem(chip_icon, item, palette, density, cx));
        }
        div()
            .id(SharedString::from(format!("chip-grid-{title}")))
            .w_full()
            .max_h(px(SCROLL_PANEL_MAX_H))
            .overflow_y_revealing_focus()
            .p(scaled(SCROLL_PANEL_PAD, density))
            .child(grid)
            .into_any_element()
    };
    card_shell(palette)
        .child(panel_header_row(
            icon_token, title, None, None, None, density, cx,
        ))
        .child(divider(palette))
        .child(panel_body)
        .into_any_element()
}

fn chip_elem(
    chip_icon: &SectionIcon,
    name: &str,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .bg(palette.shell)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .rounded(radius(Radius::Sm))
        .py(scaled(CHIP_PAD_V, density))
        .px(spacing(Spacing::Sm, density))
        .child(icon(
            Icon::from_name(chip_icon.as_str()),
            CHIP_ICON_GLYPH,
            palette.warning,
        ))
        .child(body(
            name.to_owned(),
            CHIP_NAME_FONT,
            palette.text_primary,
            cx,
        ))
        .into_any_element()
}

fn content_list_panel(
    list: &ContentList,
    region_h: Option<f32>,
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let row_h = content_list_row_h(list);
    let rows = RowHooks {
        scope: content_list_scope(list),
        pad_y: row_pad_y(list),
        run_step: hooks.run_step.clone(),
    };
    let mut slack = 0.0;
    let list_region: AnyElement = match region_h {
        Some(h) if list.items.len() as f32 * row_h > h => {
            let whole_rows = (h / row_h).floor().max(1.0);
            slack = h - whole_rows * row_h;
            let owned = Arc::clone(&list.items);
            let pal = *palette;
            uniform_list(
                rows.scope.clone(),
                owned.len(),
                move |range, _window, cx| {
                    range
                        .map(|i| content_list_item_row(&owned[i], i, &rows, &pal, density, cx))
                        .collect::<Vec<_>>()
                },
            )
            .w_full()
            .flex_none()
            .h(px(whole_rows * row_h))
            .occlude()
            .into_any_element()
        }
        pinned_h => {
            let mut stack = div().w_full().flex().flex_col();
            if let Some(h) = pinned_h {
                stack = stack.flex_none().h(px(h));
            }
            for (row_index, item) in list.items.iter().enumerate() {
                stack = stack.child(content_list_item_row(
                    item, row_index, &rows, palette, density, cx,
                ));
            }
            stack.into_any_element()
        }
    };
    let refresh = list.refreshable.then(|| hooks.refresh.clone()).flatten();
    let mut card = card_shell(palette)
        .child(panel_header_row(
            PanelIcon::of(&list.icon, &list.icon_tint),
            &user_text_string(&list.title),
            list.inline_label.as_ref().map(user_text_string).as_deref(),
            list.count_label.as_ref().map(user_text_string).as_deref(),
            refresh,
            density,
            cx,
        ))
        .child(divider(palette))
        .child(list_region);
    if slack > 0.0 {
        card = card.child(div().w_full().flex_none().h(px(slack)));
    }
    if let Some(f) = &list.footer {
        card = card.child(list_footer_bar(f, palette, density, cx));
    }
    card.into_any_element()
}

fn info_field_cell(
    field: &InfoField,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let value_family = if field.value_is_identifier {
        cx.mono_family()
    } else {
        cx.body_family()
    };
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(mono(
            user_text_string(&field.label).to_uppercase(),
            FONT_XS,
            palette.text_muted,
            cx,
        ))
        .child(
            div()
                .font_family(value_family)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(user_text_string(&field.value)),
        )
        .into_any_element()
}

fn stat_column_cell(
    col: &StatColumn,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density))
        .child(mono(
            user_text_string(&col.label).to_uppercase(),
            FONT_XS,
            palette.text_muted,
            cx,
        ))
        .child(mono(
            user_text_string(&col.value),
            FONT_LG,
            palette.text_primary,
            cx,
        ))
        .child(mono(
            user_text_string(&col.subtitle),
            FONT_XS,
            palette.success,
            cx,
        ))
        .into_any_element()
}

fn token_color_value(color: &Tone, palette: &ForgePalette) -> Rgba {
    match color {
        Tone::Positive => palette.success,
        Tone::Caution => palette.warning,
        Tone::Critical => palette.danger,
        Tone::Muted => palette.text_faint,
        Tone::Accent => palette.brand,
        Tone::Subtle => palette.text_muted,
        Tone::Info => palette.info,
        Tone::ThemeBrand => palette.theme_brand,
        Tone::Highlight => palette.accent_pink_light,
    }
}

fn subscription_status_color(status: &SubscriptionStatus, palette: &ForgePalette) -> Rgba {
    match status {
        SubscriptionStatus::Active => palette.success,
        SubscriptionStatus::Degraded => palette.warning,
        SubscriptionStatus::Error => palette.danger,
    }
}

fn banner_level_color(level: &BannerLevel, palette: &ForgePalette) -> Rgba {
    match level {
        BannerLevel::Warning => palette.warning,
        BannerLevel::Info => palette.info,
        BannerLevel::Error => palette.danger,
    }
}

fn health_level_color(level: &HealthLevel, palette: &ForgePalette) -> Rgba {
    match level {
        HealthLevel::Good => palette.success,
        HealthLevel::Ok => palette.warning,
        HealthLevel::Bad => palette.danger,
        HealthLevel::NoData => palette.disabled,
    }
}

#[cfg(test)]
mod tests;
