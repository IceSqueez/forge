use std::collections::HashSet;

use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, EventEmitter, FontWeight,
    ListAlignment, ListState, MouseButton, MouseDownEvent, Pixels, Rgba, SharedString,
    Subscription, Window, div, list, prelude::*, px, relative,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::status::badge;
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{
    BORDER_ACCENT, Density, FONT_SM, FONT_XS, FONT_XXS, Radius, Spacing, body_family, mono_family,
    radius, spacing,
};

const GRID_W: Pixels = px(880.0);
const GRID_H: Pixels = px(640.0);
const GRID_BAND_PAD_H: Pixels = px(16.0);
const GRID_TILE: Pixels = px(30.0);
const GRID_TILE_RADIUS: Pixels = px(7.0);
const GRID_TILE_ICON: Pixels = px(15.0);
const GRID_HEADER_GAP: Pixels = px(11.0);
const GRID_HEADER_PAD_V: Pixels = px(13.0);
const GRID_CLOSE_ICON: Pixels = px(15.0);
const GRID_SEARCH_PAD_T: Pixels = px(11.0);
const GRID_SEARCH_PAD_B: Pixels = px(9.0);
const GRID_SEARCH_ICON: Pixels = px(14.0);
const GRID_SEARCH_FS: Pixels = px(13.0);
const RAIL_W: Pixels = px(184.0);
const RAIL_PAD_V: Pixels = px(10.0);
const RAIL_PAD_H: Pixels = px(10.0);
const RAIL_GAP: Pixels = px(2.0);
const RAIL_ENTRY_GAP: Pixels = px(8.0);
const RAIL_ENTRY_PAD_V: Pixels = px(6.0);
const RAIL_ENTRY_PAD_H: Pixels = px(8.0);
const RAIL_LEAD_SLOT: Pixels = px(14.0);
const RAIL_DOT: Pixels = px(6.0);
const RAIL_STAR: Pixels = px(12.0);
const GRID_BODY_PAD_V: Pixels = px(13.0);
const GRID_GROUP_FS: Pixels = px(9.5);
const GRID_GROUP_DOT: Pixels = px(5.0);
const GRID_CARD_GAP: Pixels = px(8.0);
const GRID_CARD_PAD_V: Pixels = px(11.0);
const GRID_CARD_PAD_H: Pixels = px(12.0);
const GRID_CARD_TILE: Pixels = px(26.0);
const GRID_CARD_TILE_RADIUS: Pixels = px(7.0);
const GRID_CARD_ICON: Pixels = px(13.0);
const GRID_CARD_NAME_FS: Pixels = px(12.5);
const GRID_CARD_ROW_MB: Pixels = px(6.0);
const GRID_META_FS: Pixels = px(11.0);
const GRID_FOOTER_PAD_V: Pixels = px(8.0);
const GRID_EMPTY_PAD_V: Pixels = px(50.0);
const GRID_EMPTY_GLYPH: Pixels = px(22.0);
const GRID_BADGE_FS: Pixels = px(9.0);

const GRID_CARD_H: Pixels = px(72.0);
const GRID_ROW_H: Pixels = px(80.0);
const GRID_HEADER_ROW_H: Pixels = px(30.0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GridPickerItemState {
    Normal,
    Added,
    Disabled,
}

pub struct GridPickerItem {
    pub id: SharedString,
    pub icon: Icon,
    pub icon_color: Rgba,
    pub name: SharedString,
    pub desc: SharedString,
    pub state: GridPickerItemState,
}

pub struct GridPickerGroup {
    pub label: SharedString,
    pub dot_color: Rgba,
    pub scope: SharedString,
    pub items: Vec<GridPickerItem>,
}

pub enum GridPickerSubtitle {
    Plain(SharedString),
    Context {
        lead: SharedString,
        name: SharedString,
        note: SharedString,
    },
}

pub struct GridPickerConfig {
    pub accent: Rgba,
    pub header_icon: Icon,
    pub title: SharedString,
    pub subtitle: GridPickerSubtitle,
    pub footer_hint: SharedString,
    pub search_placeholder: SharedString,
    pub favorites_label: SharedString,
    pub favorites_empty: SharedString,
}

#[derive(Debug, Clone)]
pub enum GridPickerEvent {
    Picked(SharedString),
    FavoriteToggled(SharedString),
    Dismissed,
}

#[derive(Clone, PartialEq)]
enum RailSel {
    All,
    Favorites,
    Group(SharedString),
}

#[derive(Clone)]
struct CardData {
    id: SharedString,
    icon: Icon,
    icon_color: Rgba,
    name: SharedString,
    desc: SharedString,
    state: GridPickerItemState,
    favorite: bool,
}

enum PickerRow {
    Header {
        label: SharedString,
        dot: Rgba,
        count: usize,
    },
    Cards([Option<CardData>; 2]),
}

pub struct GridPicker {
    search: Entity<TextInput>,
    query: String,
    rail: RailSel,
    favorites: HashSet<SharedString>,
    groups: Vec<GridPickerGroup>,
    config: GridPickerConfig,
    palette: ForgePalette,
    rows: Vec<PickerRow>,
    match_total: usize,
    list_state: ListState,
    _search_sub: Subscription,
}

impl EventEmitter<GridPickerEvent> for GridPicker {}

impl GridPicker {
    pub fn new(
        config: GridPickerConfig,
        groups: Vec<GridPickerGroup>,
        favorites: HashSet<SharedString>,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = config.search_placeholder.clone();
        let search = cx.new(|cx| {
            TextInput::new(placeholder, cx)
                .with_palette(palette)
                .leading_icon(Icon::Search, palette.text_muted)
                .with_font_size(GRID_SEARCH_FS)
                .static_chrome(palette.border_regular, Radius::Sm)
        });
        let search_sub = cx.subscribe(&search, Self::on_search_event);

        let mut picker = Self {
            search,
            query: String::new(),
            rail: RailSel::All,
            favorites,
            groups,
            config,
            palette,
            rows: Vec::new(),
            match_total: 0,
            list_state: ListState::new(0, ListAlignment::Top, px(320.0)),
            _search_sub: search_sub,
        };
        picker.rebuild_rows(true);
        picker
    }

    fn rebuild_rows(&mut self, reset_scroll: bool) {
        let searching = !self.query.trim().is_empty();
        let query = self.query.trim().to_lowercase();

        let mut rows: Vec<PickerRow> = Vec::new();
        let mut total = 0usize;
        for group in &self.groups {
            let show_group = searching
                || matches!(self.rail, RailSel::All | RailSel::Favorites)
                || matches!(&self.rail, RailSel::Group(s) if s == &group.scope);
            if !show_group {
                continue;
            }
            let items: Vec<&GridPickerItem> = group
                .items
                .iter()
                .filter(|it| {
                    let matches_query = !searching
                        || it.name.to_lowercase().contains(&query)
                        || it.desc.to_lowercase().contains(&query);
                    let matches_fav =
                        !matches!(self.rail, RailSel::Favorites) || self.favorites.contains(&it.id);
                    matches_query && matches_fav
                })
                .collect();
            if items.is_empty() {
                continue;
            }
            total += items.len();
            rows.push(PickerRow::Header {
                label: group.label.clone(),
                dot: group.dot_color,
                count: items.len(),
            });
            for chunk in items.chunks(2) {
                let left = self.card_data(chunk[0]);
                let right = chunk.get(1).map(|it| self.card_data(it));
                rows.push(PickerRow::Cards([Some(left), right]));
            }
        }

        let count_changed = rows.len() != self.list_state.item_count();
        let new_count = rows.len();
        self.rows = rows;
        self.match_total = total;
        if reset_scroll || count_changed {
            self.list_state.reset(new_count);
        }
    }

    /// The caller must call this when the picker opens; gpui delivers key events only down the focus path, so without it typing and Escape never reach the search field.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.search.update(cx, |f, cx| f.focus(window, cx));
    }

    pub fn set_favorites(&mut self, favorites: HashSet<SharedString>, cx: &mut Context<Self>) {
        self.favorites = favorites;
        self.rebuild_rows(false);
        cx.notify();
    }

    fn on_search_event(
        &mut self,
        field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Changed(text) => {
                self.query = text.to_string();
                self.rebuild_rows(true);
                let border = if self.query.trim().is_empty() {
                    self.palette.border_regular
                } else {
                    self.config.accent
                };
                field.update(cx, |input, cx| {
                    input.set_static_chrome(Some((border, Radius::Sm)));
                    cx.notify();
                });
                cx.notify();
            }
            InputEvent::Cancelled => cx.emit(GridPickerEvent::Dismissed),
            InputEvent::Submitted(_) => {}
        }
    }

    fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.query.clear();
        self.rebuild_rows(true);
        let field = self.search.clone();
        let border = self.palette.border_regular;
        field.update(cx, |input, cx| {
            input.set_content("", cx);
            input.set_static_chrome(Some((border, Radius::Sm)));
        });
        cx.notify();
    }

    fn set_rail(&mut self, rail: RailSel, cx: &mut Context<Self>) {
        self.rail = rail;
        self.rebuild_rows(true);
        cx.notify();
    }

    fn emit_picked(&mut self, id: SharedString, cx: &mut Context<Self>) {
        cx.emit(GridPickerEvent::Picked(id));
    }

    fn toggle_favorite(&mut self, id: SharedString, cx: &mut Context<Self>) {
        if self.favorites.contains(&id) {
            self.favorites.remove(&id);
        } else {
            self.favorites.insert(id.clone());
        }
        self.rebuild_rows(false);
        cx.emit(GridPickerEvent::FavoriteToggled(id));
        cx.notify();
    }

    fn emit_dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(GridPickerEvent::Dismissed);
    }

    fn scope_entries(&self) -> Vec<(SharedString, String, Rgba)> {
        let mut seen: Vec<(SharedString, String, Rgba)> = Vec::new();
        for g in &self.groups {
            if g.scope.as_ref() == "all" || seen.iter().any(|(id, _, _)| id == &g.scope) {
                continue;
            }
            seen.push((g.scope.clone(), scope_label(&g.label), g.dot_color));
        }
        seen.sort_by_key(|(_, label, _)| label.to_lowercase());
        seen
    }

    fn leading_band_entry(&self) -> Option<(SharedString, String, Rgba)> {
        self.groups
            .iter()
            .find(|g| g.scope.as_ref() == "all")
            .map(|g| (g.scope.clone(), scope_label(&g.label), g.dot_color))
    }

    fn render_header(&self, accent: Rgba, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(GRID_TILE)
            .rounded(GRID_TILE_RADIUS)
            .bg(p.surface_overlay)
            .child(icon(self.config.header_icon, GRID_TILE_ICON, accent));

        let subtitle: AnyElement = match &self.config.subtitle {
            GridPickerSubtitle::Plain(text) => div()
                .overflow_hidden()
                .font_family(body_family())
                .text_size(GRID_META_FS)
                .text_color(p.text_faint)
                .child(text.clone())
                .into_any_element(),
            GridPickerSubtitle::Context { lead, name, note } => div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .overflow_hidden()
                .font_family(body_family())
                .text_size(GRID_META_FS)
                .child(div().text_color(p.text_faint).child(lead.clone()))
                .child(div().text_color(accent).child(name.clone()))
                .child(div().text_color(p.text_faint).child(note.clone()))
                .into_any_element(),
        };

        let titles = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(self.config.title.clone()),
            )
            .child(subtitle);

        let close = div()
            .id("forge-grid-close")
            .flex_none()
            .p(px(4.0))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(p.surface_overlay))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.emit_dismiss(cx)))
            .child(icon(Icon::X, GRID_CLOSE_ICON, p.text_faint));

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(GRID_HEADER_GAP)
            .py(GRID_HEADER_PAD_V)
            .px(GRID_BAND_PAD_H)
            .border_b(BORDER_ACCENT)
            .border_color(p.surface_overlay)
            .child(tile)
            .child(titles)
            .child(close)
            .into_any_element()
    }

    fn render_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(div().flex_1().min_w(px(0.0)).child(self.search.clone()));
        if !self.query.is_empty() {
            row = row.child(
                div()
                    .id("forge-grid-search-clear")
                    .flex_none()
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear_search(cx)))
                    .child(icon(Icon::X, GRID_SEARCH_ICON, p.text_faint)),
            );
        }

        div()
            .flex_none()
            .pt(GRID_SEARCH_PAD_T)
            .pb(GRID_SEARCH_PAD_B)
            .px(GRID_BAND_PAD_H)
            .border_b(BORDER_ACCENT)
            .border_color(p.surface_overlay)
            .child(row)
            .into_any_element()
    }

    fn render_rail(&self, accent: Rgba, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let star: AnyElement = icon(Icon::Star, RAIL_STAR, accent).into_any_element();
        let mut col = div()
            .id("forge-grid-rail")
            .flex_none()
            .w(RAIL_W)
            .flex()
            .flex_col()
            .gap(RAIL_GAP)
            .py(RAIL_PAD_V)
            .px(RAIL_PAD_H)
            .overflow_y_scroll()
            .border_r(BORDER_ACCENT)
            .border_color(p.surface_overlay)
            .child(grid_rail_entry(
                "forge-grid-rail-all",
                "All",
                Some(
                    div()
                        .flex_none()
                        .size(RAIL_DOT)
                        .rounded(radius(Radius::Pill))
                        .bg(p.text_muted)
                        .into_any_element(),
                ),
                matches!(self.rail, RailSel::All),
                &p,
                cx.listener(|this, _: &ClickEvent, _, cx| this.set_rail(RailSel::All, cx)),
            ))
            .child(grid_rail_entry(
                "forge-grid-rail-fav",
                self.config.favorites_label.clone(),
                Some(star),
                matches!(self.rail, RailSel::Favorites),
                &p,
                cx.listener(|this, _: &ClickEvent, _, cx| this.set_rail(RailSel::Favorites, cx)),
            ));

        if let Some((scope_id, label, dot)) = self.leading_band_entry() {
            let active = matches!(&self.rail, RailSel::Group(s) if s == &scope_id);
            let sid = scope_id.clone();
            let lead: AnyElement = div()
                .flex_none()
                .size(RAIL_DOT)
                .rounded(radius(Radius::Pill))
                .bg(dot)
                .into_any_element();
            col = col.child(grid_rail_entry(
                SharedString::from(format!("forge-grid-rail-{scope_id}")),
                label,
                Some(lead),
                active,
                &p,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_rail(RailSel::Group(sid.clone()), cx)
                }),
            ));
        }

        for (scope_id, label, dot) in self.scope_entries() {
            let active = matches!(&self.rail, RailSel::Group(s) if s == &scope_id);
            let sid = scope_id.clone();
            let lead: AnyElement = div()
                .flex_none()
                .size(RAIL_DOT)
                .rounded(radius(Radius::Pill))
                .bg(dot)
                .into_any_element();
            col = col.child(grid_rail_entry(
                SharedString::from(format!("forge-grid-rail-{scope_id}")),
                label,
                Some(lead),
                active,
                &p,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_rail(RailSel::Group(sid.clone()), cx)
                }),
            ));
        }

        col.into_any_element()
    }

    fn render_cards(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let query = self.query.trim().to_owned();
        let searching = !query.is_empty();
        let total = self.match_total;

        let mut col = div()
            .id("forge-grid-body")
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .py(GRID_BODY_PAD_V)
            .px(GRID_BAND_PAD_H);

        if searching {
            col = col.child(
                div()
                    .flex_none()
                    .pb(spacing(Spacing::Sm, Density::Cozy))
                    .font_family(body_family())
                    .text_size(GRID_META_FS)
                    .text_color(p.text_faint)
                    .child(format!(
                        "{total} {} for \u{201c}{query}\u{201d}",
                        if total == 1 { "match" } else { "matches" },
                    )),
            );
        }

        if self.rows.is_empty() {
            let favorites_empty = matches!(self.rail, RailSel::Favorites) && !searching;
            let (glyph, message): (Icon, SharedString) = if favorites_empty {
                (Icon::Star, self.config.favorites_empty.clone())
            } else {
                (
                    Icon::Search,
                    SharedString::from(format!("Nothing matches \u{201c}{query}\u{201d}")),
                )
            };
            return col
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(spacing(Spacing::Sm, Density::Cozy))
                        .py(GRID_EMPTY_PAD_V)
                        .child(icon(glyph, GRID_EMPTY_GLYPH, p.text_faint))
                        .child(
                            div()
                                .font_family(body_family())
                                .text_size(FONT_XS)
                                .text_color(p.text_muted)
                                .child(message),
                        ),
                )
                .into_any_element();
        }

        let list = list(
            self.list_state.clone(),
            cx.processor(|this, ix, _window, cx| this.render_list_row(ix, cx)),
        )
        .flex_1()
        .min_h(px(0.0));

        col.child(list).into_any_element()
    }

    fn render_list_row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let accent = self.config.accent;
        match &self.rows[ix] {
            PickerRow::Header { label, dot, count } => render_header_row(label, *dot, *count, &p),
            PickerRow::Cards(cards) => render_card_row(cards, accent, &p, cx),
        }
    }

    fn card_data(&self, item: &GridPickerItem) -> CardData {
        CardData {
            id: item.id.clone(),
            icon: item.icon,
            icon_color: item.icon_color,
            name: item.name.clone(),
            desc: item.desc.clone(),
            state: item.state,
            favorite: self.favorites.contains(&item.id),
        }
    }

    fn render_footer(&self) -> AnyElement {
        let p = self.palette;
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .py(GRID_FOOTER_PAD_V)
            .px(GRID_BAND_PAD_H)
            .bg(p.shell)
            .border_t(BORDER_ACCENT)
            .border_color(p.surface_overlay)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(p.text_faint)
                    .child(self.config.footer_hint.clone()),
            )
            .into_any_element()
    }
}

impl Render for GridPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let accent = self.config.accent;

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .child(self.render_rail(accent, cx))
            .child(self.render_cards(cx));

        div()
            .w(GRID_W)
            .h(GRID_H)
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(p.elevated)
            .rounded(radius(Radius::Lg))
            .border(BORDER_ACCENT)
            .border_color(p.border_regular)
            .child(self.render_header(accent, cx))
            .child(self.render_search(cx))
            .child(body)
            .child(self.render_footer())
    }
}

fn render_header_row(
    label: &SharedString,
    dot: Rgba,
    count: usize,
    p: &ForgePalette,
) -> AnyElement {
    let inner = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .child(
            div()
                .flex_none()
                .size(GRID_GROUP_DOT)
                .rounded(radius(Radius::Pill))
                .bg(dot),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(mono_family())
                .text_size(GRID_GROUP_FS)
                .text_color(p.text_muted)
                .child(label.to_uppercase()),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(GRID_GROUP_FS)
                .text_color(p.text_faint)
                .child(count.to_string()),
        );

    div()
        .h(GRID_HEADER_ROW_H)
        .w_full()
        .flex()
        .items_end()
        .pb(px(6.0))
        .child(inner)
        .into_any_element()
}

fn render_card_row(
    cards: &[Option<CardData>; 2],
    accent: Rgba,
    p: &ForgePalette,
    cx: &mut Context<GridPicker>,
) -> AnyElement {
    let row = div()
        .h(GRID_ROW_H)
        .w_full()
        .flex()
        .items_start()
        .gap(GRID_CARD_GAP);
    match cards {
        [Some(left), Some(right)] => row
            .child(render_card_el(left, accent, p, cx))
            .child(render_card_el(right, accent, p, cx)),
        [Some(left), None] => row.child(
            div()
                .w(relative(0.5))
                .flex()
                .child(render_card_el(left, accent, p, cx)),
        ),
        _ => row,
    }
    .into_any_element()
}

fn render_card_el(
    card: &CardData,
    accent: Rgba,
    p: &ForgePalette,
    cx: &mut Context<GridPicker>,
) -> AnyElement {
    let dim = !matches!(card.state, GridPickerItemState::Normal);

    let tile = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(GRID_CARD_TILE)
        .rounded(GRID_CARD_TILE_RADIUS)
        .bg(p.surface_overlay)
        .child(icon(card.icon, GRID_CARD_ICON, card.icon_color));

    let name = div()
        .flex_1()
        .min_w(px(0.0))
        .truncate()
        .font_family(body_family())
        .font_weight(FontWeight::MEDIUM)
        .text_size(GRID_CARD_NAME_FS)
        .text_color(p.text_primary)
        .child(card.name.clone());

    let star_glyph = if card.favorite {
        Icon::StarFilled
    } else {
        Icon::Star
    };
    let star_tint = if card.favorite { accent } else { p.text_faint };
    let star = div()
        .id(SharedString::from(format!("forge-grid-star-{}", card.id)))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .p(px(2.0))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
            cx.stop_propagation()
        })
        .on_click(cx.listener({
            let id = card.id.clone();
            move |this, _: &ClickEvent, _, cx| this.toggle_favorite(id.clone(), cx)
        }))
        .child(icon(star_glyph, GRID_CARD_ICON, star_tint));

    let trailing: Option<AnyElement> = match card.state {
        GridPickerItemState::Added => Some(
            badge(p.surface_overlay, p.success, "added", true, GRID_BADGE_FS).into_any_element(),
        ),
        GridPickerItemState::Disabled => Some(
            badge(
                p.surface_overlay,
                p.text_primary,
                "off",
                true,
                GRID_BADGE_FS,
            )
            .into_any_element(),
        ),
        GridPickerItemState::Normal => None,
    };

    let top = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .pb(GRID_CARD_ROW_MB)
        .child(tile)
        .child(name)
        .child(star)
        .children(trailing);

    let desc = div()
        .truncate()
        .w_full()
        .min_w(px(0.0))
        .font_family(body_family())
        .text_size(GRID_META_FS)
        .text_color(p.text_muted)
        .child(card.desc.clone());

    let base = div()
        .flex_1()
        .min_w(px(0.0))
        .h(GRID_CARD_H)
        .flex()
        .flex_col()
        .py(GRID_CARD_PAD_V)
        .px(GRID_CARD_PAD_H)
        .rounded(radius(Radius::Md))
        .border(BORDER_ACCENT)
        .border_color(p.border_regular)
        .bg(p.shell)
        .child(top)
        .child(desc);

    if matches!(card.state, GridPickerItemState::Added) {
        return base.opacity(0.5).into_any_element();
    }

    let base = if dim { base.opacity(0.5) } else { base };
    let border_input = p.border_input;
    base.id(card.id.clone())
        .cursor_pointer()
        .hover(move |s| s.border_color(border_input))
        .on_click(cx.listener({
            let id = card.id.clone();
            move |this, _: &ClickEvent, _, cx| this.emit_picked(id.clone(), cx)
        }))
        .into_any_element()
}

fn scope_label(group_label: &str) -> String {
    group_label
        .split(" \u{b7} ")
        .next()
        .unwrap_or(group_label)
        .to_owned()
}

fn grid_rail_entry(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    lead: Option<AnyElement>,
    active: bool,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let (bg, text_color): (Rgba, Rgba) = if active {
        (palette.surface_overlay, palette.text_primary)
    } else {
        (gpui::transparent_black().into(), palette.text_secondary)
    };
    let hover_bg = palette.surface_overlay;
    let lead_slot = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(RAIL_LEAD_SLOT)
        .children(lead);

    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap(RAIL_ENTRY_GAP)
        .w_full()
        .py(RAIL_ENTRY_PAD_V)
        .px(RAIL_ENTRY_PAD_H)
        .rounded(radius(Radius::Sm))
        .bg(bg)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_click(handler)
        .child(lead_slot)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_family(body_family())
                .text_size(GRID_META_FS)
                .text_color(text_color)
                .child(label.into()),
        )
        .into_any_element()
}
