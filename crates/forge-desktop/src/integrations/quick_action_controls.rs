use std::collections::BTreeMap;

use forge_components::{
    ActivePresentation, FONT_FIELD, FONT_XS, ForgePalette, Icon, InputEvent, Picker, PickerEvent,
    PickerItem, PickerLabels, TextArea, TextInput, choice_trigger, dropdown, failure_note,
    failure_with_retry, field_column, field_frame, int_entry_invalid, int_range_hint,
    parse_int_in_range, scaled, spinner, toggle_row, tr,
};
use forge_platform_core::{
    OptionListId, OptionListMetadata, QuickActionChoiceOption, QuickActionChoiceSource,
    QuickActionField, QuickActionFieldKind, QuickActionFieldValue,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};

use crate::shared::user_text::user_text_string;

const MULTILINE_H: Pixels = px(80.0);
const CHOICE_GAP: Pixels = px(8.0);

pub(crate) enum ChoiceState {
    Loading,
    Ready(Vec<PickerItem>),
    Failed(String),
}

pub(crate) struct OptionListBinding {
    pub(crate) list: OptionListId,
    pub(crate) depends_on: Vec<String>,
    pub(crate) default: Option<SharedString>,
    pub(crate) ticket: u64,
}

pub(crate) struct ChoiceControl {
    pub(crate) option_list: Option<OptionListBinding>,
    pub(crate) state: ChoiceState,
    pub(crate) selected: Option<SharedString>,
}

pub(crate) enum FieldControl {
    Text(Entity<TextInput>),
    Multiline(Entity<TextArea>),
    Toggle(bool),
    Choice(ChoiceControl),
    Int {
        input: Entity<TextInput>,
        min: i64,
        max: i64,
        fallback: i64,
    },
}

pub(crate) struct QuickField {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) hint: Option<String>,
    pub(crate) required: bool,
    pub(crate) max_chars: Option<usize>,
    pub(crate) error: Option<String>,
    pub(crate) control: FieldControl,
}

pub(crate) struct QuickFieldSeed<'a> {
    pub(crate) spec: &'a QuickActionField,
    pub(crate) initial: Option<&'a QuickActionFieldValue>,
    pub(crate) max_chars: Option<usize>,
}

pub(crate) struct OptionListSupport<'a> {
    pub(crate) declared: &'a [OptionListMetadata],
    pub(crate) unavailable: String,
}

pub(crate) struct OpenChoice {
    pub(crate) index: usize,
    pub(crate) picker: Entity<Picker>,
    pub(crate) _sub: Subscription,
}

#[derive(Clone, Copy)]
pub(crate) struct QuickFieldIds {
    pub(crate) toggle: &'static str,
    pub(crate) choice: &'static str,
    pub(crate) spinner: &'static str,
    pub(crate) retry: &'static str,
}

pub(crate) struct QuickFieldHandlers<V: 'static> {
    pub(crate) toggle: fn(&mut V, usize, &mut Context<V>),
    pub(crate) open: fn(&mut V, usize, &mut Window, &mut Context<V>),
    pub(crate) close: fn(&mut V, &mut Context<V>),
    pub(crate) retry: Option<fn(&mut V, usize, &mut Context<V>)>,
}

type FieldEventHandler<V> = fn(&mut V, usize, &InputEvent, &mut Context<V>);
type PickerEventHandler<V> = fn(&mut V, usize, &PickerEvent, &mut Context<V>);

pub(crate) struct QuickFieldControls {
    pub(crate) fields: Vec<QuickField>,
    pub(crate) open_choice: Option<OpenChoice>,
    _subs: Vec<Subscription>,
}

impl QuickFieldControls {
    pub(crate) fn new<V: 'static>(
        seeds: Vec<QuickFieldSeed<'_>>,
        lists: &OptionListSupport<'_>,
        on_event: FieldEventHandler<V>,
        cx: &mut Context<V>,
    ) -> Self {
        let mut subs = Vec::new();
        let fields = seeds
            .into_iter()
            .enumerate()
            .map(|(index, seed)| build_field(index, seed, lists, on_event, &mut subs, cx))
            .collect();
        let controls = Self {
            fields,
            open_choice: None,
            _subs: subs,
        };
        for index in 0..controls.fields.len() {
            controls.revalidate(index, cx);
        }
        controls
    }

    pub(crate) fn revalidate(&self, index: usize, cx: &mut App) {
        let Some(field) = self.fields.get(index) else {
            return;
        };
        let invalid = field.error.is_some() || field.entry_invalid(cx);
        match &field.control {
            FieldControl::Text(input) | FieldControl::Int { input, .. } => {
                input.update(cx, |input, cx| input.set_invalid(invalid, cx));
            }
            FieldControl::Multiline(area) => {
                area.update(cx, |area, cx| area.set_invalid(invalid, cx));
            }
            FieldControl::Toggle(_) | FieldControl::Choice(_) => {}
        }
    }

    pub(crate) fn entry_changed(&mut self, index: usize, cx: &mut App) {
        if let Some(field) = self.fields.get_mut(index) {
            field.error = None;
        }
        self.revalidate(index, cx);
    }

    pub(crate) fn focus_first<V: 'static>(&self, window: &mut Window, cx: &mut Context<V>) {
        for field in &self.fields {
            match &field.control {
                FieldControl::Text(input) | FieldControl::Int { input, .. } => {
                    input.update(cx, |input, cx| input.focus(window, cx));
                    return;
                }
                FieldControl::Multiline(area) => {
                    area.update(cx, |area, cx| area.focus(window, cx));
                    return;
                }
                FieldControl::Toggle(_) | FieldControl::Choice(_) => {}
            }
        }
    }

    pub(crate) fn toggle(&mut self, index: usize) -> bool {
        if let Some(field) = self.fields.get_mut(index)
            && let FieldControl::Toggle(value) = &mut field.control
        {
            *value = !*value;
            field.error = None;
            return true;
        }
        false
    }

    pub(crate) fn choice_is_open(&self, index: usize) -> bool {
        self.open_choice
            .as_ref()
            .is_some_and(|open| open.index == index)
    }

    pub(crate) fn open_choice_picker<V: 'static>(
        &mut self,
        index: usize,
        on_picker: PickerEventHandler<V>,
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        if self.choice_is_open(index) {
            self.open_choice = None;
            return;
        }
        let Some(FieldControl::Choice(choice)) = self.fields.get(index).map(|f| &f.control) else {
            return;
        };
        let ChoiceState::Ready(items) = &choice.state else {
            return;
        };
        let (items, current) = (items.clone(), choice.selected.clone());
        let labels = PickerLabels {
            placeholder: tr!("widget_picker_search_placeholder").into(),
            empty: tr!("widget_picker_no_results").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let picker = cx.new(|cx| Picker::new(labels, items, cx).with_current(current));
        let sub = cx.subscribe(&picker, move |view, _picker, event: &PickerEvent, cx| {
            on_picker(view, index, event, cx);
        });
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        self.open_choice = Some(OpenChoice {
            index,
            picker,
            _sub: sub,
        });
    }

    pub(crate) fn pick(&mut self, index: usize, id: &SharedString) -> bool {
        let mut changed = false;
        if let Some(field) = self.fields.get_mut(index)
            && let FieldControl::Choice(choice) = &mut field.control
        {
            changed = choice.selected.as_ref() != Some(id);
            choice.selected = Some(id.clone());
            field.error = None;
        }
        self.open_choice = None;
        changed
    }

    pub(crate) fn values(&self, cx: &App) -> BTreeMap<String, QuickActionFieldValue> {
        self.fields
            .iter()
            .filter_map(|field| {
                field
                    .current_value(cx)
                    .map(|value| (field.key.clone(), value))
            })
            .collect()
    }

    pub(crate) fn awaits_parent_choice(&self, choice: &ChoiceControl) -> bool {
        choice.option_list.as_ref().is_some_and(|binding| {
            self.fields.iter().any(|field| {
                binding.depends_on.contains(&field.key)
                    && field.option_list().is_some()
                    && field.selected_choice().is_none()
            })
        })
    }

    pub(crate) fn render_field<V: 'static>(
        &self,
        index: usize,
        ids: QuickFieldIds,
        handlers: &QuickFieldHandlers<V>,
        palette: &ForgePalette,
        cx: &mut Context<V>,
    ) -> AnyElement {
        let density = cx.density();
        let Some(field) = self.fields.get(index) else {
            return div().into_any_element();
        };
        let control = match &field.control {
            FieldControl::Text(input) | FieldControl::Int { input, .. } => {
                input.clone().into_any_element()
            }
            FieldControl::Multiline(area) => area.clone().into_any_element(),
            FieldControl::Toggle(value) => {
                let on_toggle = handlers.toggle;
                toggle_row(
                    (ids.toggle, index),
                    *value,
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| on_toggle(this, index, cx)),
                    density,
                    cx,
                )
            }
            FieldControl::Choice(choice) => {
                self.render_choice(index, choice, ids, handlers, palette, cx)
            }
        };
        field_column(
            palette,
            &field.label,
            field.hint.as_deref(),
            field.error.as_deref(),
            control,
            density,
            cx,
        )
    }

    fn render_choice<V: 'static>(
        &self,
        index: usize,
        choice: &ChoiceControl,
        ids: QuickFieldIds,
        handlers: &QuickFieldHandlers<V>,
        palette: &ForgePalette,
        cx: &mut Context<V>,
    ) -> AnyElement {
        let density = cx.density();
        let muted_note = |text: String, cx: &Context<V>| {
            div()
                .font_family(cx.body_family())
                .text_size(FONT_FIELD)
                .text_color(palette.text_muted)
                .child(text)
        };
        match &choice.state {
            ChoiceState::Loading => field_frame(palette, density)
                .gap(scaled(CHOICE_GAP, density))
                .child(spinner(
                    (ids.spinner, index),
                    Icon::Refresh,
                    FONT_XS,
                    palette.text_muted,
                ))
                .child(muted_note(tr!("integration_qa_field_loading"), cx))
                .into_any_element(),
            ChoiceState::Failed(reason) => match handlers.retry {
                Some(retry) => failure_with_retry(
                    (ids.retry, index),
                    reason,
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| retry(this, index, cx)),
                    density,
                    cx,
                ),
                None => failure_note(reason, palette, density, cx),
            },
            ChoiceState::Ready(items) if items.is_empty() => {
                let note = if self.awaits_parent_choice(choice) {
                    tr!("integration_qa_field_awaits_parent")
                } else {
                    tr!("integration_qa_field_empty")
                };
                field_frame(palette, density)
                    .child(muted_note(note, cx))
                    .into_any_element()
            }
            ChoiceState::Ready(items) => {
                let selected = selected_label(items, choice.selected.as_ref());
                let on_close = handlers.close;
                let popover = self
                    .open_choice
                    .as_ref()
                    .filter(|open| open.index == index)
                    .map(|open| {
                        let view = cx.entity();
                        dropdown(open.picker.clone()).on_dismiss(move |_window, cx| {
                            view.update(cx, on_close);
                        })
                    });
                let on_open = handlers.open;
                let trigger = choice_trigger(
                    (ids.choice, index),
                    selected,
                    choice.selected.is_some(),
                    palette,
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        on_open(this, index, window, cx)
                    }),
                    density,
                    cx,
                );
                div()
                    .relative()
                    .w_full()
                    .child(trigger)
                    .children(popover)
                    .into_any_element()
            }
        }
    }
}

impl QuickField {
    pub(crate) fn option_list(&self) -> Option<&OptionListBinding> {
        match &self.control {
            FieldControl::Choice(choice) => choice.option_list.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn selected_choice(&self) -> Option<&SharedString> {
        match &self.control {
            FieldControl::Choice(choice) => choice.selected.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn entry_invalid(&self, cx: &App) -> bool {
        match &self.control {
            FieldControl::Text(input) => self.text_invalid(input.read(cx).content()),
            FieldControl::Multiline(area) => self.text_invalid(area.read(cx).content()),
            FieldControl::Int {
                input, min, max, ..
            } => int_entry_invalid(input.read(cx).content(), *min, *max, self.required),
            FieldControl::Choice(choice) => self.required && choice.selected.is_none(),
            FieldControl::Toggle(_) => false,
        }
    }

    fn text_invalid(&self, text: &str) -> bool {
        (self.required && text.trim().is_empty())
            || self
                .max_chars
                .is_some_and(|limit| text.chars().count() > limit)
    }

    pub(crate) fn current_value(&self, cx: &App) -> Option<QuickActionFieldValue> {
        match &self.control {
            FieldControl::Text(input) => Some(QuickActionFieldValue::Text(
                input.read(cx).content().to_owned(),
            )),
            FieldControl::Multiline(area) => Some(QuickActionFieldValue::Text(
                area.read(cx).content().to_owned(),
            )),
            FieldControl::Toggle(value) => Some(QuickActionFieldValue::Toggle(*value)),
            FieldControl::Choice(choice) => choice
                .selected
                .as_ref()
                .map(|id| QuickActionFieldValue::Text(id.to_string())),
            FieldControl::Int {
                input,
                min,
                max,
                fallback,
            } => Some(QuickActionFieldValue::Int(
                parse_int_in_range(input.read(cx).content(), *min, *max).unwrap_or(*fallback),
            )),
        }
    }
}

fn build_field<V: 'static>(
    index: usize,
    seed: QuickFieldSeed<'_>,
    lists: &OptionListSupport<'_>,
    on_event: FieldEventHandler<V>,
    subs: &mut Vec<Subscription>,
    cx: &mut Context<V>,
) -> QuickField {
    let QuickFieldSeed {
        spec,
        initial,
        max_chars,
    } = seed;
    QuickField {
        key: spec.key.clone(),
        label: user_text_string(&spec.label),
        hint: field_hint(spec, max_chars),
        required: spec.required,
        max_chars,
        error: None,
        control: build_control(index, spec, initial, lists, on_event, subs, cx),
    }
}

fn build_control<V: 'static>(
    index: usize,
    spec: &QuickActionField,
    initial: Option<&QuickActionFieldValue>,
    lists: &OptionListSupport<'_>,
    on_event: FieldEventHandler<V>,
    subs: &mut Vec<Subscription>,
    cx: &mut Context<V>,
) -> FieldControl {
    let placeholder = spec
        .placeholder
        .as_ref()
        .map(user_text_string)
        .unwrap_or_default();
    match &spec.kind {
        QuickActionFieldKind::Text => {
            let content = initial_text(initial);
            let input = cx.new(|cx| {
                let mut input = TextInput::new(placeholder, cx);
                if !content.is_empty() {
                    input.set_content(content, cx);
                }
                input
            });
            subs.push(cx.subscribe(&input, move |this, _, event, cx| {
                on_event(this, index, event, cx)
            }));
            FieldControl::Text(input)
        }
        QuickActionFieldKind::Multiline | QuickActionFieldKind::MultilineList => {
            let content = initial_text(initial);
            let area = cx.new(|cx| {
                let mut area = TextArea::new(placeholder, cx).with_height(MULTILINE_H);
                if !content.is_empty() {
                    area.set_content(content, cx);
                }
                area
            });
            subs.push(cx.subscribe(&area, move |this, _, event, cx| {
                on_event(this, index, event, cx)
            }));
            FieldControl::Multiline(area)
        }
        QuickActionFieldKind::Toggle => {
            FieldControl::Toggle(matches!(initial, Some(QuickActionFieldValue::Toggle(true))))
        }
        QuickActionFieldKind::Int { min, max } => {
            let fallback = initial_int(initial).max(*min).min(*max);
            let input = cx.new(|cx| {
                let mut input = TextInput::new(placeholder, cx);
                input.set_content(fallback.to_string(), cx);
                input
            });
            subs.push(cx.subscribe(&input, move |this, _, event, cx| {
                on_event(this, index, event, cx)
            }));
            FieldControl::Int {
                input,
                min: *min,
                max: *max,
                fallback,
            }
        }
        QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)) => {
            FieldControl::Choice(ChoiceControl {
                option_list: None,
                state: ChoiceState::Ready(options.iter().map(static_item).collect()),
                selected: initial_choice(initial, options),
            })
        }
        QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(list)) => {
            let binding = lists
                .declared
                .iter()
                .find(|metadata| &metadata.id == list)
                .map(|metadata| OptionListBinding {
                    list: metadata.id.clone(),
                    depends_on: metadata.depends_on.clone(),
                    default: initial_option(initial),
                    ticket: 0,
                });
            let state = match binding {
                Some(_) => ChoiceState::Loading,
                None => ChoiceState::Failed(lists.unavailable.clone()),
            };
            FieldControl::Choice(ChoiceControl {
                option_list: binding,
                state,
                selected: None,
            })
        }
    }
}

fn field_hint(spec: &QuickActionField, max_chars: Option<usize>) -> Option<String> {
    if let Some(hint) = &spec.hint {
        return Some(user_text_string(hint));
    }
    match (&spec.kind, max_chars) {
        (QuickActionFieldKind::Int { min, max }, _) => Some(int_range_hint(*min, *max)),
        (_, Some(limit)) => Some(tr!("collection_field_max_chars", max = limit.to_string())),
        _ => None,
    }
}

fn static_item(option: &QuickActionChoiceOption) -> PickerItem {
    PickerItem {
        id: option.value.clone().into(),
        label: user_text_string(&option.label).into(),
        sublabel: None,
        icon: None,
    }
}

fn initial_choice(
    initial: Option<&QuickActionFieldValue>,
    options: &[QuickActionChoiceOption],
) -> Option<SharedString> {
    match initial {
        Some(QuickActionFieldValue::Text(value)) => Some(value.clone().into()),
        _ => options.first().map(|option| option.value.clone().into()),
    }
}

fn initial_option(initial: Option<&QuickActionFieldValue>) -> Option<SharedString> {
    match initial {
        Some(QuickActionFieldValue::Text(value)) if !value.is_empty() => Some(value.clone().into()),
        _ => None,
    }
}

fn initial_text(initial: Option<&QuickActionFieldValue>) -> String {
    match initial {
        Some(QuickActionFieldValue::Text(value)) => value.clone(),
        _ => String::new(),
    }
}

fn initial_int(initial: Option<&QuickActionFieldValue>) -> i64 {
    match initial {
        Some(QuickActionFieldValue::Int(value)) => *value,
        _ => 0,
    }
}

fn selected_label(items: &[PickerItem], selected: Option<&SharedString>) -> SharedString {
    match selected {
        Some(id) => items
            .iter()
            .find(|item| &item.id == id)
            .map(|item| item.label.clone())
            .unwrap_or_else(|| id.clone()),
        None => tr!("integration_qa_field_select").into(),
    }
}
