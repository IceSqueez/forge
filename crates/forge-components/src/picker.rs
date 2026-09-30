use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, KeyDownEvent, ParentElement, Pixels, Render, ScrollStrategy,
    SharedString, Styled, Subscription, UniformListScrollHandle, Window, actions, div, px,
    uniform_list,
};

use crate::dropdown::{
    DROPDOWN_MAX_HEIGHT, DROPDOWN_PADDING, DROPDOWN_ROW_HEIGHT, dropdown_row, dropdown_surface,
};
use crate::icons::Icon;
use crate::palette::ForgePalette;
use crate::search_state::SearchState;
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{Radius, body_family};

const SEARCH_THRESHOLD: usize = 10;
const MESSAGE_FONT: Pixels = px(12.5);
const ENTER_KEY: &str = "enter";
const ESCAPE_KEY: &str = "escape";

pub const PICKER_CONTEXT: &str = "ForgePicker";

actions!(forge_picker, [SelectNext, SelectPrev]);

pub fn bind_picker_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(PICKER_CONTEXT)),
        KeyBinding::new("up", SelectPrev, Some(PICKER_CONTEXT)),
    ]);
}

#[derive(Debug, Clone)]
pub struct PickerItem {
    pub id: SharedString,
    pub label: SharedString,
    pub sublabel: Option<SharedString>,
    pub icon: Option<Icon>,
}

#[derive(Debug, Clone)]
pub struct PickerLabels {
    pub placeholder: SharedString,
    pub empty: SharedString,
    pub loading: SharedString,
}

#[derive(Debug, Clone)]
pub enum PickerEvent {
    Selected(SharedString),
    Cancelled,
}

pub struct Picker {
    search: SearchState,
    items: Vec<PickerItem>,
    filtered: Vec<usize>,
    selected: usize,
    current: Option<SharedString>,
    loading: bool,
    labels: PickerLabels,
    palette: ForgePalette,
    list_scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    _search_sub: Subscription,
}

impl EventEmitter<PickerEvent> for Picker {}

impl Focusable for Picker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

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
            current: None,
            loading: false,
            labels,
            palette,
            list_scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _search_sub: search_sub,
        };
        this.recompute();
        this
    }

    #[must_use]
    pub fn with_current(mut self, current: Option<SharedString>) -> Self {
        self.current = current;
        self.highlight_current();
        self
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

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if self.shows_search() {
            self.search.field().update(cx, |f, cx| f.focus(window, cx));
        } else {
            window.focus(&self.focus_handle, cx);
        }
    }

    fn shows_search(&self) -> bool {
        self.items.len() > SEARCH_THRESHOLD
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
            InputEvent::Blurred(_) => {}
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
        if query.is_empty() {
            self.highlight_current();
        }
    }

    fn highlight_current(&mut self) {
        let Some(current) = self.current.as_ref() else {
            return;
        };
        if let Some(pos) = self
            .filtered
            .iter()
            .position(|&idx| self.items[idx].id == *current)
        {
            self.selected = pos;
            self.list_scroll
                .scroll_to_item(pos, ScrollStrategy::Nearest);
        }
    }

    fn confirm_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(&idx) = self.filtered.get(self.selected) {
            let id = self.items[idx].id.clone();
            cx.emit(PickerEvent::Selected(id));
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

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            ENTER_KEY => {
                cx.stop_propagation();
                self.confirm_selected(cx);
            }
            ESCAPE_KEY => {
                cx.stop_propagation();
                cx.emit(PickerEvent::Cancelled);
            }
            _ => {}
        }
    }

    fn render_item(
        &self,
        pos: usize,
        idx: usize,
        item: &PickerItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = item.id.clone();
        let is_current = self.current.as_ref() == Some(&item.id);
        dropdown_row(("forge-picker-row", idx), item.label.clone(), &self.palette)
            .icon(item.icon)
            .suffix(item.sublabel.clone())
            .current(is_current)
            .highlighted(pos == self.selected)
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                cx.emit(PickerEvent::Selected(id.clone()));
            }))
            .into_any_element()
    }

    fn list_height(&self, count: usize) -> Pixels {
        let natural = DROPDOWN_ROW_HEIGHT * count as f32;
        let ceiling = DROPDOWN_MAX_HEIGHT - DROPDOWN_PADDING * 2.0;
        if natural < ceiling { natural } else { ceiling }
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

fn message_row(text: SharedString, palette: ForgePalette) -> AnyElement {
    div()
        .w_full()
        .h(DROPDOWN_ROW_HEIGHT)
        .flex()
        .items_center()
        .justify_center()
        .font_family(body_family())
        .text_size(MESSAGE_FONT)
        .text_color(palette.text_faint)
        .child(text)
        .into_any_element()
}

impl Render for Picker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;

        let list_area = if self.loading {
            message_row(self.labels.loading.clone(), p)
        } else if self.filtered.is_empty() {
            message_row(self.labels.empty.clone(), p)
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
                        rows.push(this.render_item(pos, idx, &item, cx));
                    }
                    rows
                }),
            )
            .track_scroll(&self.list_scroll)
            .w_full()
            .h(self.list_height(count))
            .into_any_element()
        };

        let mut surface = dropdown_surface(&p)
            .key_context(PICKER_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_prev))
            .on_key_down(cx.listener(Self::on_key_down));
        if self.shows_search() {
            surface = surface.child(
                div()
                    .w_full()
                    .pb(DROPDOWN_PADDING)
                    .child(self.search.field().clone()),
            );
        }
        surface.child(list_area)
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
            ("Start", Some("obs.start.recording"), "record", true),
            ("Start", Some("obs.start.recording"), "zzz", false),
            ("Start", None, "stop", false),
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
