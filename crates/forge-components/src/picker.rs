//! Searchable item picker — a stateful `Entity` view (owns the query field, the item
//! list and the derived match set), NOT a stateless `RenderOnce` card. The screen
//! creates and holds `Entity<Picker>`, wraps it in a centred [`crate::overlay`] to gain
//! the scrim + enter animation, and reacts to a chosen or dismissed picker via
//! `cx.subscribe(&picker, …)` on [`PickerEvent`].
//!
//! Typing in the embedded search field re-derives the visible rows (case-insensitive
//! substring over each item's label OR sublabel; an empty query shows every item).
//! Clicking a row reports the item's id through [`PickerEvent::Selected`]; Escape in the
//! search field reports [`PickerEvent::Cancelled`], as does the footer cancel button.
//! The consuming screen owns the open/closed state and hides the overlay when either
//! event lands.

use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, Entity, EventEmitter, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};

use crate::buttons::secondary_button;
use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::text_input::{InputEvent, TextInput, search_input};
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_MD, FONT_SM, Radius, Spacing, radius, spacing,
};

/// Card width envelope. The source caps the card at 480px and lets its full-width bands
/// push it to that cap, so the rendered card is a fixed 480 wide — reproduced here as a
/// fixed width.
const CARD_WIDTH: Pixels = px(480.0);
/// Viewport height of the scrolling result list; content taller than this scrolls.
const LIST_HEIGHT: Pixels = px(320.0);
/// Height of the centred loading placeholder that stands in for the list.
const LOADING_HEIGHT: Pixels = px(200.0);
/// Height of the centred "no matches" placeholder that stands in for the list.
const EMPTY_HEIGHT: Pixels = px(120.0);
/// Side of the square tile behind each row's leading glyph.
const ICON_TILE: Pixels = px(28.0);
/// Rendered size of the glyph centred in a row's icon tile.
const ICON_TILE_GLYPH: Pixels = px(14.0);
/// Gap between a row's label and its optional sublabel — a tight caption stack the
/// source pins at a literal 2px, off the `Spacing` scale.
const LABEL_LINE_GAP: Pixels = px(2.0);
/// Gap between a row's icon tile and its label column. The source pins this at a literal
/// 10px, carried off the `Spacing` scale.
const ROW_GAP: Pixels = px(10.0);
/// Alpha of the brand wash a row fills with under the pointer / while pressed.
const ROW_HOVER_ALPHA: f32 = 0.08;

/// Resolves a spacing token at the fixed default density — the picker's bands are chrome
/// sized once, carrying no per-instance density knob (mirrors [`crate::modal`]).
fn pad(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

/// One selectable entry: a stable `id` returned on selection, a `label` (and optional
/// `sublabel`) shown in the row, and the leading `icon`.
#[derive(Debug, Clone)]
pub struct PickerItem {
    pub id: SharedString,
    pub label: SharedString,
    pub sublabel: Option<SharedString>,
    pub icon: Icon,
}

/// The caller-supplied, already-resolved strings the picker renders. The kit carries no
/// localisation, so every visible phrase is passed in.
#[derive(Debug, Clone)]
pub struct PickerLabels {
    /// Header title.
    pub title: SharedString,
    /// Placeholder shown in the empty search field.
    pub placeholder: SharedString,
    /// Message shown centred in place of the list when no item matches the query.
    pub empty: SharedString,
    /// Message shown centred in place of the list while [`Picker::set_loading`] is set.
    pub loading: SharedString,
    /// Footer cancel-button label.
    pub cancel: SharedString,
}

/// Emitted by a [`Picker`] to its subscriber.
#[derive(Debug, Clone)]
pub enum PickerEvent {
    /// The user picked a row; carries that item's [`PickerItem::id`].
    Selected(SharedString),
    /// The user dismissed the picker (Escape in the search field or the cancel button).
    Cancelled,
}

/// Searchable item picker. Owns its embedded [`TextInput`] search field, the full item
/// list, the current query and the derived list of matching item indices. Build one with
/// [`Picker::new`] inside `cx.new(…)`, then feed live data through [`Picker::set_items`] /
/// [`Picker::set_loading`]. Wrap the held entity in a centred [`crate::overlay`] to gain
/// the scrim + dismissal chrome.
pub struct Picker {
    search: Entity<TextInput>,
    items: Vec<PickerItem>,
    /// Indices into `items` that match the current query, in original order.
    filtered: Vec<usize>,
    query: String,
    loading: bool,
    labels: PickerLabels,
    palette: ForgePalette,
    list_scroll: ScrollHandle,
    _search_sub: Subscription,
}

impl EventEmitter<PickerEvent> for Picker {}

impl Picker {
    /// Builds a picker over `items` with the given `labels` and `palette`. Creates the
    /// embedded search field and subscribes to its edits so typing re-derives the match
    /// set and Escape reports a cancel.
    pub fn new(
        labels: PickerLabels,
        items: Vec<PickerItem>,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = labels.placeholder.clone();
        let search = cx.new(|cx| search_input(placeholder, palette, cx));
        let search_sub = cx.subscribe(&search, Self::on_search_event);

        let mut this = Self {
            search,
            items,
            filtered: Vec::new(),
            query: String::new(),
            loading: false,
            labels,
            palette,
            list_scroll: ScrollHandle::new(),
            _search_sub: search_sub,
        };
        this.recompute();
        this
    }

    /// Replaces the item list (e.g. once an async load resolves) and re-derives the match
    /// set against the current query.
    pub fn set_items(&mut self, items: Vec<PickerItem>, cx: &mut Context<Self>) {
        self.items = items;
        self.recompute();
        cx.notify();
    }

    /// Toggles the loading placeholder, shown centred in place of the list.
    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
    }

    /// Re-themes the picker and its embedded search field.
    pub fn set_palette(&mut self, palette: ForgePalette, cx: &mut Context<Self>) {
        self.palette = palette;
        self.search.update(cx, |input, cx| {
            input.set_palette(palette, cx);
            input.set_static_chrome(Some((palette.border_regular, Radius::Sm)));
        });
        cx.notify();
    }

    /// Focuses the embedded search field so typing and Escape reach it. The caller invokes
    /// this when the picker opens (gpui delivers key events only down the focus path).
    pub fn focus(&self, window: &mut Window, cx: &App) {
        self.search.read(cx).focus(window);
    }

    fn on_search_event(
        &mut self,
        _search: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Changed(text) => {
                self.query = text.to_string();
                self.recompute();
                cx.notify();
            }
            InputEvent::Cancelled => cx.emit(PickerEvent::Cancelled),
            InputEvent::Submitted(_) => {}
        }
    }

    fn recompute(&mut self) {
        let query = self.query.clone();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item_matches(&item.label, item.sublabel.as_deref().map(|s| &**s), &query)
            })
            .map(|(idx, _)| idx)
            .collect();
    }

    fn emit_selected(&mut self, id: SharedString, cx: &mut Context<Self>) {
        cx.emit(PickerEvent::Selected(id));
    }

    fn cancel(&mut self, _event: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PickerEvent::Cancelled);
    }

    /// Builds one result row: a leading icon tile, the label over an optional sublabel,
    /// and a whole-row brand-wash hover, reporting the item's id on click.
    fn render_item(&self, idx: usize, item: &PickerItem, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let id = item.id.clone();

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(ICON_TILE)
            .rounded(radius(Radius::Sm))
            .bg(p.surface_overlay)
            .child(icon(item.icon, ICON_TILE_GLYPH, p.text_secondary));

        let mut labels = div().flex().flex_col().gap(LABEL_LINE_GAP).child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(p.text_primary)
                .child(item.label.clone()),
        );
        if let Some(sub) = item.sublabel.clone() {
            labels = labels.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(p.text_muted)
                    .child(sub),
            );
        }

        let hover_bg = with_alpha(p.brand, ROW_HOVER_ALPHA);

        div()
            .id(("forge-picker-row", idx))
            .w_full()
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .py(pad(Spacing::Xs))
            .px(pad(Spacing::Md))
            .rounded(radius(Radius::Sm))
            .text_color(p.text_primary)
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .active(move |style| style.bg(hover_bg))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.emit_selected(id.clone(), cx);
            }))
            .child(tile)
            .child(div().flex_1().child(labels))
            .into_any_element()
    }
}

/// True when `query` matches the item: an empty query matches every item, otherwise a
/// case-insensitive substring hit on the label OR (when present) the sublabel.
pub(crate) fn item_matches(label: &str, sublabel: Option<&str>, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let needle = query.to_lowercase();
    label.to_lowercase().contains(&needle)
        || sublabel.is_some_and(|s| s.to_lowercase().contains(&needle))
}

/// A centred single-line message filling a fixed-height slot — the loading and
/// no-matches placeholders that stand in for the list.
fn centered_message(text: SharedString, height: Pixels, palette: ForgePalette) -> AnyElement {
    div()
        .w_full()
        .h(height)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(text),
        )
        .into_any_element()
}

impl Render for Picker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;

        let header = div()
            .w_full()
            .py(pad(Spacing::Md))
            .px(pad(Spacing::Md))
            .border(BORDER_THIN)
            .border_color(p.border_regular)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_MD)
                    .text_color(p.text_primary)
                    .child(self.labels.title.clone()),
            );

        let search_band = div()
            .w_full()
            .py(pad(Spacing::Sm))
            .px(pad(Spacing::Md))
            .border(BORDER_THIN)
            .border_color(p.border_regular)
            .child(self.search.clone());

        let list_area = if self.loading {
            centered_message(self.labels.loading.clone(), LOADING_HEIGHT, p)
        } else if self.filtered.is_empty() {
            centered_message(self.labels.empty.clone(), EMPTY_HEIGHT, p)
        } else {
            let mut list = div().flex().flex_col();
            for &idx in &self.filtered {
                list = list.child(self.render_item(idx, &self.items[idx], cx));
            }
            div()
                .id("forge-picker-list")
                .track_scroll(&self.list_scroll)
                .overflow_y_scroll()
                .h(LIST_HEIGHT)
                .child(list)
                .into_any_element()
        };

        let footer = div()
            .w_full()
            .py(pad(Spacing::Sm))
            .px(pad(Spacing::Md))
            .border(BORDER_THIN)
            .border_color(p.border_regular)
            .flex()
            .items_center()
            .justify_end()
            .child(
                secondary_button(self.labels.cancel.clone(), &p)
                    .on_click("forge-picker-cancel", cx.listener(Self::cancel)),
            );

        // `overflow_hidden` clips the full-bleed bands to the rounded card so their square
        // corners do not poke past the `Radius::Lg` edge (the modal card's clip pattern).
        div()
            .flex()
            .flex_col()
            .w(CARD_WIDTH)
            .bg(p.elevated)
            .rounded(radius(Radius::Lg))
            .overflow_hidden()
            .border(BORDER_THIN)
            .border_color(p.border_regular)
            .child(header)
            .child(search_band)
            .child(list_area)
            .child(footer)
    }
}
