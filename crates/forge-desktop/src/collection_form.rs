use std::collections::BTreeMap;

use forge_components::{
    InputEvent, Picker, PickerEvent, PickerItem, PickerLabels, TextArea, TextInput, dropdown, tr,
};
use forge_platform_core::{
    CollectionField, CollectionItem, CollectionItemId, QuickActionChoiceSource, QuickActionField,
    QuickActionFieldKind, QuickActionFieldValue,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use crate::collection_text::localized_collection_text;
use crate::presentation::ActivePresentation;
use crate::quick_action_field_rows::{
    choice_trigger, failure_note, field_column, int_entry_invalid, int_range_hint,
    parse_int_in_range, toggle_row,
};

const FORM_GAP: gpui::Pixels = px(12.0);
const MULTILINE_HEIGHT: gpui::Pixels = px(80.0);

pub enum CollectionFormEvent {
    Cancelled,
}

enum FormControl {
    Text {
        input: Entity<TextInput>,
        max_chars: Option<usize>,
    },
    Multiline {
        area: Entity<TextArea>,
        max_chars: Option<usize>,
    },
    Toggle(bool),
    Int {
        input: Entity<TextInput>,
        min: i64,
        max: i64,
        fallback: i64,
    },
    Choice {
        items: Vec<PickerItem>,
        selected: Option<SharedString>,
    },
    Unsupported,
}

struct FormField {
    key: String,
    label: String,
    hint: Option<String>,
    required: bool,
    control: FormControl,
    error: Option<String>,
}

struct OpenChoice {
    index: usize,
    picker: Entity<Picker>,
    _sub: Subscription,
}

pub struct CollectionForm {
    editing: Option<CollectionItemId>,
    editing_title: Option<String>,
    fields: Vec<FormField>,
    open_choice: Option<OpenChoice>,
    submitting: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<CollectionFormEvent> for CollectionForm {}

impl CollectionForm {
    pub fn new(
        schema: &[CollectionField],
        item: Option<&CollectionItem>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let mut fields = Vec::with_capacity(schema.len());
        let mut subs = Vec::new();
        for (index, declared) in schema.iter().enumerate() {
            let spec = &declared.field;
            let initial = item
                .and_then(|item| item.values.get(&spec.key))
                .or(spec.default.as_ref());
            let placeholder = spec
                .placeholder
                .as_deref()
                .map(localized_collection_text)
                .unwrap_or_default();
            let control = match &spec.kind {
                QuickActionFieldKind::Text => {
                    let content = text_of(initial);
                    let input = cx.new(|cx| {
                        let mut input = TextInput::new(placeholder, cx).with_palette(palette);
                        if !content.is_empty() {
                            input.set_content(content, cx);
                        }
                        input
                    });
                    subs.push(cx.subscribe(&input, move |this, _, event, cx| {
                        this.on_field_event(index, event, cx)
                    }));
                    FormControl::Text {
                        input,
                        max_chars: declared.max_chars,
                    }
                }
                QuickActionFieldKind::Multiline | QuickActionFieldKind::MultilineList => {
                    let content = text_of(initial);
                    let area = cx.new(|cx| {
                        let mut area = TextArea::new(placeholder, cx)
                            .with_palette(palette)
                            .with_height(MULTILINE_HEIGHT);
                        if !content.is_empty() {
                            area.set_content(content, cx);
                        }
                        area
                    });
                    subs.push(cx.subscribe(&area, move |this, _, event, cx| {
                        this.on_field_event(index, event, cx)
                    }));
                    FormControl::Multiline {
                        area,
                        max_chars: declared.max_chars,
                    }
                }
                QuickActionFieldKind::Toggle => FormControl::Toggle(matches!(
                    initial,
                    Some(QuickActionFieldValue::Toggle(true))
                )),
                QuickActionFieldKind::Int { min, max } => {
                    let fallback = match initial {
                        Some(QuickActionFieldValue::Int(value)) => *value,
                        _ => *min,
                    }
                    .clamp(*min, *max);
                    let input = cx.new(|cx| {
                        let mut input = TextInput::new(placeholder, cx).with_palette(palette);
                        input.set_content(fallback.to_string(), cx);
                        input
                    });
                    subs.push(cx.subscribe(&input, move |this, _, event, cx| {
                        this.on_field_event(index, event, cx)
                    }));
                    FormControl::Int {
                        input,
                        min: *min,
                        max: *max,
                        fallback,
                    }
                }
                QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)) => {
                    let items: Vec<PickerItem> = options
                        .iter()
                        .map(|option| PickerItem {
                            id: option.value.clone().into(),
                            label: localized_collection_text(&option.label).into(),
                            sublabel: None,
                            icon: None,
                        })
                        .collect();
                    let selected = match initial {
                        Some(QuickActionFieldValue::Text(value)) => Some(value.clone().into()),
                        _ => items.first().map(|item| item.id.clone()),
                    };
                    FormControl::Choice { items, selected }
                }
                QuickActionFieldKind::Choice(QuickActionChoiceSource::Dynamic(_)) => {
                    FormControl::Unsupported
                }
            };
            fields.push(FormField {
                key: spec.key.clone(),
                label: localized_collection_text(&spec.label),
                hint: field_hint(spec, declared.max_chars),
                required: spec.required,
                control,
                error: None,
            });
        }
        let mut form = Self {
            editing: item.map(|item| item.id.clone()),
            editing_title: item.map(|item| item.title.clone()),
            fields,
            open_choice: None,
            submitting: false,
            _subs: subs,
        };
        form.revalidate_all(cx);
        form
    }

    pub fn editing(&self) -> Option<&CollectionItemId> {
        self.editing.as_ref()
    }

    pub fn editing_title(&self) -> Option<&str> {
        self.editing_title.as_deref()
    }

    pub fn is_submitting(&self) -> bool {
        self.submitting
    }

    pub fn set_submitting(&mut self, submitting: bool, cx: &mut Context<Self>) {
        self.submitting = submitting;
        cx.notify();
    }

    pub fn show_field_error(&mut self, key: &str, message: String, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.fields.iter().position(|field| field.key == key) else {
            return false;
        };
        if let Some(field) = self.fields.get_mut(index) {
            field.error = Some(message);
        }
        self.revalidate(index, cx);
        cx.notify();
        true
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        for field in &self.fields {
            match &field.control {
                FormControl::Text { input, .. } | FormControl::Int { input, .. } => {
                    input.update(cx, |input, cx| input.focus(window, cx));
                    return;
                }
                FormControl::Multiline { area, .. } => {
                    area.update(cx, |area, cx| area.focus(window, cx));
                    return;
                }
                FormControl::Toggle(_) | FormControl::Choice { .. } | FormControl::Unsupported => {}
            }
        }
    }

    pub fn can_submit(&self, cx: &App) -> bool {
        !self.submitting
            && self
                .fields
                .iter()
                .all(|field| !entry_invalid(field, &raw_entry(&field.control, cx)))
    }

    pub fn values(&self, cx: &App) -> BTreeMap<String, QuickActionFieldValue> {
        self.fields
            .iter()
            .filter_map(|field| {
                current_value(&field.control, cx).map(|value| (field.key.clone(), value))
            })
            .collect()
    }

    fn on_field_event(&mut self, index: usize, event: &InputEvent, cx: &mut Context<Self>) {
        match event {
            InputEvent::Cancelled => cx.emit(CollectionFormEvent::Cancelled),
            InputEvent::Changed(_) => {
                if let Some(field) = self.fields.get_mut(index) {
                    field.error = None;
                }
                self.revalidate(index, cx);
                cx.notify();
            }
            InputEvent::Submitted(_) | InputEvent::Blurred(_) => {}
        }
    }

    fn revalidate_all(&mut self, cx: &mut Context<Self>) {
        for index in 0..self.fields.len() {
            self.revalidate(index, cx);
        }
    }

    fn revalidate(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(field) = self.fields.get(index) else {
            return;
        };
        let invalid = field.error.is_some() || entry_invalid(field, &raw_entry(&field.control, cx));
        match &field.control {
            FormControl::Text { input, .. } | FormControl::Int { input, .. } => {
                let input = input.clone();
                input.update(cx, |input, cx| input.set_invalid(invalid, cx));
            }
            FormControl::Multiline { area, .. } => {
                let area = area.clone();
                area.update(cx, |area, cx| area.set_invalid(invalid, cx));
            }
            FormControl::Toggle(_) | FormControl::Choice { .. } | FormControl::Unsupported => {}
        }
    }

    fn toggle_field(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(field) = self.fields.get_mut(index)
            && let FormControl::Toggle(value) = &mut field.control
        {
            *value = !*value;
            field.error = None;
            cx.notify();
        }
    }

    fn open_choice(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .open_choice
            .as_ref()
            .is_some_and(|open| open.index == index)
        {
            self.close_choice(cx);
            return;
        }
        let Some(FormControl::Choice { items, selected }) =
            self.fields.get(index).map(|field| &field.control)
        else {
            return;
        };
        let (items, current) = (items.clone(), selected.clone());
        let palette = cx.palette();
        let labels = PickerLabels {
            placeholder: tr!("widget_picker_search_placeholder").into(),
            empty: tr!("widget_picker_no_results").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let picker = cx.new(|cx| Picker::new(labels, items, palette, cx).with_current(current));
        let sub = cx.subscribe(&picker, move |this, _picker, event: &PickerEvent, cx| {
            this.on_choice_picker(index, event, cx);
        });
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        self.open_choice = Some(OpenChoice {
            index,
            picker,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_choice_picker(&mut self, index: usize, event: &PickerEvent, cx: &mut Context<Self>) {
        match event {
            PickerEvent::Selected(id) => {
                if let Some(field) = self.fields.get_mut(index)
                    && let FormControl::Choice { selected, .. } = &mut field.control
                {
                    *selected = Some(id.clone());
                    field.error = None;
                }
                self.open_choice = None;
                cx.notify();
            }
            PickerEvent::Cancelled => self.close_choice(cx),
        }
    }

    fn close_choice(&mut self, cx: &mut Context<Self>) {
        self.open_choice = None;
        cx.notify();
    }

    fn render_field(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let palette = cx.palette();
        let Some(field) = self.fields.get(index) else {
            return div().into_any_element();
        };
        let control = match &field.control {
            FormControl::Text { input, .. } | FormControl::Int { input, .. } => {
                input.clone().into_any_element()
            }
            FormControl::Multiline { area, .. } => area.clone().into_any_element(),
            FormControl::Toggle(value) => toggle_row(
                ("collection-form-toggle", index),
                *value,
                &palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_field(index, cx)),
            ),
            FormControl::Choice { items, selected } => {
                let label = selected
                    .as_ref()
                    .map(|id| {
                        items
                            .iter()
                            .find(|item| &item.id == id)
                            .map(|item| item.label.clone())
                            .unwrap_or_else(|| id.clone())
                    })
                    .unwrap_or_else(|| tr!("integration_qa_field_select").into());
                let popover = self
                    .open_choice
                    .as_ref()
                    .filter(|open| open.index == index)
                    .map(|open| {
                        let view = cx.entity();
                        dropdown(open.picker.clone()).on_dismiss(move |_window, cx| {
                            view.update(cx, |this, cx| this.close_choice(cx));
                        })
                    });
                let trigger = choice_trigger(
                    ("collection-form-choice", index),
                    label,
                    selected.is_some(),
                    &palette,
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_choice(index, window, cx)
                    }),
                );
                div()
                    .relative()
                    .w_full()
                    .child(trigger)
                    .children(popover)
                    .into_any_element()
            }
            FormControl::Unsupported => {
                failure_note(&tr!("collection_field_unsupported"), &palette)
            }
        };
        field_column(
            &palette,
            &field.label,
            field.hint.as_deref(),
            field.error.as_deref(),
            control,
        )
    }
}

impl Render for CollectionForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div().w_full().flex().flex_col().gap(FORM_GAP);
        for index in 0..self.fields.len() {
            body = body.child(self.render_field(index, cx));
        }
        body
    }
}

fn field_hint(spec: &QuickActionField, max_chars: Option<usize>) -> Option<String> {
    if let Some(hint) = &spec.hint {
        return Some(localized_collection_text(hint));
    }
    match (&spec.kind, max_chars) {
        (QuickActionFieldKind::Int { min, max }, _) => Some(int_range_hint(*min, *max)),
        (_, Some(limit)) => Some(tr!("collection_field_max_chars", max = limit.to_string())),
        _ => None,
    }
}

fn text_of(value: Option<&QuickActionFieldValue>) -> String {
    match value {
        Some(QuickActionFieldValue::Text(text)) => text.clone(),
        _ => String::new(),
    }
}

enum RawEntry {
    Text {
        text: String,
        max_chars: Option<usize>,
    },
    Int {
        text: String,
        min: i64,
        max: i64,
    },
    Choice(bool),
    Toggle,
    Unsupported,
}

fn raw_entry(control: &FormControl, cx: &App) -> RawEntry {
    match control {
        FormControl::Text { input, max_chars } => RawEntry::Text {
            text: input.read(cx).content().to_owned(),
            max_chars: *max_chars,
        },
        FormControl::Multiline { area, max_chars } => RawEntry::Text {
            text: area.read(cx).content().to_owned(),
            max_chars: *max_chars,
        },
        FormControl::Int {
            input, min, max, ..
        } => RawEntry::Int {
            text: input.read(cx).content().to_owned(),
            min: *min,
            max: *max,
        },
        FormControl::Choice { selected, .. } => RawEntry::Choice(selected.is_some()),
        FormControl::Toggle(_) => RawEntry::Toggle,
        FormControl::Unsupported => RawEntry::Unsupported,
    }
}

fn entry_invalid(field: &FormField, entry: &RawEntry) -> bool {
    match entry {
        RawEntry::Text { text, max_chars } => {
            (field.required && text.trim().is_empty())
                || max_chars.is_some_and(|limit| text.chars().count() > limit)
        }
        RawEntry::Int { text, min, max } => int_entry_invalid(text, *min, *max, field.required),
        RawEntry::Choice(has_selection) => field.required && !has_selection,
        RawEntry::Toggle => false,
        RawEntry::Unsupported => field.required,
    }
}

fn current_value(control: &FormControl, cx: &App) -> Option<QuickActionFieldValue> {
    match control {
        FormControl::Text { input, .. } => Some(QuickActionFieldValue::Text(
            input.read(cx).content().to_owned(),
        )),
        FormControl::Multiline { area, .. } => Some(QuickActionFieldValue::Text(
            area.read(cx).content().to_owned(),
        )),
        FormControl::Toggle(value) => Some(QuickActionFieldValue::Toggle(*value)),
        FormControl::Int {
            input,
            min,
            max,
            fallback,
        } => Some(QuickActionFieldValue::Int(
            parse_int_in_range(input.read(cx).content(), *min, *max).unwrap_or(*fallback),
        )),
        FormControl::Choice { selected, .. } => selected
            .as_ref()
            .map(|id| QuickActionFieldValue::Text(id.to_string())),
        FormControl::Unsupported => None,
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{Density, ThemeId};
    use forge_platform_core::CollectionItemAccess;
    use gpui::{Entity, TestAppContext};

    use super::*;
    use crate::presentation::Presentation;

    const TITLE: &str = "title";
    const COST: &str = "cost";
    const INPUT: &str = "needs_input";
    const TITLE_LIMIT: usize = 45;

    fn field(key: &str, kind: QuickActionFieldKind, required: bool) -> QuickActionField {
        QuickActionField {
            key: key.to_owned(),
            label: key.to_owned(),
            kind,
            default: None,
            placeholder: None,
            hint: None,
            required,
        }
    }

    fn schema() -> Vec<CollectionField> {
        vec![
            CollectionField {
                field: field(TITLE, QuickActionFieldKind::Text, true),
                max_chars: Some(TITLE_LIMIT),
            },
            CollectionField {
                field: field(
                    COST,
                    QuickActionFieldKind::Int {
                        min: 1,
                        max: i64::MAX,
                    },
                    true,
                ),
                max_chars: None,
            },
            CollectionField {
                field: field(INPUT, QuickActionFieldKind::Toggle, false),
                max_chars: None,
            },
        ]
    }

    fn existing() -> CollectionItem {
        CollectionItem {
            id: CollectionItemId::new("rw1"),
            title: "Hydrate".to_owned(),
            values: BTreeMap::from([
                (
                    TITLE.to_owned(),
                    QuickActionFieldValue::Text("Hydrate".to_owned()),
                ),
                (COST.to_owned(), QuickActionFieldValue::Int(250)),
                (INPUT.to_owned(), QuickActionFieldValue::Toggle(true)),
            ]),
            toggles: BTreeMap::new(),
            access: CollectionItemAccess::Manageable,
        }
    }

    fn open(cx: &mut TestAppContext, item: Option<CollectionItem>) -> Entity<CollectionForm> {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.new(|cx| CollectionForm::new(&schema(), item.as_ref(), cx))
        })
    }

    fn type_title(cx: &mut TestAppContext, form: &Entity<CollectionForm>, title: String) {
        cx.update(|cx| {
            let input = match &form.read(cx).fields[0].control {
                FormControl::Text { input, .. } => input.clone(),
                _ => unreachable!("the first declared field is the text title"),
            };
            input.update(cx, |input, cx| input.set_content(title, cx));
        });
    }

    fn can_submit(cx: &mut TestAppContext, form: &Entity<CollectionForm>) -> bool {
        cx.update(|cx| form.read(cx).can_submit(cx))
    }

    #[gpui::test]
    fn editing_an_item_prefills_every_declared_value(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));

        let values = cx.update(|cx| form.read(cx).values(cx));

        assert_eq!(values, existing().values);
    }

    #[gpui::test]
    fn the_title_character_limit_blocks_submit_only_past_the_limit(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));
        let mut verdicts = Vec::new();

        for title in [
            "a".repeat(TITLE_LIMIT),
            "ї".repeat(TITLE_LIMIT),
            "a".repeat(TITLE_LIMIT + 1),
        ] {
            type_title(cx, &form, title);
            verdicts.push(can_submit(cx, &form));
        }

        assert_eq!(verdicts, vec![true, true, false]);
    }

    #[gpui::test]
    fn a_blank_required_title_blocks_submit(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));

        type_title(cx, &form, "   ".to_owned());

        assert!(!can_submit(cx, &form));
    }

    #[gpui::test]
    fn a_new_form_starts_blocked_until_the_required_title_is_typed(cx: &mut TestAppContext) {
        let form = open(cx, None);
        let before = can_submit(cx, &form);

        type_title(cx, &form, "Hydrate".to_owned());

        assert_eq!((before, can_submit(cx, &form)), (false, true));
    }

    #[gpui::test]
    fn a_server_error_lands_only_on_a_declared_field(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));

        let shown = cx.update(|cx| {
            form.update(cx, |form, cx| {
                (
                    form.show_field_error(TITLE, "taken".to_owned(), cx),
                    form.show_field_error("in_stock", "nope".to_owned(), cx),
                )
            })
        });

        assert_eq!(shown, (true, false));
    }

    #[gpui::test]
    fn submitting_blocks_a_second_submit(cx: &mut TestAppContext) {
        let form = open(cx, Some(existing()));

        cx.update(|cx| form.update(cx, |form, cx| form.set_submitting(true, cx)));

        assert!(!can_submit(cx, &form));
    }
}
