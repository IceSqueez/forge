use forge_components::{
    BORDER_THIN, Density, FONT_LG, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing,
    body_family, icon, mono_family, radius, spacing, status_dot, tr, with_alpha,
};
use forge_platform_core::{
    ActiveRow, BannerLevel, ContentList, ContentListItem, DetailSection, HealthBar, HealthLevel,
    HealthMetric, HealthValue, InfoField, KeyValueRow, ListFooter, RowAction, SectionIcon,
    StatColumn, SubscriptionRow, SubscriptionStatus, TokenColor, TrailingToken,
};
use forge_types::SubActionStep;
use gpui::{
    AnyElement, App, ClickEvent, Div, FontWeight, ListSizingBehavior, Pixels, Rgba, SharedString,
    Window, div, prelude::*, px, relative, svg, uniform_list,
};
use std::rc::Rc;

/// Keeps the scopes and subscription panels within one calm viewport; their row lists scroll
/// internally past this so Quick actions stay reachable below.
const SCROLL_PANEL_MAX_H: f32 = 320.0;
/// Estimated row height for deciding whether a list overflows its panel; short lists must not
/// occlude, or they trap the page scroll under a panel that cannot scroll itself.
const SCROLL_ROW_EST_H: f32 = 30.0;

fn panel_overflows(count: usize) -> bool {
    count as f32 * SCROLL_ROW_EST_H > SCROLL_PANEL_MAX_H
}

const PANEL_TITLE_FONT: Pixels = px(12.5);
const PANEL_BADGE_FONT: Pixels = px(9.5);
const PANEL_REFRESH_GLYPH: Pixels = px(13.0);
const ROW_GLYPH: Pixels = px(13.0);
const DEFAULT_ROW_PAD_Y: f32 = 7.0;
const ROW_NAME_FONT: Pixels = FONT_XS;
const ROW_MONO_NAME_FONT: Pixels = px(11.5);
const ROW_TRAILING_GLYPH: Pixels = px(12.0);
const ROW_TINTED_LABEL_FONT: Pixels = px(10.0);
const ROW_PAD_X: Pixels = px(14.0);
/// gpui's default line height, which rounds the resulting line box to whole pixels.
const LINE_HEIGHT_RATIO: f32 = 1.618_034;

/// Invoked by the header control a `ContentList` marks `refreshable`.
pub type SectionRefresh = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

pub struct StepClick {
    pub row: SharedString,
    pub step: SubActionStep,
}

/// Invoked by a row or trailing glyph that carries a step; the handler owns the double-fire guard.
pub type SectionStepRun = Rc<dyn Fn(&StepClick, &mut Window, &mut App)>;

pub struct SectionHooks {
    pub refresh: Option<SectionRefresh>,
    pub run_step: Option<SectionStepRun>,
}

struct RowHooks {
    scope: SharedString,
    pad_y: f32,
    run_step: Option<SectionStepRun>,
}

impl RowHooks {
    fn element_id(&self, part: &str, name: &str) -> SharedString {
        SharedString::from(format!("{}-{part}-{name}", self.scope))
    }
}

struct TokenSite<'a> {
    hooks: &'a RowHooks,
    row: &'a str,
    index: usize,
}

/// The row's tallest element is its name line box; both name fonts round to the same box, and
/// every glyph and badge in the row is shorter.
fn content_list_row_h(list: &ContentList) -> f32 {
    f32::from(list.row_padding_y_px) * 2.0 + (f32::from(ROW_NAME_FONT) * LINE_HEIGHT_RATIO).round()
}

fn content_list_region_h(list: &ContentList) -> Option<f32> {
    list.visible_rows
        .map(|rows| f32::from(rows) * content_list_row_h(list))
}

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
    div()
        .w_full()
        .flex_none()
        .h(BORDER_THIN)
        .bg(palette.border_regular)
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

pub(crate) fn grow_cell(el: impl IntoElement, grow: f32) -> Div {
    let mut cell = div().min_w(px(0.0)).child(el);
    let style = cell.style();
    style.flex_grow = Some(grow);
    style.flex_basis = Some(relative(0.0).into());
    cell
}

pub fn content_sections(
    sections: &[DetailSection],
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut col = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Lg, density));
    for section in sections {
        col = col.child(dispatch_section(section, hooks, palette, density));
    }
    col.into_any_element()
}

fn dispatch_section(
    section: &DetailSection,
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    match section {
        DetailSection::TwoColumnLists { left, right } => {
            render_two_column_lists(left, right, hooks, palette, density)
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
            banner,
        } => render_subscription_list(
            title,
            icon,
            items,
            footer.as_ref(),
            banner.as_deref(),
            palette,
            density,
        ),
        DetailSection::ScopesList {
            title,
            icon,
            scopes,
            footer,
        } => render_scopes_list(title, icon, scopes, footer.as_ref(), palette, density),
        DetailSection::TwoColumn { left, right } => {
            render_two_column(left, right, hooks, palette, density)
        }
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
        DetailSection::ChipGrid {
            title,
            icon,
            chip_icon,
            items,
        } => render_chip_grid(title, icon, chip_icon, items, palette, density),
    }
}

fn render_two_column_lists(
    left: &ContentList,
    right: &ContentList,
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
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
            content_list_panel(left, left_h, hooks, palette, density),
            10.0,
        ))
        .child(grow_cell(
            content_list_panel(right, right_h, hooks, palette, density),
            13.0,
        ))
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
            None,
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
            None,
            count.as_deref(),
            None,
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

#[allow(clippy::too_many_arguments)]
fn render_subscription_list(
    title: &str,
    icon: &SectionIcon,
    items: &[SubscriptionRow],
    footer: Option<&ListFooter>,
    banner: Option<&str>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let count = tr!("widget_builtin_active_count", count = items.len() as i64);
    let list_region: AnyElement = if panel_overflows(items.len()) {
        let owned: Vec<SubscriptionRow> = items.to_vec();
        let pal = *palette;
        uniform_list(
            "eventsub-scroll",
            owned.len(),
            move |range, _window, _cx| {
                range
                    .map(|i| subscription_row_elem(&owned[i], &pal, density))
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
            .id("eventsub-scroll")
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for item in items {
            rows = rows.child(subscription_row_elem(item, palette, density));
        }
        rows.into_any_element()
    };
    let mut card = card_shell(palette)
        .max_h(px(SCROLL_PANEL_MAX_H))
        .child(panel_header_row(
            icon.as_str(),
            title,
            None,
            Some(&count),
            None,
            palette,
            density,
        ))
        .child(divider(palette));
    if let Some(message) = banner {
        card = card.child(subscription_banner(message, palette, density));
    }
    card = card.child(list_region);
    if let Some(f) = footer {
        card = card.child(list_footer_bar(f, palette, density));
    }
    card.into_any_element()
}

fn subscription_banner(message: &str, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Md, density))
        .bg(with_alpha(palette.random, 0.10))
        .child(icon(Icon::AlertCircle, FONT_SM, palette.random))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.random)
                .child(message.to_owned()),
        )
        .into_any_element()
}

fn render_scopes_list(
    title: &str,
    icon_token: &SectionIcon,
    scopes: &[String],
    footer: Option<&ListFooter>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let count = scopes.len().to_string();
    let list_region: AnyElement = if panel_overflows(scopes.len()) {
        let owned: Vec<String> = scopes.to_vec();
        let pal = *palette;
        uniform_list("scopes-scroll", owned.len(), move |range, _window, _cx| {
            range
                .map(|i| scope_row_elem(&owned[i], &pal, density))
                .collect::<Vec<_>>()
        })
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .occlude()
        .into_any_element()
    } else {
        let mut rows = div()
            .id("scopes-scroll")
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for scope in scopes {
            rows = rows.child(scope_row_elem(scope, palette, density));
        }
        rows.into_any_element()
    };
    let mut card = card_shell(palette)
        .max_h(px(SCROLL_PANEL_MAX_H))
        .child(panel_header_row(
            icon_token.as_str(),
            title,
            None,
            Some(&count),
            None,
            palette,
            density,
        ))
        .child(divider(palette))
        .child(list_region);
    if let Some(f) = footer {
        card = card.child(list_footer_bar(f, palette, density));
    }
    card.into_any_element()
}

fn render_two_column(
    left: &DetailSection,
    right: &DetailSection,
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_stretch()
        .gap(px(12.0))
        .child(grow_cell(
            dispatch_section(left, hooks, palette, density),
            10.0,
        ))
        .child(grow_cell(
            dispatch_section(right, hooks, palette, density),
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
            None,
            None,
            palette,
            density,
        ))
        .child(divider(palette))
        .child(stats_row)
        .into_any_element()
}

const CHIP_GRID_COLUMNS: u16 = 4;
const CHIP_ICON_GLYPH: Pixels = px(11.0);
const CHIP_NAME_FONT: Pixels = px(11.5);

fn render_chip_grid(
    title: &str,
    icon_token: &SectionIcon,
    chip_icon: &SectionIcon,
    items: &[String],
    palette: &ForgePalette,
    density: Density,
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
            ))
            .into_any_element()
    } else {
        let mut grid = div()
            .w_full()
            .grid()
            .grid_cols(CHIP_GRID_COLUMNS)
            .gap(px(6.0));
        for item in items {
            grid = grid.child(chip_elem(chip_icon, item, palette));
        }
        div()
            .id(SharedString::from(format!("chip-grid-{title}")))
            .w_full()
            .max_h(px(SCROLL_PANEL_MAX_H))
            .overflow_y_scroll()
            .p(px(14.0))
            .child(grid)
            .into_any_element()
    };
    card_shell(palette)
        .child(panel_header_row(
            icon_token.as_str(),
            title,
            None,
            None,
            None,
            palette,
            density,
        ))
        .child(divider(palette))
        .child(panel_body)
        .into_any_element()
}

fn chip_elem(chip_icon: &SectionIcon, name: &str, palette: &ForgePalette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .bg(palette.shell)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .rounded(radius(Radius::Sm))
        .py(px(7.0))
        .px(px(10.0))
        .child(icon(
            Icon::from_name(chip_icon.as_str()),
            CHIP_ICON_GLYPH,
            palette.warning,
        ))
        .child(body(name.to_owned(), CHIP_NAME_FONT, palette.text_primary))
        .into_any_element()
}

fn content_list_panel(
    list: &ContentList,
    region_h: Option<f32>,
    hooks: &SectionHooks,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let row_h = content_list_row_h(list);
    let rows = RowHooks {
        scope: SharedString::from(format!("content-list-{}", list.title)),
        pad_y: f32::from(list.row_padding_y_px),
        run_step: hooks.run_step.clone(),
    };
    let mut slack = 0.0;
    let list_region: AnyElement = match region_h {
        Some(h) if list.items.len() as f32 * row_h > h => {
            let whole_rows = (h / row_h).floor().max(1.0);
            slack = h - whole_rows * row_h;
            let owned: Vec<ContentListItem> = list.items.clone();
            let pal = *palette;
            uniform_list(
                rows.scope.clone(),
                owned.len(),
                move |range, _window, _cx| {
                    range
                        .map(|i| content_list_item_row(&owned[i], &rows, &pal, density))
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
            for item in &list.items {
                stack = stack.child(content_list_item_row(item, &rows, palette, density));
            }
            stack.into_any_element()
        }
    };
    let refresh = list.refreshable.then(|| hooks.refresh.clone()).flatten();
    let mut card = card_shell(palette)
        .child(panel_header_row(
            list.icon.as_str(),
            &list.title,
            list.inline_label.as_deref(),
            list.count_label.as_deref(),
            refresh,
            palette,
            density,
        ))
        .child(divider(palette))
        .child(list_region);
    if slack > 0.0 {
        card = card.child(div().w_full().flex_none().h(px(slack)));
    }
    if let Some(f) = &list.footer {
        card = card.child(list_footer_bar(f, palette, density));
    }
    card.into_any_element()
}

fn content_list_item_row(
    item: &ContentListItem,
    rows: &RowHooks,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let dim = if item.enabled { 1.0 } else { 0.5 };
    let text_color = with_alpha(
        if !item.enabled {
            palette.text_faint
        } else if item.active {
            palette.text_primary
        } else {
            palette.text_secondary
        },
        dim,
    );
    let icon_color = with_alpha(
        match &item.icon_tint {
            Some(tint) => token_color_value(tint, palette),
            None if item.active => palette.success,
            None => palette.text_faint,
        },
        dim,
    );

    let (name_family, name_font) = if item.monospace_name {
        (mono_family(), ROW_MONO_NAME_FONT)
    } else {
        (body_family(), ROW_NAME_FONT)
    };
    let name_el = div()
        .flex_1()
        .min_w(px(0.0))
        .font_family(name_family)
        .text_size(name_font)
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
    for (index, token) in item.trailing.iter().enumerate() {
        let site = TokenSite {
            hooks: rows,
            row: &item.name,
            index,
        };
        trailing = trailing.child(trailing_token_elem(token, dim, &site, palette, density));
    }

    let content = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(icon(
            Icon::from_name(item.icon.as_str()),
            ROW_GLYPH,
            icon_color,
        ))
        .child(name_el)
        .child(trailing);

    if item.active {
        return active_row_wrapper(content.into_any_element(), rows.pad_y, palette, density);
    }
    match (&item.on_click, &rows.run_step) {
        (Some(step), Some(run)) => {
            let run = Rc::clone(run);
            let click = StepClick {
                row: SharedString::from(item.name.clone()),
                step: step.clone(),
            };
            let hover_bg = palette.surface_overlay;
            row_surface(rows.pad_y, palette)
                .id(rows.element_id("row", &item.name))
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_click(move |_: &ClickEvent, window, cx| run(&click, window, cx))
                .child(content)
                .into_any_element()
        }
        _ => plain_row_wrapper(content.into_any_element(), rows.pad_y, palette, density),
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
    plain_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
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
        active_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
    } else {
        plain_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
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
        .overflow_hidden()
        .truncate()
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
        row = row.child(mono(ver.clone(), FONT_XXS, palette.text_faint));
    }
    let trailing: AnyElement = if let Some(err) = &item.error_label {
        body(err.clone(), FONT_XXS, palette.random).into_any_element()
    } else if let Some(count) = item.event_count {
        body(
            tr!("widget_builtin_event_count", count = count as i64),
            FONT_XXS,
            palette.text_muted,
        )
        .into_any_element()
    } else {
        div().into_any_element()
    };
    row = row.child(trailing);
    plain_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
}

fn scope_row_elem(scope: &str, palette: &ForgePalette, density: Density) -> AnyElement {
    let row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(icon(Icon::CircleCheck, FONT_XS, palette.success))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(scope.to_owned()),
        );
    plain_row_wrapper(row.into_any_element(), DEFAULT_ROW_PAD_Y, palette, density)
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
        "rss" | "layout-grid" => palette.brand,
        "stack-2" => palette.info,
        "mood-smile" => palette.accent_pink_light,
        _ => palette.text_secondary,
    }
}

fn panel_header_row(
    icon_str: &str,
    title: &str,
    inline_label: Option<&str>,
    count: Option<&str>,
    refresh: Option<SectionRefresh>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let mut left = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .child(icon(
            Icon::from_name(icon_str),
            FONT_SM,
            panel_header_icon_color(icon_str, palette),
        ))
        .child(
            body(title.to_owned(), PANEL_TITLE_FONT, palette.text_primary)
                .font_weight(FontWeight::MEDIUM),
        );
    if let Some(label) = inline_label {
        left = left.child(body(label.to_owned(), PANEL_TITLE_FONT, palette.text_faint));
    }

    let mut row = div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .py(spacing(Spacing::Sm, density))
        .px(px(14.0))
        .child(left);
    if let Some(c) = count {
        row = row.child(mono(c.to_owned(), FONT_XXS, palette.text_faint));
    }
    if let Some(handler) = refresh {
        row = row.child(
            div()
                .id(SharedString::from(format!("panel-refresh-{title}")))
                .flex_none()
                .cursor_pointer()
                .child(icon(Icon::Refresh, PANEL_REFRESH_GLYPH, palette.text_faint))
                .on_click(move |event, window, cx| handler(event, window, cx)),
        );
    }
    row.into_any_element()
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
        .flex_none()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .py(px(8.0))
        .px(px(14.0))
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

fn active_row_wrapper(
    content: AnyElement,
    pad_y: f32,
    palette: &ForgePalette,
    _density: Density,
) -> AnyElement {
    let padded = div()
        .flex_1()
        .min_w(px(0.0))
        .py(px(pad_y))
        .px(ROW_PAD_X)
        .child(content);
    div()
        .w_full()
        .flex()
        .bg(palette.shell)
        .child(div().flex_none().w(px(2.0)).bg(palette.success))
        .child(padded)
        .into_any_element()
}

fn row_surface(pad_y: f32, palette: &ForgePalette) -> Div {
    div()
        .w_full()
        .py(px(pad_y))
        .px(ROW_PAD_X)
        .bg(palette.elevated)
}

fn plain_row_wrapper(
    content: AnyElement,
    pad_y: f32,
    palette: &ForgePalette,
    _density: Density,
) -> AnyElement {
    row_surface(pad_y, palette)
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
        .child(body(
            label.to_uppercase(),
            PANEL_BADGE_FONT,
            palette.success,
        ))
        .into_any_element()
}

fn trailing_token_elem(
    token: &TrailingToken,
    dim: f32,
    site: &TokenSite,
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
                .child(body(label.clone(), PANEL_BADGE_FONT, tc))
                .into_any_element()
        }
        TrailingToken::Icon(ic, color) => {
            let tc = with_alpha(token_color_value(color, palette), dim);
            icon(Icon::from_name(ic.as_str()), ROW_TRAILING_GLYPH, tc).into_any_element()
        }
        TrailingToken::Label(label) => {
            mono(label.clone(), FONT_XXS, with_alpha(palette.text_faint, dim)).into_any_element()
        }
        TrailingToken::TintedLabel(label, color) => mono(
            label.clone(),
            ROW_TINTED_LABEL_FONT,
            with_alpha(token_color_value(color, palette), dim),
        )
        .into_any_element(),
        TrailingToken::ActionIcon {
            icon: ic,
            tint,
            step,
        } => {
            let tc = with_alpha(token_color_value(tint, palette), dim);
            let glyph = Icon::from_name(ic.as_str());
            let Some(run) = site.hooks.run_step.clone() else {
                return icon(glyph, ROW_TRAILING_GLYPH, tc).into_any_element();
            };
            let hover_tint = palette.text_primary;
            let click = StepClick {
                row: SharedString::from(site.row.to_owned()),
                step: step.clone(),
            };
            svg()
                .flex_none()
                .size(ROW_TRAILING_GLYPH)
                .path(glyph.path())
                .text_color(tc)
                .id(site
                    .hooks
                    .element_id(&format!("token-{}", site.index), site.row))
                .cursor_pointer()
                .hover(move |s| s.text_color(hover_tint))
                .on_click(move |_: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    run(&click, window, cx);
                })
                .into_any_element()
        }
    }
}

fn token_color_value(color: &TokenColor, palette: &ForgePalette) -> Rgba {
    match color {
        TokenColor::Green => palette.success,
        TokenColor::Yellow => palette.warning,
        TokenColor::Red => palette.random,
        TokenColor::Muted => palette.text_faint,
        TokenColor::Accent => palette.brand,
        TokenColor::Subtle => palette.text_muted,
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
        .gap(px(4.0))
        .py(spacing(Spacing::Sm, density))
        .px(px(12.0))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
        .child(mono(
            metric.label.to_uppercase(),
            FONT_XXS,
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
                    .gap(px(2.0))
                    .child(value_row)
                    .child(mono(d.clone(), FONT_XXS, palette.text_faint))
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
                    .gap(px(2.0))
                    .child(primary_el)
                    .child(mono(sec.clone(), FONT_XXS, palette.text_faint))
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
                    .gap(px(2.0))
                    .child(ratio_el)
                    .child(mono(hint.clone(), FONT_XXS, palette.text_faint))
                    .into_any_element()
            } else {
                ratio_el.into_any_element()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use forge_components::FORGE_DEFAULT;
    use gpui::{Modifiers, TestAppContext, VisualTestContext, point, size};

    use super::*;

    type Fired = Rc<RefCell<Vec<Vec<(String, String)>>>>;

    const WINDOW_W: f32 = 720.0;
    const WINDOW_H: f32 = 360.0;
    const SCAN_STEP: f32 = 4.0;

    fn step(kind_id: &str) -> SubActionStep {
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config: Default::default(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    fn row(name: &str, active: bool, on_click: Option<&str>) -> ContentListItem {
        ContentListItem {
            icon: SectionIcon::new("film"),
            icon_tint: None,
            name: name.to_owned(),
            monospace_name: false,
            active,
            active_label: None,
            trailing: Vec::new(),
            enabled: true,
            on_click: on_click.map(step),
        }
    }

    fn with_action_glyph(mut item: ContentListItem, kind_id: &str) -> ContentListItem {
        item.trailing.push(TrailingToken::ActionIcon {
            icon: SectionIcon::new("eye"),
            tint: TokenColor::Green,
            step: step(kind_id),
        });
        item
    }

    fn list(title: &str, items: Vec<ContentListItem>) -> ContentList {
        ContentList {
            title: title.to_owned(),
            icon: SectionIcon::new("film"),
            inline_label: None,
            count_label: None,
            visible_rows: None,
            row_padding_y_px: 7,
            refreshable: false,
            items,
            footer: None,
        }
    }

    struct Host {
        sections: Vec<DetailSection>,
        fired: Fired,
    }

    impl Render for Host {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let fired = Rc::clone(&self.fired);
            let run_step: SectionStepRun = Rc::new(move |click: &StepClick, _, _| {
                if let Some(current) = fired.borrow_mut().last_mut() {
                    current.push((click.row.to_string(), click.step.kind_id.clone()));
                }
            });
            let hooks = SectionHooks {
                refresh: None,
                run_step: Some(run_step),
            };
            div().size_full().child(content_sections(
                &self.sections,
                &hooks,
                &FORGE_DEFAULT,
                Density::default(),
            ))
        }
    }

    /// Clicks every point of a grid over the window and returns what each click fired.
    fn click_everywhere(left: ContentList, right: ContentList, cx: &mut TestAppContext) -> Fired {
        let fired: Fired = Rc::default();
        let host_fired = Rc::clone(&fired);
        let (_host, vcx) = cx.add_window_view(|_window, _cx| Host {
            sections: vec![DetailSection::TwoColumnLists {
                left: Box::new(left),
                right: Box::new(right),
            }],
            fired: host_fired,
        });
        vcx.simulate_resize(size(px(WINDOW_W), px(WINDOW_H)));
        vcx.run_until_parked();
        scan(vcx, &fired);
        fired
    }

    fn scan(vcx: &mut VisualTestContext, fired: &Fired) {
        let mut y = 0.0;
        while y < WINDOW_H {
            let mut x = 0.0;
            while x < WINDOW_W {
                fired.borrow_mut().push(Vec::new());
                vcx.simulate_click(point(px(x), px(y)), Modifiers::none());
                x += SCAN_STEP;
            }
            y += SCAN_STEP;
        }
    }

    fn distinct(fired: &Fired) -> Vec<(String, String)> {
        let mut all: Vec<(String, String)> = fired.borrow().iter().flatten().cloned().collect();
        all.sort();
        all.dedup();
        all
    }

    #[gpui::test]
    fn only_rows_and_glyphs_that_carry_a_step_run_it_on_click(cx: &mut TestAppContext) {
        let fired = click_everywhere(
            list(
                "Scenes",
                vec![
                    row("Live", true, Some("obs.scene.set_current")),
                    row("Intro", false, Some("obs.scene.set_current")),
                    row("Plain", false, None),
                ],
            ),
            list(
                "Sources",
                vec![with_action_glyph(
                    row("Mic", false, None),
                    "obs.source.set_visibility",
                )],
            ),
            cx,
        );

        assert_eq!(
            distinct(&fired),
            vec![
                ("Intro".to_owned(), "obs.scene.set_current".to_owned()),
                ("Mic".to_owned(), "obs.source.set_visibility".to_owned()),
            ]
        );
    }

    #[gpui::test]
    fn a_glyph_click_inside_an_actionable_row_runs_only_the_glyph_step(cx: &mut TestAppContext) {
        let fired = click_everywhere(
            list(
                "Scenes",
                vec![with_action_glyph(
                    row("Cam", false, Some("obs.scene.set_current")),
                    "obs.source.set_visibility",
                )],
            ),
            list("Sources", Vec::new()),
            cx,
        );

        assert!(
            distinct(&fired).contains(&("Cam".to_owned(), "obs.source.set_visibility".to_owned())),
            "the glyph never fired"
        );
        let doubled: Vec<Vec<(String, String)>> = fired
            .borrow()
            .iter()
            .filter(|click| click.len() > 1)
            .cloned()
            .collect();
        assert!(
            doubled.is_empty(),
            "one click ran several steps: {doubled:?}"
        );
    }

    #[gpui::test]
    fn equal_row_names_in_two_lists_each_run_their_own_step(cx: &mut TestAppContext) {
        let fired = click_everywhere(
            list(
                "Scenes",
                vec![row("Mic", false, Some("obs.scene.set_current"))],
            ),
            list(
                "Sources",
                vec![row("Mic", false, Some("obs.source.set_visibility"))],
            ),
            cx,
        );

        assert_eq!(
            distinct(&fired),
            vec![
                ("Mic".to_owned(), "obs.scene.set_current".to_owned()),
                ("Mic".to_owned(), "obs.source.set_visibility".to_owned()),
            ]
        );
    }

    #[gpui::test]
    fn two_glyphs_on_one_row_each_run_their_own_step(cx: &mut TestAppContext) {
        let source = with_action_glyph(
            with_action_glyph(row("Mic", false, None), "obs.source.set_visibility"),
            "obs.source.set_lock",
        );
        let fired = click_everywhere(
            list("Sources", vec![source]),
            list("Scenes", Vec::new()),
            cx,
        );

        assert_eq!(
            distinct(&fired),
            vec![
                ("Mic".to_owned(), "obs.source.set_lock".to_owned()),
                ("Mic".to_owned(), "obs.source.set_visibility".to_owned()),
            ]
        );
    }
}
