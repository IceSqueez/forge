use crate::FONT_BADGE;
use crate::FONT_FIELD;
use crate::FONT_META;
use crate::FONT_ROW;
use crate::FONT_SECTION_LABEL;
use crate::key_activation::KeyActivated;
use crate::presentation::ActivePresentation;
use std::collections::HashSet;

use crate::focus_reveal::OverflowRevealingFocus;
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, EventEmitter, FontWeight,
    ListAlignment, ListState, Pixels, Rgba, SharedString, Subscription, Window, div, list,
    prelude::*, px,
};

use crate::error_row::compact_error_with_retry;
use crate::glyph::GlyphArt;
use crate::icons::{Icon, icon};
use crate::picker::{PICKER_CONTEXT, SelectNext, SelectPrev};
use crate::search_state::{SearchState, contains_folded};
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, Radius, Spacing, radius, scaled, spacing,
};
use crate::tr;

mod rows;

use rows::{render_card_row, render_header_row};

const GRID_W: Pixels = px(880.0);
const GRID_H: Pixels = px(640.0);
const GRID_TILE: Pixels = px(30.0);
const GRID_TILE_RADIUS: Pixels = px(7.0);
const GRID_TILE_ICON: Pixels = px(15.0);
const GRID_HEADER_GAP: Pixels = px(11.0);
const GRID_HEADER_PAD_V: Pixels = px(13.0);
const GRID_CLOSE_ICON: Pixels = px(15.0);
const GRID_SEARCH_PAD_T: Pixels = px(11.0);
const GRID_SEARCH_PAD_B: Pixels = px(9.0);
const GRID_SEARCH_ICON: Pixels = px(14.0);
const GRID_SEARCH_FS: Pixels = FONT_FIELD;
const RAIL_W: Pixels = px(184.0);
const RAIL_GAP: Pixels = px(2.0);
const RAIL_ENTRY_GAP: Pixels = px(8.0);
const RAIL_ENTRY_PAD_H: Pixels = px(8.0);
const RAIL_LEAD_SLOT: Pixels = px(14.0);
const RAIL_DOT: Pixels = px(6.0);
const RAIL_STAR: Pixels = px(12.0);
const GRID_BODY_PAD_V: Pixels = px(13.0);
const GRID_GROUP_FS: Pixels = FONT_SECTION_LABEL;
const GRID_GROUP_DOT: Pixels = px(5.0);
const GRID_CARD_GAP: Pixels = px(8.0);
const GRID_CARD_PAD_V: Pixels = px(11.0);
const GRID_CARD_PAD_H: Pixels = px(12.0);
const GRID_CARD_TILE: Pixels = px(26.0);
const GRID_CARD_TILE_RADIUS: Pixels = px(7.0);
const GRID_CARD_ICON: Pixels = px(13.0);
const GRID_CARD_NAME_FS: Pixels = FONT_ROW;
const GRID_CARD_ROW_MB: Pixels = px(6.0);
const GRID_META_FS: Pixels = FONT_META;
const GRID_FOOTER_PAD_V: Pixels = px(8.0);
const GRID_EMPTY_PAD_V: Pixels = px(50.0);
const GRID_EMPTY_GLYPH: Pixels = px(22.0);
const GRID_BADGE_FS: Pixels = FONT_BADGE;
const GRID_CARD_DESC_H: Pixels = px(18.0);
const GRID_LIST_OVERDRAW: Pixels = px(320.0);
const GRID_STAR_PAD: Pixels = px(2.0);

const GRID_HEADER_ROW_H: Pixels = px(30.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridPickerItemState {
    Normal,
    Disabled,
}

pub(crate) type GridItemQuery = Box<dyn Fn(&str) -> bool>;

pub struct GridPickerItem {
    pub id: SharedString,
    pub glyph: GlyphArt,
    pub tint: Rgba,
    pub name: SharedString,
    pub desc: SharedString,
    pub state: GridPickerItemState,
    pub matches: Option<GridItemQuery>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GridScope {
    Leading,
    Named(SharedString),
}

impl GridScope {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Leading => None,
            Self::Named(name) => Some(name),
        }
    }

    fn rail_element_id(&self) -> SharedString {
        match self {
            Self::Leading => SharedString::new_static("forge-grid-rail-leading"),
            Self::Named(name) => SharedString::from(format!("forge-grid-rail-{name}")),
        }
    }
}

pub struct GridPickerGroup {
    pub label: SharedString,
    pub dot_color: Rgba,
    pub scope: GridScope,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridPickerArt {
    pub tile: Pixels,
    pub glyph: Pixels,
}

impl Default for GridPickerArt {
    fn default() -> Self {
        Self {
            tile: GRID_CARD_TILE,
            glyph: GRID_CARD_ICON,
        }
    }
}

impl GridPickerArt {
    fn card_height(self) -> Pixels {
        GRID_CARD_PAD_V + self.tile + GRID_CARD_ROW_MB + GRID_CARD_DESC_H + GRID_CARD_PAD_V
    }

    fn row_height(self) -> Pixels {
        self.card_height() + GRID_CARD_GAP
    }
}

pub struct GridPickerConfig {
    pub accent: Rgba,
    pub art: GridPickerArt,
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
    FavoritesRetry,
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FavoritesStatus {
    Ready,
    Pending,
    Failed(SharedString),
}

#[derive(Clone, PartialEq)]
enum RailSel {
    All,
    Favorites,
    Group(GridScope),
}

struct RailScope {
    scope: GridScope,
    label: SharedString,
    dot: Rgba,
}

#[derive(Clone)]
struct CardData {
    id: SharedString,
    glyph: GlyphArt,
    tint: Rgba,
    name: SharedString,
    desc: SharedString,
    state: GridPickerItemState,
    favorite: bool,
    favorite_editable: bool,
    cursored: bool,
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
    search: SearchState,
    rail: RailSel,
    favorites: HashSet<SharedString>,
    favorites_status: FavoritesStatus,
    groups: Vec<GridPickerGroup>,
    config: GridPickerConfig,
    rows: Vec<PickerRow>,
    matched: Vec<MatchedItem>,
    cursor: usize,
    cursor_visible: bool,
    match_total: usize,
    list_state: ListState,
    _search_sub: Subscription,
}

#[derive(Clone)]
struct MatchedItem {
    id: SharedString,
    row: usize,
}

impl EventEmitter<GridPickerEvent> for GridPicker {}

impl GridPicker {
    pub fn new(
        config: GridPickerConfig,
        groups: Vec<GridPickerGroup>,
        favorites: HashSet<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = config.search_placeholder.clone();
        let search = SearchState::from_field(cx.new(|cx| {
            TextInput::new(placeholder, cx)
                .leading_icon(Icon::Search, |palette| palette.text_muted)
                .with_font_size(GRID_SEARCH_FS)
                .static_chrome(|palette| palette.border_regular, Radius::Sm)
        }));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);

        let mut picker = Self {
            search,
            rail: RailSel::All,
            favorites,
            favorites_status: FavoritesStatus::Ready,
            groups,
            config,
            rows: Vec::new(),
            matched: Vec::new(),
            cursor: 0,
            cursor_visible: false,
            match_total: 0,
            list_state: ListState::new(0, ListAlignment::Top, GRID_LIST_OVERDRAW),
            _search_sub: search_sub,
        };
        picker.rebuild_rows(true);
        picker
    }

    fn rebuild_rows(&mut self, reset_scroll: bool) {
        let searching = !self.search.is_empty();
        let query = self.search.query().to_owned();

        let mut shown: Vec<(&GridPickerGroup, Vec<&GridPickerItem>)> = Vec::new();
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
                    let matches_query = !searching || item_hit(it, &query);
                    let matches_fav =
                        !matches!(self.rail, RailSel::Favorites) || self.favorites.contains(&it.id);
                    matches_query && matches_fav
                })
                .collect();
            if items.is_empty() {
                continue;
            }
            shown.push((group, items));
        }

        let total: usize = shown.iter().map(|(_, items)| items.len()).sum();
        let cursor = if reset_scroll {
            self.cursor_visible = false;
            0
        } else {
            self.cursor.min(total.saturating_sub(1))
        };

        let mut rows: Vec<PickerRow> = Vec::new();
        let mut matched: Vec<MatchedItem> = Vec::new();
        for (group, items) in shown {
            rows.push(PickerRow::Header {
                label: group.label.clone(),
                dot: group.dot_color,
                count: items.len(),
            });
            for chunk in items.chunks(2) {
                let row = rows.len();
                let mut cards: [Option<CardData>; 2] = [None, None];
                for (slot, item) in chunk.iter().enumerate() {
                    cards[slot] =
                        Some(self.card_data(item, self.cursor_visible && matched.len() == cursor));
                    matched.push(MatchedItem {
                        id: item.id.clone(),
                        row,
                    });
                }
                rows.push(PickerRow::Cards(cards));
            }
        }

        let count_changed = rows.len() != self.list_state.item_count();
        let new_count = rows.len();
        self.rows = rows;
        self.matched = matched;
        self.cursor = cursor;
        self.match_total = total;
        if reset_scroll || count_changed {
            self.list_state.reset(new_count);
        }
    }

    fn move_cursor(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.matched.is_empty() {
            return;
        }
        let last = self.matched.len() - 1;
        let first_press = !self.cursor_visible;
        self.cursor_visible = true;
        self.cursor = match (forward, self.cursor) {
            (true, position) if first_press => position,
            (true, position) if position >= last => 0,
            (true, position) => position + 1,
            (false, 0) => last,
            (false, position) => position - 1,
        };
        self.rebuild_rows(false);
        if let Some(item) = self.matched.get(self.cursor) {
            self.list_state.scroll_to_reveal_item(item.row);
        }
        cx.notify();
    }

    fn select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(true, cx);
    }

    fn select_prev(&mut self, _: &SelectPrev, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(false, cx);
    }

    fn confirm_cursored(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.matched.get(self.cursor) else {
            return;
        };
        let id = item.id.clone();
        self.emit_picked(id, cx);
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.search.field().update(cx, |f, cx| f.focus(window, cx));
    }

    pub fn set_groups(&mut self, groups: Vec<GridPickerGroup>, cx: &mut Context<Self>) {
        self.groups = groups;
        self.rebuild_rows(false);
        cx.notify();
    }

    pub fn set_favorites(&mut self, favorites: HashSet<SharedString>, cx: &mut Context<Self>) {
        self.favorites = favorites;
        self.rebuild_rows(false);
        cx.notify();
    }

    pub fn set_favorites_status(&mut self, status: FavoritesStatus, cx: &mut Context<Self>) {
        self.favorites_status = status;
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
            InputEvent::Changed(_) => {
                self.search.on_changed(event);
                self.rebuild_rows(true);
                let border = (!self.search.is_empty()).then_some(self.config.accent);
                field.update(cx, |input, cx| {
                    input.set_border_override(border);
                    cx.notify();
                });
                cx.notify();
            }
            InputEvent::Cancelled => cx.emit(GridPickerEvent::Dismissed),
            InputEvent::Submitted(_) => self.confirm_cursored(cx),
            InputEvent::Blurred(_) => {}
        }
    }

    fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search.clear(cx);
        self.rebuild_rows(true);
        self.search.field().update(cx, |input, _cx| {
            input.set_border_override(None);
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
        if self.favorites_status != FavoritesStatus::Ready {
            return;
        }
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

    fn rail_scopes(&self) -> Vec<RailScope> {
        let leading = self
            .groups
            .iter()
            .filter(|group| group.scope == GridScope::Leading)
            .take(1);
        let named = self
            .groups
            .iter()
            .filter(|group| group.scope != GridScope::Leading);
        let mut scopes: Vec<RailScope> = Vec::new();
        for group in leading.chain(named) {
            if scopes.iter().all(|seen| seen.scope != group.scope) {
                scopes.push(RailScope {
                    scope: group.scope.clone(),
                    label: group.label.clone(),
                    dot: group.dot_color,
                });
            }
        }
        scopes
    }

    fn render_header(&self, accent: Rgba, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let p = cx.palette();

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
                .font_family(cx.body_family())
                .text_size(GRID_META_FS)
                .text_color(p.text_faint)
                .child(text.clone())
                .into_any_element(),
            GridPickerSubtitle::Context { lead, name, note } => div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .overflow_hidden()
                .font_family(cx.body_family())
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
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(self.config.title.clone()),
            )
            .child(subtitle);

        let close = div()
            .id("forge-grid-close")
            .flex_none()
            .p(spacing(Spacing::Xxs, density))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(p.surface_overlay))
            .key_activated(p.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.emit_dismiss(cx)))
            .child(icon(Icon::X, GRID_CLOSE_ICON, p.text_faint));

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(scaled(GRID_HEADER_GAP, density))
            .py(scaled(GRID_HEADER_PAD_V, density))
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(p.surface_overlay)
            .child(tile)
            .child(titles)
            .child(close)
            .into_any_element()
    }

    fn render_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let p = cx.palette();
        let mut row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.search.field().clone()),
            );
        if !self.search.typed().is_empty() {
            row = row.child(
                div()
                    .id("forge-grid-search-clear")
                    .flex_none()
                    .cursor_pointer()
                    .key_activated(p.text_primary)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear_search(cx)))
                    .child(icon(Icon::X, GRID_SEARCH_ICON, p.text_faint)),
            );
        }

        div()
            .flex_none()
            .pt(scaled(GRID_SEARCH_PAD_T, density))
            .pb(scaled(GRID_SEARCH_PAD_B, density))
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(p.surface_overlay)
            .child(row)
            .into_any_element()
    }

    fn render_rail(&self, accent: Rgba, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let p = cx.palette();
        let star: AnyElement = icon(Icon::Star, RAIL_STAR, accent).into_any_element();
        let mut col = div()
            .id("forge-grid-rail")
            .flex_none()
            .w(RAIL_W)
            .flex()
            .flex_col()
            .gap(scaled(RAIL_GAP, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .overflow_y_revealing_focus()
            .border_r(BORDER_THIN)
            .border_color(p.surface_overlay)
            .child(grid_rail_entry(
                "forge-grid-rail-all",
                tr!("widget_grid_picker_all"),
                Some(rail_dot(p.text_muted)),
                matches!(self.rail, RailSel::All),
                cx.listener(|this, _: &ClickEvent, _, cx| this.set_rail(RailSel::All, cx)),
                density,
                cx,
            ))
            .child(grid_rail_entry(
                "forge-grid-rail-fav",
                self.config.favorites_label.clone(),
                Some(star),
                matches!(self.rail, RailSel::Favorites),
                cx.listener(|this, _: &ClickEvent, _, cx| this.set_rail(RailSel::Favorites, cx)),
                density,
                cx,
            ));
        if let FavoritesStatus::Failed(message) = &self.favorites_status {
            col = col.child(compact_error_with_retry(
                "forge-grid-rail-fav-retry",
                message.clone(),
                &p,
                cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(GridPickerEvent::FavoritesRetry)),
                density,
                cx,
            ));
        }

        for entry in self.rail_scopes() {
            let active = matches!(&self.rail, RailSel::Group(selected) if selected == &entry.scope);
            let selection = entry.scope.clone();
            col = col.child(grid_rail_entry(
                entry.scope.rail_element_id(),
                entry.label,
                Some(rail_dot(entry.dot)),
                active,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_rail(RailSel::Group(selection.clone()), cx)
                }),
                density,
                cx,
            ));
        }

        col.into_any_element()
    }

    fn render_cards(&self, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let p = cx.palette();
        let query = self.search.typed().trim().to_owned();
        let searching = !query.is_empty();
        let total = self.match_total;

        let mut col = div()
            .id("forge-grid-body")
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .py(scaled(GRID_BODY_PAD_V, density))
            .px(spacing(Spacing::Md, density));

        if searching {
            col = col.child(
                div()
                    .flex_none()
                    .pb(spacing(Spacing::Sm, density))
                    .font_family(cx.body_family())
                    .text_size(GRID_META_FS)
                    .text_color(p.text_faint)
                    .child(if total == 1 {
                        tr!("widget_grid_picker_match_one", query = query.as_str())
                    } else {
                        tr!(
                            "widget_grid_picker_match_many",
                            total = total,
                            query = query.as_str()
                        )
                    }),
            );
        }

        if self.rows.is_empty() {
            let favorites_empty = matches!(self.rail, RailSel::Favorites) && !searching;
            let (glyph, message): (Icon, SharedString) = if favorites_empty {
                (Icon::Star, self.config.favorites_empty.clone())
            } else {
                (
                    Icon::Search,
                    SharedString::from(tr!("widget_grid_picker_no_match", query = query.as_str())),
                )
            };
            return col
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(spacing(Spacing::Sm, density))
                        .py(scaled(GRID_EMPTY_PAD_V, density))
                        .child(icon(glyph, GRID_EMPTY_GLYPH, p.text_faint))
                        .child(
                            div()
                                .font_family(cx.body_family())
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
        let density = cx.density();
        let p = cx.palette();
        let accent = self.config.accent;
        let art = self.config.art;
        match &self.rows[ix] {
            PickerRow::Header { label, dot, count } => {
                render_header_row(label, *dot, *count, &p, density, cx)
            }
            PickerRow::Cards(cards) => render_card_row(cards, accent, art, &p, cx),
        }
    }

    fn card_data(&self, item: &GridPickerItem, cursored: bool) -> CardData {
        CardData {
            id: item.id.clone(),
            glyph: item.glyph.clone(),
            tint: item.tint,
            name: item.name.clone(),
            desc: item.desc.clone(),
            state: item.state,
            favorite: self.favorites.contains(&item.id),
            favorite_editable: self.favorites_status == FavoritesStatus::Ready,
            cursored,
        }
    }

    fn render_footer(&self, density: Density, cx: &App) -> AnyElement {
        let p = cx.palette();
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .py(scaled(GRID_FOOTER_PAD_V, density))
            .px(spacing(Spacing::Md, density))
            .bg(p.shell)
            .border_t(BORDER_THIN)
            .border_color(p.surface_overlay)
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XXS)
                    .text_color(p.text_faint)
                    .child(self.config.footer_hint.clone()),
            )
            .into_any_element()
    }
}

impl Render for GridPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let p = cx.palette();
        let accent = self.config.accent;

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .child(self.render_rail(accent, cx))
            .child(self.render_cards(cx));

        div()
            .key_context(PICKER_CONTEXT)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_prev))
            .w(GRID_W)
            .h(GRID_H)
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(p.elevated)
            .rounded(radius(Radius::Lg))
            .border(BORDER_THIN)
            .border_color(p.border_regular)
            .child(self.render_header(accent, cx))
            .child(self.render_search(cx))
            .child(body)
            .child(self.render_footer(density, cx))
    }
}

fn item_hit(item: &GridPickerItem, query: &str) -> bool {
    match &item.matches {
        Some(test) => test(query),
        None => contains_folded(&item.name, query) || contains_folded(&item.desc, query),
    }
}

fn rail_dot(color: Rgba) -> AnyElement {
    div()
        .flex_none()
        .size(RAIL_DOT)
        .rounded(radius(Radius::Pill))
        .bg(color)
        .into_any_element()
}

fn grid_rail_entry(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    lead: Option<AnyElement>,
    active: bool,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    density: Density,
    cx: &App,
) -> AnyElement {
    let palette = &cx.palette();
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
        .gap(scaled(RAIL_ENTRY_GAP, density))
        .w_full()
        .py(spacing(Spacing::Xs, density))
        .px(scaled(RAIL_ENTRY_PAD_H, density))
        .rounded(radius(Radius::Sm))
        .bg(bg)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .key_activated(palette.text_primary)
        .on_click(handler)
        .child(lead_slot)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_family(cx.body_family())
                .text_size(GRID_META_FS)
                .text_color(text_color)
                .child(label.into()),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use crate::test_support::install_presentation;
    use gpui::VisualTestContext;

    use super::*;
    use crate::palette::FORGE_DEFAULT;

    const ALPHA: &str = "alpha";
    const BETA: &str = "beta";
    const GAMMA: &str = "gamma";
    const SHARED_WORD: &str = "pickable";

    struct Picked {
        seen: Vec<String>,
        _sub: Subscription,
    }

    fn settings() -> GridPickerConfig {
        GridPickerConfig {
            accent: FORGE_DEFAULT.brand,
            art: GridPickerArt::default(),
            header_icon: Icon::Photo,
            title: "Choose".into(),
            subtitle: GridPickerSubtitle::Plain("subtitle".into()),
            footer_hint: "hint".into(),
            search_placeholder: "search".into(),
            favorites_label: "Favorites".into(),
            favorites_empty: "empty".into(),
        }
    }

    fn item(id: &str, name: &str, desc: &str, matches: Option<GridItemQuery>) -> GridPickerItem {
        GridPickerItem {
            id: SharedString::from(id.to_owned()),
            glyph: GlyphArt::Icon(Icon::Photo),
            tint: FORGE_DEFAULT.brand,
            name: SharedString::from(name.to_owned()),
            desc: SharedString::from(desc.to_owned()),
            state: GridPickerItemState::Normal,
            matches,
        }
    }

    fn band(scope: &str, items: Vec<GridPickerItem>) -> GridPickerGroup {
        GridPickerGroup {
            label: SharedString::from(scope.to_owned()),
            dot_color: FORGE_DEFAULT.brand,
            scope: GridScope::Named(SharedString::from(scope.to_owned())),
            items,
        }
    }

    fn plain_roster() -> Vec<GridPickerGroup> {
        vec![
            band(
                "first",
                vec![
                    item(ALPHA, "Alpha", SHARED_WORD, None),
                    item(BETA, "Beta", SHARED_WORD, None),
                ],
            ),
            band("second", vec![item(GAMMA, "Gamma", SHARED_WORD, None)]),
        ]
    }

    fn open(
        cx: &mut gpui::TestAppContext,
        groups: Vec<GridPickerGroup>,
    ) -> (Entity<GridPicker>, Entity<Picked>, &mut VisualTestContext) {
        install_presentation(cx);
        let (picker, vcx) = cx
            .add_window_view(|_window, cx| GridPicker::new(settings(), groups, HashSet::new(), cx));
        let picked = vcx.update(|_window, cx| {
            cx.new(|cx| Picked {
                seen: Vec::new(),
                _sub: cx.subscribe(&picker, |this: &mut Picked, _view, event, _cx| {
                    if let GridPickerEvent::Picked(id) = event {
                        this.seen.push(id.to_string());
                    }
                }),
            })
        });
        (picker, picked, vcx)
    }

    fn search(picker: &Entity<GridPicker>, vcx: &mut VisualTestContext, event: InputEvent) {
        vcx.update(|_window, cx| {
            picker.update(cx, |view, cx| {
                let field = view.search.field().clone();
                view.on_search_event(field, &event, cx);
            });
        });
        vcx.run_until_parked();
    }

    fn type_query(picker: &Entity<GridPicker>, vcx: &mut VisualTestContext, text: &str) {
        search(
            picker,
            vcx,
            InputEvent::Changed(SharedString::from(text.to_owned())),
        );
    }

    fn press_enter(picker: &Entity<GridPicker>, vcx: &mut VisualTestContext) {
        search(picker, vcx, InputEvent::Submitted(SharedString::default()));
    }

    fn arrow(picker: &Entity<GridPicker>, vcx: &mut VisualTestContext, forward: bool) {
        vcx.update(|window, cx| {
            picker.update(cx, |view, cx| {
                if forward {
                    view.select_next(&SelectNext, window, cx);
                } else {
                    view.select_prev(&SelectPrev, window, cx);
                }
            });
        });
    }

    fn matched_ids(picker: &Entity<GridPicker>, vcx: &mut VisualTestContext) -> Vec<String> {
        vcx.update(|_window, cx| {
            picker
                .read(cx)
                .matched
                .iter()
                .map(|entry| entry.id.to_string())
                .collect()
        })
    }

    fn cursored(picker: &Entity<GridPicker>, vcx: &mut VisualTestContext) -> Option<String> {
        vcx.update(|_window, cx| {
            let view = picker.read(cx);
            view.matched
                .get(view.cursor)
                .map(|entry| entry.id.to_string())
        })
    }

    fn ring_shown(picker: &Entity<GridPicker>, vcx: &mut VisualTestContext) -> bool {
        vcx.update(|_window, cx| picker.read(cx).cursor_visible)
    }

    fn picks(picked: &Entity<Picked>, vcx: &mut VisualTestContext) -> Vec<String> {
        vcx.update(|_window, cx| picked.read(cx).seen.clone())
    }

    #[test]
    fn a_taller_tile_grows_the_card_and_its_row_by_what_the_tile_gained() {
        let base = GridPickerArt::default();

        for extra in [px(4.0), px(18.0)] {
            let grown = GridPickerArt {
                tile: base.tile + extra,
                ..base
            };

            assert_eq!(
                grown.card_height(),
                base.card_height() + extra,
                "a tile {extra:?} taller",
            );
            assert_eq!(
                grown.row_height(),
                base.row_height() + extra,
                "a tile {extra:?} taller",
            );
        }
    }

    #[gpui::test]
    fn the_rail_lists_the_scopes_in_the_order_the_caller_pushed_its_groups(
        cx: &mut gpui::TestAppContext,
    ) {
        let groups = vec![
            band("zebra", vec![item(ALPHA, "Alpha", SHARED_WORD, None)]),
            band("alpha", vec![item(BETA, "Beta", SHARED_WORD, None)]),
            band("mid", vec![item(GAMMA, "Gamma", SHARED_WORD, None)]),
        ];
        let (picker, _picked, vcx) = open(cx, groups);

        let rail = vcx.update(|_window, cx| {
            picker
                .read(cx)
                .rail_scopes()
                .into_iter()
                .map(|entry| entry.label.to_string())
                .collect::<Vec<_>>()
        });

        assert_eq!(
            rail,
            ["zebra".to_owned(), "alpha".to_owned(), "mid".to_owned()]
        );
    }

    #[gpui::test]
    fn an_item_that_carries_its_own_test_is_searched_through_it_instead_of_its_words(
        cx: &mut gpui::TestAppContext,
    ) {
        let groups = vec![band(
            "only",
            vec![
                item("own-hit", "zzz", "zzz", Some(Box::new(|q| q == "glow"))),
                item("own-miss", "Glowing", "glow", Some(Box::new(|_| false))),
                item("words", "Glow ring", "", None),
            ],
        )];
        let (picker, _picked, vcx) = open(cx, groups);

        type_query(&picker, vcx, "glow");

        assert_eq!(matched_ids(&picker, vcx), vec!["own-hit", "words"]);
    }

    #[gpui::test]
    fn the_cursor_wraps_forward_through_every_match_in_display_order(
        cx: &mut gpui::TestAppContext,
    ) {
        let (picker, _picked, vcx) = open(cx, plain_roster());

        let mut walked = Vec::new();
        for _ in 0..4 {
            arrow(&picker, vcx, true);
            walked.push(cursored(&picker, vcx));
        }

        assert_eq!(
            walked,
            vec![
                Some(ALPHA.to_owned()),
                Some(BETA.to_owned()),
                Some(GAMMA.to_owned()),
                Some(ALPHA.to_owned()),
            ],
        );
    }

    #[gpui::test]
    fn the_cursor_wraps_backward_from_the_first_match_to_the_last(cx: &mut gpui::TestAppContext) {
        let (picker, _picked, vcx) = open(cx, plain_roster());
        arrow(&picker, vcx, true);

        arrow(&picker, vcx, false);

        assert_eq!(cursored(&picker, vcx), Some(GAMMA.to_owned()));
    }

    #[gpui::test]
    fn no_keyboard_ring_is_shown_until_the_first_arrow_press(cx: &mut gpui::TestAppContext) {
        let (picker, _picked, vcx) = open(cx, plain_roster());

        assert!(!ring_shown(&picker, vcx));
    }

    #[gpui::test]
    fn the_first_down_press_reveals_the_ring_on_the_first_card_without_moving(
        cx: &mut gpui::TestAppContext,
    ) {
        let (picker, _picked, vcx) = open(cx, plain_roster());

        arrow(&picker, vcx, true);

        assert!(ring_shown(&picker, vcx));
        assert_eq!(cursored(&picker, vcx), Some(ALPHA.to_owned()));
    }

    #[gpui::test]
    fn the_first_up_press_reveals_the_ring_on_the_last_card(cx: &mut gpui::TestAppContext) {
        let (picker, _picked, vcx) = open(cx, plain_roster());

        arrow(&picker, vcx, false);

        assert!(ring_shown(&picker, vcx));
        assert_eq!(cursored(&picker, vcx), Some(GAMMA.to_owned()));
    }

    #[gpui::test]
    fn changing_the_query_hides_the_ring_again(cx: &mut gpui::TestAppContext) {
        let (picker, _picked, vcx) = open(cx, plain_roster());
        arrow(&picker, vcx, true);
        arrow(&picker, vcx, true);

        type_query(&picker, vcx, SHARED_WORD);

        assert!(!ring_shown(&picker, vcx));
    }

    #[gpui::test]
    fn enter_with_the_ring_hidden_picks_the_first_match(cx: &mut gpui::TestAppContext) {
        let (picker, picked, vcx) = open(cx, plain_roster());

        press_enter(&picker, vcx);

        assert_eq!(picks(&picked, vcx), vec![ALPHA.to_owned()]);
        assert!(!ring_shown(&picker, vcx));
    }

    #[gpui::test]
    fn a_new_query_puts_the_cursor_back_on_the_first_match(cx: &mut gpui::TestAppContext) {
        let (picker, _picked, vcx) = open(cx, plain_roster());
        arrow(&picker, vcx, true);
        arrow(&picker, vcx, true);

        type_query(&picker, vcx, SHARED_WORD);

        assert_eq!(cursored(&picker, vcx), Some(ALPHA.to_owned()));
    }

    #[gpui::test]
    fn enter_picks_the_item_the_cursor_rests_on(cx: &mut gpui::TestAppContext) {
        let (picker, picked, vcx) = open(cx, plain_roster());
        arrow(&picker, vcx, true);
        arrow(&picker, vcx, true);

        press_enter(&picker, vcx);

        assert_eq!(picks(&picked, vcx), vec![BETA.to_owned()]);
    }

    #[gpui::test]
    fn enter_over_a_query_that_matches_nothing_picks_nothing(cx: &mut gpui::TestAppContext) {
        let (picker, picked, vcx) = open(cx, plain_roster());
        type_query(&picker, vcx, "no-such-card");

        press_enter(&picker, vcx);

        assert!(picks(&picked, vcx).is_empty());
    }

    #[gpui::test]
    fn a_roster_that_arrives_after_the_search_was_typed_is_still_filtered_by_it(
        cx: &mut gpui::TestAppContext,
    ) {
        let (picker, _picked, vcx) = open(cx, plain_roster());
        type_query(&picker, vcx, BETA);

        vcx.update(|_window, cx| {
            picker.update(cx, |view, cx| {
                let mut grown = plain_roster();
                grown.push(band("late", vec![item("betamax", "Betamax", "", None)]));
                view.set_groups(grown, cx);
            });
        });

        assert_eq!(
            matched_ids(&picker, vcx),
            vec![BETA.to_owned(), "betamax".to_owned()],
        );
    }

    struct Starred {
        toggles: usize,
        _sub: Subscription,
    }

    #[gpui::test]
    fn a_star_click_toggles_and_reports_only_while_favorites_are_ready(
        cx: &mut gpui::TestAppContext,
    ) {
        for (status, expected) in [
            (FavoritesStatus::Ready, (true, 1)),
            (FavoritesStatus::Pending, (false, 0)),
            (FavoritesStatus::Failed("unreadable".into()), (false, 0)),
        ] {
            let (picker, _picked, vcx) = open(cx, plain_roster());
            let starred = vcx.update(|_window, cx| {
                cx.new(|cx| Starred {
                    toggles: 0,
                    _sub: cx.subscribe(&picker, |this: &mut Starred, _view, event, _cx| {
                        if let GridPickerEvent::FavoriteToggled(_) = event {
                            this.toggles += 1;
                        }
                    }),
                })
            });

            vcx.update(|_window, cx| {
                picker.update(cx, |view, cx| {
                    view.set_favorites_status(status.clone(), cx);
                    view.toggle_favorite(SharedString::from(ALPHA), cx);
                });
            });
            vcx.run_until_parked();

            let seen = vcx.update(|_window, cx| {
                (
                    picker.read(cx).favorites.contains(ALPHA),
                    starred.read(cx).toggles,
                )
            });
            assert_eq!(seen, expected, "{status:?}");
        }
    }
}
