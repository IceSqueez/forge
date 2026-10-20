use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, Rgba, ScrollStrategy, SharedString, Styled,
    Subscription, UniformListScrollHandle, Window, div, uniform_list,
};

use crate::dropdown::{
    DROPDOWN_PADDING, dropdown_group_header, dropdown_list_height, dropdown_row, dropdown_surface,
};
use crate::icons::Icon;
use crate::palette::ForgePalette;
use crate::picker::{PICKER_CONTEXT, SelectNext, SelectPrev, item_matches, message_row};
use crate::search_state::SearchState;
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{body_family, mono_family};

const ENTER_KEY: &str = "enter";
const ESCAPE_KEY: &str = "escape";
const LEFT_KEY: &str = "left";

#[derive(Clone, Debug, PartialEq)]
pub struct VariableFormat {
    pub pattern: Option<SharedString>,
    pub preview: SharedString,
    pub token: SharedString,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariableEntry {
    pub name: SharedString,
    pub token: SharedString,
    pub tag: Option<(SharedString, Rgba)>,
    pub formats: Vec<VariableFormat>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariableGroup {
    pub title: SharedString,
    pub entries: Vec<VariableEntry>,
}

#[derive(Clone, Debug)]
pub struct VariablePickerLabels {
    pub placeholder: SharedString,
    pub empty: SharedString,
    pub no_format: SharedString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariablePickerEvent {
    Picked(SharedString),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerRow {
    Header(usize),
    Entry(usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OpenFormats {
    group: usize,
    entry: usize,
    selected: usize,
}

pub struct VariablePicker {
    search: SearchState,
    groups: Vec<VariableGroup>,
    rows: Vec<PickerRow>,
    selected: Option<usize>,
    formats: Option<OpenFormats>,
    labels: VariablePickerLabels,
    palette: ForgePalette,
    list_scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    _search_sub: Subscription,
}

impl EventEmitter<VariablePickerEvent> for VariablePicker {}

impl Focusable for VariablePicker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl VariablePicker {
    pub fn new(
        labels: VariablePickerLabels,
        groups: Vec<VariableGroup>,
        palette: ForgePalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = SearchState::new(cx, palette, labels.placeholder.clone());
        let search_sub = cx.subscribe_in(search.field(), window, Self::on_search_event);
        let mut picker = Self {
            search,
            groups,
            rows: Vec::new(),
            selected: None,
            formats: None,
            labels,
            palette,
            list_scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _search_sub: search_sub,
        };
        picker.recompute();
        picker
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if self.formats.is_some() {
            window.focus(&self.focus_handle, cx);
        } else {
            self.search
                .field()
                .update(cx, |field, cx| field.focus(window, cx));
        }
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = self.formats {
            let token = self
                .entry(open.group, open.entry)
                .and_then(|entry| entry.formats.get(open.selected))
                .map(|format| format.token.clone());
            if let Some(token) = token {
                cx.emit(VariablePickerEvent::Picked(token));
            }
            return;
        }
        let Some(row) = self.selected.and_then(|pos| self.rows.get(pos).copied()) else {
            return;
        };
        if let PickerRow::Entry(group, entry) = row {
            self.choose_entry(group, entry, window, cx);
        }
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.formats.take().is_some() {
            self.focus(window, cx);
            cx.notify();
        }
    }

    fn choose_entry(
        &mut self,
        group: usize,
        entry: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chosen) = self.entry(group, entry) else {
            return;
        };
        if chosen.formats.is_empty() {
            cx.emit(VariablePickerEvent::Picked(chosen.token.clone()));
            return;
        }
        self.formats = Some(OpenFormats {
            group,
            entry,
            selected: 0,
        });
        self.focus(window, cx);
        cx.notify();
    }

    fn entry(&self, group: usize, entry: usize) -> Option<&VariableEntry> {
        self.groups.get(group)?.entries.get(entry)
    }

    fn recompute(&mut self) {
        let query = self.search.query();
        self.rows.clear();
        for (group_index, group) in self.groups.iter().enumerate() {
            let matching: Vec<usize> = group
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| item_matches(&entry.name, None, query))
                .map(|(index, _)| index)
                .collect();
            if matching.is_empty() {
                continue;
            }
            self.rows.push(PickerRow::Header(group_index));
            self.rows.extend(
                matching
                    .into_iter()
                    .map(|entry_index| PickerRow::Entry(group_index, entry_index)),
            );
        }
        self.selected = self
            .rows
            .iter()
            .position(|row| matches!(row, PickerRow::Entry(..)));
        if let Some(pos) = self.selected {
            self.list_scroll
                .scroll_to_item(pos, ScrollStrategy::Nearest);
        }
    }

    fn step_selection(&mut self, forward: bool) {
        if let Some(open) = self.formats.as_mut() {
            let count = self
                .groups
                .get(open.group)
                .and_then(|group| group.entries.get(open.entry))
                .map_or(0, |entry| entry.formats.len());
            if count > 0 {
                open.selected = wrapped_step(open.selected, count, forward);
            }
            return;
        }
        let entries: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, PickerRow::Entry(..)))
            .map(|(pos, _)| pos)
            .collect();
        if entries.is_empty() {
            return;
        }
        let current = self
            .selected
            .and_then(|pos| entries.iter().position(|&entry_pos| entry_pos == pos))
            .unwrap_or(0);
        let next = entries[wrapped_step(current, entries.len(), forward)];
        self.selected = Some(next);
        self.list_scroll
            .scroll_to_item(next, ScrollStrategy::Nearest);
    }

    fn on_search_event(
        &mut self,
        _search: &Entity<TextInput>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Changed(_) => {
                self.search.on_changed(event);
                self.recompute();
                cx.notify();
            }
            InputEvent::Cancelled => cx.emit(VariablePickerEvent::Cancelled),
            InputEvent::Submitted(_) => self.confirm(window, cx),
            InputEvent::Blurred(_) => {}
        }
    }

    fn on_select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        self.step_selection(true);
        cx.notify();
    }

    fn on_select_prev(&mut self, _: &SelectPrev, _window: &mut Window, cx: &mut Context<Self>) {
        self.step_selection(false);
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            ENTER_KEY => {
                cx.stop_propagation();
                self.confirm(window, cx);
            }
            ESCAPE_KEY => {
                cx.stop_propagation();
                if self.formats.is_some() {
                    self.back(window, cx);
                } else {
                    cx.emit(VariablePickerEvent::Cancelled);
                }
            }
            LEFT_KEY if self.formats.is_some() => {
                cx.stop_propagation();
                self.back(window, cx);
            }
            _ => {}
        }
    }

    fn render_row(&self, pos: usize, row: PickerRow, cx: &mut Context<Self>) -> AnyElement {
        match row {
            PickerRow::Header(group) => dropdown_group_header(
                self.groups
                    .get(group)
                    .map(|group| group.title.clone())
                    .unwrap_or_default(),
                &self.palette,
            )
            .into_any_element(),
            PickerRow::Entry(group, entry) => {
                let Some(variable) = self.entry(group, entry) else {
                    return div().into_any_element();
                };
                let trailing = (!variable.formats.is_empty()).then_some(Icon::ChevronRight);
                dropdown_row(
                    ("forge-variable-row", pos),
                    variable.token.clone(),
                    &self.palette,
                )
                .label_family(mono_family())
                .tag(variable.tag.clone())
                .trailing_icon(trailing)
                .highlighted(self.selected == Some(pos))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.choose_entry(group, entry, window, cx);
                }))
                .into_any_element()
            }
        }
    }

    fn render_main(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.rows.is_empty() {
            return message_row(self.labels.empty.clone(), self.palette);
        }
        let count = self.rows.len();
        uniform_list(
            "forge-variable-list",
            count,
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let mut rows = Vec::with_capacity(range.len());
                for pos in range {
                    if let Some(&row) = this.rows.get(pos) {
                        rows.push(this.render_row(pos, row, cx));
                    }
                }
                rows
            }),
        )
        .track_scroll(&self.list_scroll)
        .w_full()
        .h(dropdown_list_height(count))
        .into_any_element()
    }

    fn render_formats(&self, open: OpenFormats, cx: &mut Context<Self>) -> AnyElement {
        let Some(variable) = self.entry(open.group, open.entry) else {
            return div().into_any_element();
        };
        let back = dropdown_row("forge-variable-back", variable.token.clone(), &self.palette)
            .icon(Some(Icon::ChevronLeft))
            .label_family(mono_family())
            .tag(variable.tag.clone())
            .on_click(cx.listener(|this, _event, window, cx| this.back(window, cx)));
        let rows = variable.formats.iter().enumerate().map(|(index, format)| {
            let label = format
                .pattern
                .clone()
                .unwrap_or_else(|| self.labels.no_format.clone());
            let family = if format.pattern.is_some() {
                mono_family()
            } else {
                body_family()
            };
            let token = format.token.clone();
            dropdown_row(("forge-variable-format", index), label, &self.palette)
                .label_family(family)
                .suffix(Some(format.preview.clone()))
                .highlighted(open.selected == index)
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    cx.emit(VariablePickerEvent::Picked(token.clone()));
                }))
        });
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(back)
            .children(rows)
            .into_any_element()
    }
}

fn wrapped_step(current: usize, count: usize, forward: bool) -> usize {
    if forward {
        (current + 1) % count
    } else if current == 0 {
        count - 1
    } else {
        current - 1
    }
}

impl Render for VariablePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surface = dropdown_surface(&self.palette)
            .key_context(PICKER_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_select_next))
            .on_action(cx.listener(Self::on_select_prev))
            .on_key_down(cx.listener(Self::on_key_down));
        match self.formats {
            Some(open) => surface.child(self.render_formats(open, cx)),
            None => surface
                .child(
                    div()
                        .w_full()
                        .pb(DROPDOWN_PADDING)
                        .child(self.search.field().clone()),
                )
                .child(self.render_main(cx)),
        }
    }
}
