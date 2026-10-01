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
    custom_entry: Option<SharedString>,
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
            custom_entry: None,
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

    #[must_use]
    pub fn with_custom_entry(mut self, hint: SharedString) -> Self {
        self.custom_entry = Some(hint);
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
        self.custom_entry.is_some() || self.items.len() > SEARCH_THRESHOLD
    }

    fn custom_value(&self, cx: &App) -> Option<SharedString> {
        self.custom_entry.as_ref()?;
        let typed = self.search.field().read(cx).content().trim();
        if typed.is_empty() || self.items.iter().any(|item| item.id.as_ref() == typed) {
            return None;
        }
        Some(SharedString::from(typed.to_owned()))
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
            .filter(|(_, item)| {
                item_matches(&item.label, item.sublabel.as_deref(), query)
                    || item.id.as_ref() == query.trim()
            })
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
        } else if let Some(typed) = self.custom_value(cx) {
            cx.emit(PickerEvent::Selected(typed));
        }
    }

    fn render_custom_row(
        &self,
        typed: SharedString,
        hint: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = typed.clone();
        dropdown_row("forge-picker-custom", typed, &self.palette)
            .icon(Some(Icon::Pencil))
            .suffix(hint)
            .highlighted(self.filtered.is_empty())
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                cx.emit(PickerEvent::Selected(value.clone()));
            }))
            .into_any_element()
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
        let custom_row = self
            .custom_value(cx)
            .map(|typed| self.render_custom_row(typed, self.custom_entry.clone(), cx));

        let list_area = if self.loading {
            message_row(self.labels.loading.clone(), p)
        } else if self.filtered.is_empty() && custom_row.is_some() {
            div().into_any_element()
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
        surface.children(custom_row).child(list_area)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use gpui::{AppContext, Modifiers, VisualTestContext, point};

    use super::*;
    use crate::palette::FORGE_DEFAULT;

    struct Heard {
        events: Vec<PickerEvent>,
        _sub: Subscription,
    }

    fn items(count: usize) -> Vec<PickerItem> {
        (0..count)
            .map(|n| PickerItem {
                id: SharedString::from(format!("id-{n}")),
                label: SharedString::from(format!("Item {n}")),
                sublabel: None,
                icon: None,
            })
            .collect()
    }

    fn labels() -> PickerLabels {
        PickerLabels {
            placeholder: "search".into(),
            empty: "empty".into(),
            loading: "loading".into(),
        }
    }

    fn open<'a>(
        cx: &'a mut gpui::TestAppContext,
        roster: Vec<PickerItem>,
        current: Option<&str>,
    ) -> (Entity<Picker>, Entity<Heard>, &'a mut VisualTestContext) {
        cx.update(bind_picker_keys);
        let current = current.map(|id| SharedString::from(id.to_owned()));
        let (picker, vcx) = cx.add_window_view(|_window, cx| {
            Picker::new(labels(), roster, FORGE_DEFAULT, cx).with_current(current)
        });
        let heard = vcx.update(|window, cx| {
            picker.update(cx, |view, cx| view.focus(window, cx));
            cx.new(|cx| Heard {
                events: Vec::new(),
                _sub: cx.subscribe(&picker, |this: &mut Heard, _view, event, _cx| {
                    this.events.push(event.clone());
                }),
            })
        });
        vcx.run_until_parked();
        (picker, heard, vcx)
    }

    fn highlighted(picker: &Entity<Picker>, vcx: &mut VisualTestContext) -> Option<String> {
        vcx.update(|_window, cx| {
            let view = picker.read(cx);
            view.filtered
                .get(view.selected)
                .map(|&idx| view.items[idx].id.to_string())
        })
    }

    fn visible_ids(picker: &Entity<Picker>, vcx: &mut VisualTestContext) -> Vec<String> {
        vcx.update(|_window, cx| {
            let view = picker.read(cx);
            view.filtered
                .iter()
                .map(|&idx| view.items[idx].id.to_string())
                .collect()
        })
    }

    fn selections(heard: &Entity<Heard>, vcx: &mut VisualTestContext) -> Vec<String> {
        vcx.update(|_window, cx| {
            heard
                .read(cx)
                .events
                .iter()
                .filter_map(|event| match event {
                    PickerEvent::Selected(id) => Some(id.to_string()),
                    PickerEvent::Cancelled => None,
                })
                .collect()
        })
    }

    fn cancellations(heard: &Entity<Heard>, vcx: &mut VisualTestContext) -> usize {
        vcx.update(|_window, cx| {
            heard
                .read(cx)
                .events
                .iter()
                .filter(|event| matches!(event, PickerEvent::Cancelled))
                .count()
        })
    }

    fn search_event(picker: &Entity<Picker>, vcx: &mut VisualTestContext, event: InputEvent) {
        vcx.update(|_window, cx| {
            picker.update(cx, |view, cx| {
                let field = view.search.field().clone();
                view.on_search_event(field, &event, cx);
            });
        });
        vcx.run_until_parked();
    }

    fn type_query(picker: &Entity<Picker>, vcx: &mut VisualTestContext, text: &str) {
        search_event(
            picker,
            vcx,
            InputEvent::Changed(SharedString::from(text.to_owned())),
        );
    }

    #[gpui::test]
    fn arrow_down_walks_the_rows_and_wraps_from_the_last_to_the_first(
        cx: &mut gpui::TestAppContext,
    ) {
        let (picker, _heard, vcx) = open(cx, items(3), None);

        let mut walked = vec![highlighted(&picker, vcx)];
        for _ in 0..3 {
            vcx.simulate_keystrokes("down");
            walked.push(highlighted(&picker, vcx));
        }

        assert_eq!(
            walked,
            ["id-0", "id-1", "id-2", "id-0"].map(|id| Some(id.to_owned())),
        );
    }

    #[gpui::test]
    fn arrow_up_on_the_first_row_wraps_to_the_last(cx: &mut gpui::TestAppContext) {
        let (picker, _heard, vcx) = open(cx, items(3), None);

        vcx.simulate_keystrokes("up");

        assert_eq!(highlighted(&picker, vcx), Some("id-2".to_owned()));
    }

    #[gpui::test]
    fn enter_selects_the_highlighted_row(cx: &mut gpui::TestAppContext) {
        let (_picker, heard, vcx) = open(cx, items(3), None);

        vcx.simulate_keystrokes("down enter");

        assert_eq!(selections(&heard, vcx), vec!["id-1".to_owned()]);
    }

    #[gpui::test]
    fn escape_cancels_without_selecting_anything(cx: &mut gpui::TestAppContext) {
        let (_picker, heard, vcx) = open(cx, items(3), Some("id-1"));

        vcx.simulate_keystrokes("escape");

        assert_eq!(
            (cancellations(&heard, vcx), selections(&heard, vcx)),
            (1, Vec::<String>::new()),
        );
    }

    #[gpui::test]
    fn clicking_a_row_selects_that_row(cx: &mut gpui::TestAppContext) {
        let (_picker, heard, vcx) = open(cx, items(3), None);
        let third_row_middle = DROPDOWN_PADDING + DROPDOWN_ROW_HEIGHT * 2.5;

        vcx.simulate_click(point(px(40.0), third_row_middle), Modifiers::none());

        assert_eq!(selections(&heard, vcx), vec!["id-2".to_owned()]);
    }

    #[gpui::test]
    fn the_search_field_appears_only_for_lists_longer_than_ten(cx: &mut gpui::TestAppContext) {
        for (count, search_focused) in [(10, false), (11, true)] {
            let (picker, _heard, vcx) = open(cx, items(count), None);

            let focus = vcx.update(|window, cx| {
                let view = picker.read(cx);
                let field_focused = view
                    .search
                    .field()
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window);
                (field_focused, view.focus_handle.is_focused(window))
            });

            assert_eq!(
                focus,
                (search_focused, !search_focused),
                "{count} items: (search focused, list focused)",
            );
        }
    }

    #[gpui::test]
    fn the_current_item_opens_highlighted_and_scrolled_into_view(cx: &mut gpui::TestAppContext) {
        let (picker, _heard, vcx) = open(cx, items(20), Some("id-15"));

        let (top, viewport) = vcx.update(|_window, cx| {
            let view = picker.read(cx);
            let top = -view.list_scroll.0.borrow().base_handle.offset().y;
            (top, view.list_height(view.filtered.len()))
        });
        let row_top = DROPDOWN_ROW_HEIGHT * 15.0;
        let row_in_view = top <= row_top && row_top + DROPDOWN_ROW_HEIGHT <= top + viewport;

        assert_eq!(
            (highlighted(&picker, vcx), row_in_view),
            (Some("id-15".to_owned()), true),
            "scroll top {top:?}, viewport {viewport:?}",
        );
    }

    #[gpui::test]
    fn a_current_id_missing_from_the_list_leaves_the_first_row_highlighted(
        cx: &mut gpui::TestAppContext,
    ) {
        let (picker, _heard, vcx) = open(cx, items(5), Some("gone"));

        assert_eq!(highlighted(&picker, vcx), Some("id-0".to_owned()));
    }

    #[gpui::test]
    fn a_query_narrows_the_rows_and_highlights_the_first_match(cx: &mut gpui::TestAppContext) {
        let (picker, _heard, vcx) = open(cx, items(20), Some("id-3"));

        type_query(&picker, vcx, "item 1");

        assert_eq!(
            (visible_ids(&picker, vcx).len(), highlighted(&picker, vcx)),
            (11, Some("id-1".to_owned())),
        );
    }

    #[gpui::test]
    fn clearing_the_query_restores_every_row_and_rehighlights_the_current_item(
        cx: &mut gpui::TestAppContext,
    ) {
        let (picker, _heard, vcx) = open(cx, items(20), Some("id-3"));
        type_query(&picker, vcx, "item 1");

        type_query(&picker, vcx, "");

        assert_eq!(
            (visible_ids(&picker, vcx).len(), highlighted(&picker, vcx)),
            (20, Some("id-3".to_owned())),
        );
    }

    #[gpui::test]
    fn submitting_the_search_selects_the_top_match(cx: &mut gpui::TestAppContext) {
        let (picker, heard, vcx) = open(cx, items(20), None);
        type_query(&picker, vcx, "item 17");

        search_event(&picker, vcx, InputEvent::Submitted(SharedString::default()));

        assert_eq!(selections(&heard, vcx), vec!["id-17".to_owned()]);
    }

    #[gpui::test]
    fn cancelling_the_search_field_cancels_the_picker(cx: &mut gpui::TestAppContext) {
        let (picker, heard, vcx) = open(cx, items(20), None);

        search_event(&picker, vcx, InputEvent::Cancelled);

        assert_eq!(cancellations(&heard, vcx), 1);
    }

    #[gpui::test]
    fn arrows_and_enter_over_a_query_with_no_matches_do_nothing(cx: &mut gpui::TestAppContext) {
        let (picker, heard, vcx) = open(cx, items(20), None);
        type_query(&picker, vcx, "no such row");
        vcx.update(|window, cx| window.focus(&picker.read(cx).focus_handle.clone(), cx));
        vcx.run_until_parked();

        vcx.simulate_keystrokes("down up enter");

        assert_eq!(
            (highlighted(&picker, vcx), selections(&heard, vcx)),
            (None, Vec::<String>::new()),
        );
    }

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

    fn open_typable(
        cx: &mut gpui::TestAppContext,
        roster: Vec<PickerItem>,
    ) -> (Entity<Picker>, Entity<Heard>, &mut VisualTestContext) {
        cx.update(bind_picker_keys);
        cx.update(crate::text_input::bind_text_input_keys);
        let (picker, vcx) = cx.add_window_view(|_window, cx| {
            Picker::new(labels(), roster, FORGE_DEFAULT, cx).with_custom_entry("custom".into())
        });
        let heard = vcx.update(|window, cx| {
            picker.update(cx, |view, cx| view.focus(window, cx));
            cx.new(|cx| Heard {
                events: Vec::new(),
                _sub: cx.subscribe(&picker, |this: &mut Heard, _view, event, _cx| {
                    this.events.push(event.clone());
                }),
            })
        });
        vcx.run_until_parked();
        (picker, heard, vcx)
    }

    #[gpui::test]
    fn a_typable_picker_offers_the_search_field_even_for_a_short_list(
        cx: &mut gpui::TestAppContext,
    ) {
        let (picker, _heard, vcx) = open_typable(cx, items(3));

        let search_focused = vcx.update(|window, cx| {
            picker
                .read(cx)
                .search
                .field()
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        });

        assert!(search_focused);
    }

    #[gpui::test]
    fn enter_on_a_typed_value_with_no_matching_row_selects_the_trimmed_text(
        cx: &mut gpui::TestAppContext,
    ) {
        let (_picker, heard, vcx) = open_typable(cx, items(3));

        vcx.simulate_input("  %reward.id%  ");
        vcx.simulate_keystrokes("enter");

        assert_eq!(selections(&heard, vcx), vec!["%reward.id%".to_owned()]);
    }

    #[gpui::test]
    fn enter_on_a_typed_value_that_matches_a_row_selects_that_row(cx: &mut gpui::TestAppContext) {
        let (_picker, heard, vcx) = open_typable(cx, items(3));

        vcx.simulate_input("item 2");
        vcx.simulate_keystrokes("enter");

        assert_eq!(selections(&heard, vcx), vec!["id-2".to_owned()]);
    }

    fn rewards() -> Vec<PickerItem> {
        [("7c1e2f90-aaaa", "Hydrate"), ("7c1e2f90-bbbb", "Stretch")]
            .into_iter()
            .map(|(id, label)| PickerItem {
                id: SharedString::from(id),
                label: SharedString::from(label),
                sublabel: None,
                icon: None,
            })
            .collect()
    }

    #[gpui::test]
    fn enter_on_a_pasted_row_id_selects_that_row(cx: &mut gpui::TestAppContext) {
        let (picker, heard, vcx) = open_typable(cx, rewards());

        type_query(&picker, vcx, " 7c1e2f90-bbbb ");
        search_event(&picker, vcx, InputEvent::Submitted(SharedString::default()));

        assert_eq!(
            (visible_ids(&picker, vcx), selections(&heard, vcx)),
            (
                vec!["7c1e2f90-bbbb".to_owned()],
                vec!["7c1e2f90-bbbb".to_owned()],
            ),
        );
    }

    #[gpui::test]
    fn a_partial_row_id_matches_no_row(cx: &mut gpui::TestAppContext) {
        let (picker, _heard, vcx) = open_typable(cx, rewards());

        type_query(&picker, vcx, "7c1e2f90");

        assert_eq!(visible_ids(&picker, vcx), Vec::<String>::new());
    }

    #[gpui::test]
    fn enter_on_whitespace_alone_over_an_empty_list_selects_nothing(cx: &mut gpui::TestAppContext) {
        let (_picker, heard, vcx) = open_typable(cx, Vec::new());

        vcx.simulate_input("   ");
        vcx.simulate_keystrokes("enter");

        assert_eq!(selections(&heard, vcx), Vec::<String>::new());
    }
}
