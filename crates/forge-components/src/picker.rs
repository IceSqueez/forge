use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Pixels, Render, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, Subscription, UniformListScrollHandle, Window, actions,
    div, px, uniform_list,
};

use crate::buttons::secondary_button;
use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::search_state::SearchState;
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_MD, FONT_SM, Radius, Spacing, radius, spacing,
};

const CARD_WIDTH: Pixels = px(480.0);
const LIST_HEIGHT: Pixels = px(320.0);
const LOADING_HEIGHT: Pixels = px(200.0);
const EMPTY_HEIGHT: Pixels = px(120.0);
const ICON_TILE: Pixels = px(28.0);
const ICON_TILE_GLYPH: Pixels = px(14.0);
const LABEL_LINE_GAP: Pixels = px(2.0);
const ROW_GAP: Pixels = px(10.0);
const ROW_HOVER_ALPHA: f32 = 0.08;
const ROW_SELECTED_ALPHA: f32 = 0.14;

pub const PICKER_CONTEXT: &str = "ForgePicker";

actions!(forge_picker, [SelectNext, SelectPrev]);

pub fn bind_picker_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(PICKER_CONTEXT)),
        KeyBinding::new("up", SelectPrev, Some(PICKER_CONTEXT)),
    ]);
}

fn pad(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

#[derive(Debug, Clone)]
pub struct PickerItem {
    pub id: SharedString,
    pub label: SharedString,
    pub sublabel: Option<SharedString>,
    pub icon: Icon,
}

#[derive(Debug, Clone)]
pub struct PickerLabels {
    pub title: SharedString,
    pub placeholder: SharedString,
    pub empty: SharedString,
    pub loading: SharedString,
    pub cancel: SharedString,
}

#[derive(Debug, Clone)]
pub enum PickerEvent {
    Selected(SharedString),
    Cancelled,
}

pub struct Picker {
    search: SearchState,
    items: Vec<PickerItem>,
    /// Indices into `items` that match the current query, in original order.
    filtered: Vec<usize>,
    /// Highlighted position within `filtered` (not an index into `items`).
    selected: usize,
    loading: bool,
    labels: PickerLabels,
    palette: ForgePalette,
    list_scroll: UniformListScrollHandle,
    _search_sub: Subscription,
}

impl EventEmitter<PickerEvent> for Picker {}

impl Picker {
    pub fn new(
        labels: PickerLabels,
        items: Vec<PickerItem>,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = labels.placeholder.clone();
        let search = SearchState::new(cx, palette, placeholder);
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);

        let mut this = Self {
            search,
            items,
            filtered: Vec::new(),
            selected: 0,
            loading: false,
            labels,
            palette,
            list_scroll: UniformListScrollHandle::new(),
            _search_sub: search_sub,
        };
        this.recompute();
        this
    }

    pub fn set_items(&mut self, items: Vec<PickerItem>, cx: &mut Context<Self>) {
        self.items = items;
        self.recompute();
        cx.notify();
    }

    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
    }

    pub fn set_palette(&mut self, palette: ForgePalette, cx: &mut Context<Self>) {
        self.palette = palette;
        self.search.field().update(cx, |input, cx| {
            input.set_palette(palette, cx);
            input.set_static_chrome(Some((palette.border_regular, Radius::Sm)));
        });
        cx.notify();
    }

    /// The caller must call this when the picker opens; gpui delivers key events only down
    /// the focus path, so without it typing and Escape never reach the search field.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.search.field().update(cx, |f, cx| f.focus(window, cx));
    }

    fn on_search_event(
        &mut self,
        _search: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Changed(_) => {
                self.search.on_changed(event);
                self.recompute();
                cx.notify();
            }
            InputEvent::Cancelled => cx.emit(PickerEvent::Cancelled),
            InputEvent::Submitted(_) => self.confirm_selected(cx),
        }
    }

    fn recompute(&mut self) {
        let query = self.search.query();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item_matches(&item.label, item.sublabel.as_deref(), query))
            .map(|(idx, _)| idx)
            .collect();
        self.selected = 0;
    }

    fn confirm_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(&idx) = self.filtered.get(self.selected) {
            let id = self.items[idx].id.clone();
            self.emit_selected(id, cx);
        }
    }

    fn select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = if self.selected + 1 >= self.filtered.len() {
            0
        } else {
            self.selected + 1
        };
        self.list_scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn select_prev(&mut self, _: &SelectPrev, _window: &mut Window, cx: &mut Context<Self>) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.filtered.len() - 1
        } else {
            self.selected - 1
        };
        self.list_scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn emit_selected(&mut self, id: SharedString, cx: &mut Context<Self>) {
        cx.emit(PickerEvent::Selected(id));
    }

    fn cancel(&mut self, _event: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PickerEvent::Cancelled);
    }

    fn render_item(
        &self,
        idx: usize,
        item: &PickerItem,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

        let mut labels = div().flex().flex_col().min_w_0().gap(LABEL_LINE_GAP).child(
            div()
                .truncate()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(p.text_primary)
                .child(item.label.clone()),
        );
        if let Some(sub) = item.sublabel.clone() {
            labels = labels.child(
                div()
                    .truncate()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(p.text_muted)
                    .child(sub),
            );
        }

        let hover_bg = with_alpha(p.brand, ROW_HOVER_ALPHA);

        let mut row = div()
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
            .child(div().flex_1().min_w_0().overflow_hidden().child(labels));
        if is_selected {
            row = row.bg(with_alpha(p.brand, ROW_SELECTED_ALPHA));
        }
        row.into_any_element()
    }
}

pub(crate) fn item_matches(label: &str, sublabel: Option<&str>, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let needle = query.to_lowercase();
    label.to_lowercase().contains(&needle)
        || sublabel.is_some_and(|s| s.to_lowercase().contains(&needle))
}

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
            .child(self.search.field().clone());

        let list_area = if self.loading {
            centered_message(self.labels.loading.clone(), LOADING_HEIGHT, p)
        } else if self.filtered.is_empty() {
            centered_message(self.labels.empty.clone(), EMPTY_HEIGHT, p)
        } else {
            let count = self.filtered.len();
            uniform_list(
                "forge-picker-list",
                count,
                cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                    let mut rows = Vec::with_capacity(range.len());
                    for pos in range {
                        let Some(&idx) = this.filtered.get(pos) else {
                            continue;
                        };
                        let item = this.items[idx].clone();
                        rows.push(this.render_item(idx, &item, pos == this.selected, cx));
                    }
                    rows
                }),
            )
            .track_scroll(&self.list_scroll)
            .h(LIST_HEIGHT)
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

        div()
            .key_context(PICKER_CONTEXT)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_prev))
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

#[cfg(test)]
mod tests {
    use super::item_matches;

    #[test]
    fn item_matches_follows_case_insensitive_substring_over_label_or_sublabel() {
        let cases = [
            ("OBS Scene", None, "", true),
            ("OBS Scene", None, "scene", true),
            ("OBS Scene", None, "SCENE", true),
            // The needle lives only in the sublabel; the label alone would miss.
            ("Start", Some("obs.start.recording"), "record", true),
            ("Start", Some("obs.start.recording"), "zzz", false),
            // Absent sublabel with a non-matching label returns false without panicking.
            ("Start", None, "stop", false),
            // The hit sits mid-string ("connect" inside "Reconnect"), pinning substring -
            // not starts_with - semantics.
            ("Reconnect", None, "connect", true),
        ];

        for (label, sublabel, query, expected) in cases {
            assert_eq!(
                item_matches(label, sublabel, query),
                expected,
                "item_matches({label:?}, {sublabel:?}, {query:?})",
            );
        }
    }
}
