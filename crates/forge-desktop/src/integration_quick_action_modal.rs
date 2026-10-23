use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    FONT_XS, ForgePalette, Icon, InputEvent, OverlayPosition, Picker, PickerEvent, PickerItem,
    PickerLabels, TextArea, TextInput, body_family, destructive_button_with_icon, dropdown, modal,
    overlay, primary_button_with_icon, secondary_button, spinner, tr,
};
use forge_obs::{ObsClient, ObsSource};
use forge_platform_core::{
    BuiltinOptionSource, CollectionOutcome, OptionEntry, OptionListId, PickerKind, QuickAction,
    QuickActionChoiceOption, QuickActionChoiceSource, QuickActionField, QuickActionFieldKind,
    QuickActionFieldValue, RevisionWait,
};
use forge_types::{SubActionStep, Variant};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Rgba, SharedString, Subscription, Task,
    Window, div, prelude::*, px,
};
use tokio::runtime::Handle;

use crate::async_bridge::{self, Generation};
use crate::integration_quick_actions::accent_color;
use crate::option_list_text::{option_failure_message, option_hint_text};
use forge_components::ActivePresentation;
use forge_components::{
    FIELD_FONT, choice_trigger, failure_with_retry, field_column, field_frame, int_entry_invalid,
    int_range_hint, parse_int_in_range, toggle_row,
};

const TEXT_DEPENDENCY_DEBOUNCE: Duration = Duration::from_millis(400);

pub enum QuickActionModalEvent {
    Run { step: SubActionStep, label: String },
    Cancel,
}

enum ChoiceState {
    Loading,
    Ready(Vec<PickerItem>),
    Failed(String),
}

struct OptionListBinding {
    list: OptionListId,
    depends_on: Vec<String>,
    default: Option<SharedString>,
    ticket: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResolveMode {
    Load,
    Refresh,
}

struct ChoiceControl {
    dynamic: Option<PickerKind>,
    option_list: Option<OptionListBinding>,
    state: ChoiceState,
    selected: Option<SharedString>,
    scene: Option<String>,
}

enum FieldControl {
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

struct ModalField {
    key: String,
    label: String,
    hint: Option<String>,
    required: bool,
    control: FieldControl,
}

struct OpenChoice {
    index: usize,
    picker: Entity<Picker>,
    _sub: Subscription,
}

pub struct QuickActionModal {
    label: String,
    glyph: Icon,
    accent: Rgba,
    destructive: bool,
    action: QuickAction,
    fields: Vec<ModalField>,
    obs_source: Option<Arc<ObsClient>>,
    option_source: Option<Arc<dyn BuiltinOptionSource>>,
    rt_handle: Handle,
    open_choice: Option<OpenChoice>,
    text_resolve: Generation,
    pending_text_changes: Vec<usize>,
    _subs: Vec<Subscription>,
    _revision_watch: Option<Task<()>>,
}

impl EventEmitter<QuickActionModalEvent> for QuickActionModal {}

impl QuickActionModal {
    pub fn new(
        action: QuickAction,
        obs_source: Option<Arc<ObsClient>>,
        option_source: Option<Arc<dyn BuiltinOptionSource>>,
        rt_handle: Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let accent = accent_color(action.accent, &palette);
        let glyph = Icon::from_name(action.icon.as_str());
        let label = action.label.clone();
        let destructive = action.destructive;

        let specs = build_specs(&action);
        let mut merge_action = action.clone();
        merge_action.fields = specs.iter().map(spec_to_field).collect();
        let mut fields = Vec::with_capacity(specs.len());
        let mut subs = Vec::new();
        let mut pending_dynamic: Vec<(usize, PickerKind)> = Vec::new();
        let declared_lists = option_source
            .as_ref()
            .map(|source| source.option_lists())
            .unwrap_or_default();

        for (i, spec) in specs.into_iter().enumerate() {
            let range_hint = match &spec.kind {
                QuickActionFieldKind::Int { min, max } => Some(int_range_hint(*min, *max)),
                _ => None,
            };
            let control = match &spec.kind {
                QuickActionFieldKind::Text => {
                    let content = default_text(&spec.default);
                    let placeholder = spec.placeholder.clone().unwrap_or_default();
                    let blank = spec.required && content.trim().is_empty();
                    let input = cx.new(|cx| {
                        let mut ti = TextInput::new(placeholder, cx).with_palette(palette);
                        if !content.is_empty() {
                            ti.set_content(content, cx);
                        }
                        ti.set_invalid(blank, cx);
                        ti
                    });
                    subs.push(cx.subscribe(&input, move |this, _, event, cx| {
                        this.on_field_event(i, event, cx)
                    }));
                    FieldControl::Text(input)
                }
                QuickActionFieldKind::Multiline | QuickActionFieldKind::MultilineList => {
                    let content = default_text(&spec.default);
                    let placeholder = spec.placeholder.clone().unwrap_or_default();
                    let blank = spec.required && content.trim().is_empty();
                    let area = cx.new(|cx| {
                        let mut ta = TextArea::new(placeholder, cx)
                            .with_palette(palette)
                            .with_height(px(80.0));
                        if !content.is_empty() {
                            ta.set_content(content, cx);
                        }
                        ta.set_invalid(blank, cx);
                        ta
                    });
                    subs.push(cx.subscribe(&area, move |this, _, event, cx| {
                        this.on_field_event(i, event, cx)
                    }));
                    FieldControl::Multiline(area)
                }
                QuickActionFieldKind::Toggle => FieldControl::Toggle(default_toggle(&spec.default)),
                QuickActionFieldKind::Int { min, max } => {
                    let fallback = default_int(&spec.default).max(*min).min(*max);
                    let placeholder = spec.placeholder.clone().unwrap_or_default();
                    let input = cx.new(|cx| {
                        let mut ti = TextInput::new(placeholder, cx).with_palette(palette);
                        ti.set_content(fallback.to_string(), cx);
                        ti
                    });
                    subs.push(cx.subscribe(&input, move |this, _, event, cx| {
                        this.on_field_event(i, event, cx)
                    }));
                    FieldControl::Int {
                        input,
                        min: *min,
                        max: *max,
                        fallback,
                    }
                }
                QuickActionFieldKind::Choice(source) => match source {
                    QuickActionChoiceSource::Static(opts) => {
                        let items = opts.iter().map(static_item).collect();
                        FieldControl::Choice(ChoiceControl {
                            dynamic: None,
                            option_list: None,
                            state: ChoiceState::Ready(items),
                            selected: choice_default(&spec.default, opts),
                            scene: None,
                        })
                    }
                    QuickActionChoiceSource::Dynamic(pk) => {
                        if obs_source.is_some() {
                            pending_dynamic.push((i, *pk));
                            FieldControl::Choice(ChoiceControl {
                                dynamic: Some(*pk),
                                option_list: None,
                                state: ChoiceState::Loading,
                                selected: None,
                                scene: None,
                            })
                        } else {
                            FieldControl::Choice(ChoiceControl {
                                dynamic: Some(*pk),
                                option_list: None,
                                state: ChoiceState::Failed(tr!("integration_qa_field_unavailable")),
                                selected: None,
                                scene: None,
                            })
                        }
                    }
                    QuickActionChoiceSource::OptionList(list) => {
                        let binding = declared_lists
                            .iter()
                            .find(|metadata| &metadata.id == list)
                            .map(|metadata| OptionListBinding {
                                list: metadata.id.clone(),
                                depends_on: metadata.depends_on.clone(),
                                default: option_default(&spec.default),
                                ticket: 0,
                            });
                        let state = match binding {
                            Some(_) => ChoiceState::Loading,
                            None => ChoiceState::Failed(tr!("integration_qa_field_unavailable")),
                        };
                        FieldControl::Choice(ChoiceControl {
                            dynamic: None,
                            option_list: binding,
                            state,
                            selected: None,
                            scene: None,
                        })
                    }
                },
            };
            fields.push(ModalField {
                key: spec.key,
                label: spec.label,
                hint: spec.hint.or(range_hint),
                required: spec.required,
                control,
            });
        }

        for (i, pk) in pending_dynamic {
            if let Some(client) = &obs_source {
                let client = Arc::clone(client);
                async_bridge::run_async(
                    &rt_handle,
                    fetch_picker_items(client, pk, fetch_labels()),
                    move |this, result, cx| this.apply_dynamic(i, result, cx),
                    cx,
                );
            }
        }

        let revision_watch = option_source
            .as_ref()
            .filter(|_| fields.iter().any(|field| field.option_list().is_some()))
            .map(|source| {
                let mut revisions = source.revisions();
                cx.spawn(async move |this, cx| {
                    while revisions.changed().await == RevisionWait::Changed {
                        let refreshed = this.update(cx, |this, cx| {
                            this.resolve_option_lists(ResolveMode::Refresh, cx)
                        });
                        if refreshed.is_err() {
                            break;
                        }
                    }
                })
            });

        let mut modal = Self {
            label,
            glyph,
            accent,
            destructive,
            action: merge_action,
            fields,
            obs_source,
            option_source,
            rt_handle,
            open_choice: None,
            text_resolve: Generation::default(),
            pending_text_changes: Vec::new(),
            _subs: subs,
            _revision_watch: revision_watch,
        };
        modal.resolve_option_lists(ResolveMode::Load, cx);
        modal
    }

    fn resolve_option_lists(&mut self, mode: ResolveMode, cx: &mut Context<Self>) {
        for index in 0..self.fields.len() {
            self.resolve_option_list(index, mode, cx);
        }
    }

    fn resolve_dependents(&mut self, changed: usize, cx: &mut Context<Self>) {
        let Some(key) = self.fields.get(changed).map(|field| field.key.clone()) else {
            return;
        };
        let dependents: Vec<usize> = self
            .fields
            .iter()
            .enumerate()
            .filter(|(_, field)| {
                field
                    .option_list()
                    .is_some_and(|binding| binding.depends_on.contains(&key))
            })
            .map(|(index, _)| index)
            .collect();
        for index in dependents {
            self.resolve_option_list(index, ResolveMode::Load, cx);
        }
    }

    fn resolve_dependents_debounced(&mut self, changed: usize, cx: &mut Context<Self>) {
        if !self.pending_text_changes.contains(&changed) {
            self.pending_text_changes.push(changed);
        }
        let ticket = self.text_resolve.next();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(TEXT_DEPENDENCY_DEBOUNCE)
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.text_resolve.is_current(ticket) {
                    return;
                }
                for index in std::mem::take(&mut this.pending_text_changes) {
                    this.resolve_dependents(index, cx);
                }
            });
        })
        .detach();
    }

    fn dependency_values(
        &self,
        depends_on: &[String],
        cx: &Context<Self>,
    ) -> BTreeMap<String, QuickActionFieldValue> {
        self.fields
            .iter()
            .filter(|field| depends_on.contains(&field.key))
            .map(|field| (field.key.clone(), field.current_value(cx)))
            .collect()
    }

    fn resolve_option_list(&mut self, index: usize, mode: ResolveMode, cx: &mut Context<Self>) {
        let Some(source) = self.option_source.clone() else {
            return;
        };
        let Some(depends_on) = self
            .fields
            .get(index)
            .and_then(ModalField::option_list)
            .map(|binding| binding.depends_on.clone())
        else {
            return;
        };
        let values = self.dependency_values(&depends_on, cx);
        let Some(FieldControl::Choice(choice)) =
            self.fields.get_mut(index).map(|field| &mut field.control)
        else {
            return;
        };
        let Some(binding) = choice.option_list.as_mut() else {
            return;
        };
        binding.ticket = binding.ticket.wrapping_add(1);
        let ticket = binding.ticket;
        let list = binding.list.clone();
        let reloads = mode == ResolveMode::Load || !matches!(choice.state, ChoiceState::Ready(_));
        if reloads {
            choice.state = ChoiceState::Loading;
            if self
                .open_choice
                .as_ref()
                .is_some_and(|open| open.index == index)
            {
                self.open_choice = None;
            }
        }
        cx.notify();
        async_bridge::run_async(
            &self.rt_handle,
            async move { source.resolve(&list, &values).await },
            move |this, outcome, cx| this.apply_option_list(index, ticket, outcome, cx),
            cx,
        );
    }

    fn apply_option_list(
        &mut self,
        index: usize,
        ticket: u64,
        outcome: CollectionOutcome<Vec<OptionEntry>>,
        cx: &mut Context<Self>,
    ) {
        let Some(FieldControl::Choice(choice)) =
            self.fields.get_mut(index).map(|field| &mut field.control)
        else {
            return;
        };
        let Some(binding) = &choice.option_list else {
            return;
        };
        if binding.ticket != ticket {
            return;
        }
        let previous = choice.selected.clone();
        let open_picker = self
            .open_choice
            .as_ref()
            .filter(|open| open.index == index)
            .map(|open| open.picker.clone());
        match outcome {
            Ok(entries) => {
                choice.selected =
                    option_preselect(&entries, previous.as_ref(), binding.default.as_ref());
                let items: Vec<PickerItem> = entries.iter().map(option_item).collect();
                if let Some(picker) = open_picker {
                    let refreshed = items.clone();
                    picker.update(cx, |picker, cx| picker.set_items(refreshed, cx));
                }
                choice.state = ChoiceState::Ready(items);
            }
            Err(failure) => {
                choice.selected = None;
                choice.state = ChoiceState::Failed(option_failure_message(&failure));
                if open_picker.is_some() {
                    self.open_choice = None;
                }
            }
        }
        let selection_changed = self
            .fields
            .get(index)
            .is_some_and(|field| field.selected_choice() != previous.as_ref());
        cx.notify();
        if selection_changed {
            self.resolve_dependents(index, cx);
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        for field in &self.fields {
            match &field.control {
                FieldControl::Text(input) => {
                    input.update(cx, |f, cx| f.focus(window, cx));
                    return;
                }
                FieldControl::Multiline(area) => {
                    area.update(cx, |f, cx| f.focus(window, cx));
                    return;
                }
                FieldControl::Int { input, .. } => {
                    input.update(cx, |f, cx| f.focus(window, cx));
                    return;
                }
                _ => {}
            }
        }
    }

    fn on_field_event(&mut self, index: usize, event: &InputEvent, cx: &mut Context<Self>) {
        match event {
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Changed(text) => {
                self.revalidate(index, text, cx);
                cx.notify();
                self.resolve_dependents_debounced(index, cx);
            }
            InputEvent::Submitted(_) | InputEvent::Blurred(_) => {}
        }
    }

    fn revalidate(&mut self, index: usize, text: &str, cx: &mut Context<Self>) {
        let Some(field) = self.fields.get(index) else {
            return;
        };
        let invalid = match &field.control {
            FieldControl::Int { min, max, .. } => {
                int_entry_invalid(text, *min, *max, field.required)
            }
            FieldControl::Text(_) | FieldControl::Multiline(_) => {
                field.required && text.trim().is_empty()
            }
            _ => return,
        };
        match &field.control {
            FieldControl::Text(input) | FieldControl::Int { input, .. } => {
                let input = input.clone();
                input.update(cx, |input, cx| input.set_invalid(invalid, cx));
            }
            FieldControl::Multiline(area) => {
                let area = area.clone();
                area.update(cx, |area, cx| area.set_invalid(invalid, cx));
            }
            _ => {}
        }
    }

    fn apply_dynamic(
        &mut self,
        index: usize,
        result: Result<PickerFetch, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.fields.get_mut(index) else {
            return;
        };
        match result {
            Ok(fetch) => {
                field.hint = fetch
                    .hint_scene
                    .map(|scene| tr!("integration_qa_scene_current_hint", scene = scene));
                let FieldControl::Choice(choice) = &mut field.control else {
                    return;
                };
                choice.selected = fetch
                    .preselect
                    .filter(|id| fetch.items.iter().any(|item| &item.id == id))
                    .or_else(|| fetch.items.first().map(|item| item.id.clone()));
                choice.scene = fetch.scene;
                choice.state = ChoiceState::Ready(fetch.items);
            }
            Err(reason) => {
                field.hint = None;
                let FieldControl::Choice(choice) = &mut field.control else {
                    return;
                };
                choice.selected = None;
                choice.state = ChoiceState::Failed(reason);
            }
        }
        cx.notify();
    }

    fn retry_dynamic(&mut self, index: usize, cx: &mut Context<Self>) {
        if self
            .fields
            .get(index)
            .and_then(ModalField::option_list)
            .is_some()
        {
            self.resolve_option_list(index, ResolveMode::Load, cx);
            return;
        }
        let pk = match self.fields.get(index).map(|f| &f.control) {
            Some(FieldControl::Choice(choice)) => choice.dynamic,
            _ => None,
        };
        let Some(pk) = pk else {
            return;
        };
        let Some(client) = self.obs_source.clone() else {
            if let Some(FieldControl::Choice(choice)) =
                self.fields.get_mut(index).map(|f| &mut f.control)
            {
                choice.state = ChoiceState::Failed(tr!("integration_qa_field_unavailable"));
            }
            cx.notify();
            return;
        };
        if let Some(FieldControl::Choice(choice)) =
            self.fields.get_mut(index).map(|f| &mut f.control)
        {
            choice.selected = None;
            choice.state = ChoiceState::Loading;
        }
        cx.notify();
        async_bridge::run_async(
            &self.rt_handle,
            fetch_picker_items(client, pk, fetch_labels()),
            move |this, result, cx| this.apply_dynamic(index, result, cx),
            cx,
        );
    }

    fn toggle_field(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(FieldControl::Toggle(value)) =
            self.fields.get_mut(index).map(|f| &mut f.control)
        {
            *value = !*value;
            cx.notify();
            self.resolve_dependents(index, cx);
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
        let (items, current) = match self.fields.get(index) {
            Some(field) => match &field.control {
                FieldControl::Choice(choice) => match &choice.state {
                    ChoiceState::Ready(items) => (items.clone(), choice.selected.clone()),
                    _ => return,
                },
                _ => return,
            },
            None => return,
        };
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
        picker.update(cx, |f, cx| f.focus(window, cx));
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
                let mut changed = false;
                if let Some(FieldControl::Choice(choice)) =
                    self.fields.get_mut(index).map(|f| &mut f.control)
                {
                    changed = choice.selected.as_ref() != Some(id);
                    choice.selected = Some(id.clone());
                }
                self.open_choice = None;
                cx.notify();
                if changed {
                    self.resolve_dependents(index, cx);
                }
            }
            PickerEvent::Cancelled => self.close_choice(cx),
        }
    }

    fn close_choice(&mut self, cx: &mut Context<Self>) {
        self.open_choice = None;
        cx.notify();
    }

    fn awaits_debounced_resolve(&self, choice: &ChoiceControl) -> bool {
        choice.option_list.as_ref().is_some_and(|binding| {
            self.pending_text_changes.iter().any(|&changed| {
                self.fields
                    .get(changed)
                    .is_some_and(|field| binding.depends_on.contains(&field.key))
            })
        })
    }

    fn can_run(&self, cx: &Context<Self>) -> bool {
        self.fields.iter().all(|field| match &field.control {
            FieldControl::Choice(choice) => {
                matches!(choice.state, ChoiceState::Ready(_))
                    && choice.selected.is_some()
                    && !self.awaits_debounced_resolve(choice)
            }
            FieldControl::Text(input) => {
                !field.required || !input.read(cx).content().trim().is_empty()
            }
            FieldControl::Multiline(area) => {
                !field.required || !area.read(cx).content().trim().is_empty()
            }
            FieldControl::Int {
                input, min, max, ..
            } => !int_entry_invalid(input.read(cx).content(), *min, *max, field.required),
            FieldControl::Toggle(_) => true,
        })
    }

    fn build_step(&self, cx: &Context<Self>) -> SubActionStep {
        let values: BTreeMap<String, QuickActionFieldValue> = self
            .fields
            .iter()
            .map(|field| (field.key.clone(), field.current_value(cx)))
            .collect();
        let mut step = self.action.merge_config(&values);
        for field in &self.fields {
            if let FieldControl::Choice(choice) = &field.control
                && matches!(choice.dynamic, Some(PickerKind::Source))
                && let Some(scene) = &choice.scene
            {
                step.config
                    .insert("scene".to_owned(), Variant::String(scene.clone()));
            }
        }
        step
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        if !self.can_run(cx) {
            return;
        }
        let step = self.build_step(cx);
        cx.emit(QuickActionModalEvent::Run {
            step,
            label: self.label.clone(),
        });
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(QuickActionModalEvent::Cancel);
    }

    fn render_field(
        &self,
        index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(field) = self.fields.get(index) else {
            return div().into_any_element();
        };
        let control = match &field.control {
            FieldControl::Text(input) => input.clone().into_any_element(),
            FieldControl::Multiline(area) => area.clone().into_any_element(),
            FieldControl::Int { input, .. } => input.clone().into_any_element(),
            FieldControl::Toggle(value) => self.render_toggle(index, *value, palette, cx),
            FieldControl::Choice(choice) => self.render_choice(index, choice, palette, cx),
        };
        field_column(palette, &field.label, field.hint.as_deref(), None, control)
    }

    fn render_toggle(
        &self,
        index: usize,
        value: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        toggle_row(
            ("qa-toggle", index),
            value,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_field(index, cx)),
        )
    }

    fn render_choice(
        &self,
        index: usize,
        choice: &ChoiceControl,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match &choice.state {
            ChoiceState::Loading => field_frame(palette)
                .gap(px(8.0))
                .child(spinner(
                    ("qa-choice-spin", index),
                    Icon::Refresh,
                    FONT_XS,
                    palette.text_muted,
                ))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FIELD_FONT)
                        .text_color(palette.text_muted)
                        .child(tr!("integration_qa_field_loading")),
                )
                .into_any_element(),
            ChoiceState::Failed(reason) => failure_with_retry(
                ("qa-retry", index),
                reason,
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.retry_dynamic(index, cx)),
            ),
            ChoiceState::Ready(items) => {
                let selected = selected_label(items, &choice.selected);
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
                    ("qa-choice", index),
                    selected,
                    choice.selected.is_some(),
                    palette,
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
        }
    }
}

impl Render for QuickActionModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let mut body = div().w_full().flex().flex_col().gap(px(12.0));
        if self.fields.is_empty() {
            body = body.child(
                div()
                    .font_family(body_family())
                    .text_size(px(12.5))
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "integration_qa_modal_confirm_body",
                        label = self.label.clone()
                    )),
            );
        } else {
            for index in 0..self.fields.len() {
                body = body.child(self.render_field(index, &palette, cx));
            }
        }

        let hint = if self.destructive {
            tr!("integration_qa_modal_footer_destructive")
        } else {
            tr!("integration_qa_modal_footer_immediate")
        };
        let run_button = if self.destructive {
            destructive_button_with_icon(
                Icon::AlertTriangle,
                tr!("integration_qa_modal_confirm"),
                &palette,
            )
        } else {
            primary_button_with_icon(
                Icon::PlayerPlayFilled,
                tr!("integration_qa_modal_run"),
                &palette,
            )
        }
        .disabled(!self.can_run(cx))
        .on_click(
            "qa-modal-run",
            cx.listener(|this, _: &ClickEvent, _, cx| this.run(cx)),
        );
        let cancel_button = secondary_button(tr!("common_cancel"), &palette).on_click(
            "qa-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(px(11.0))
                    .text_color(palette.text_faint)
                    .child(hint),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(cancel_button)
                    .child(run_button),
            );

        let card = modal(self.label.clone(), body, &palette)
            .header_icon(self.glyph, self.accent)
            .subtitle(tr!("integration_qa_modal_subtitle"))
            .width(px(440.0))
            .footer(footer)
            .on_close(
                "qa-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let modal_view = cx.entity();
        let modal_overlay = overlay(card, &palette)
            .position(OverlayPosition::Center)
            .on_dismiss("qa-modal-scrim", move |_window, cx| {
                modal_view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element();

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(modal_overlay)
    }
}

impl ModalField {
    fn option_list(&self) -> Option<&OptionListBinding> {
        match &self.control {
            FieldControl::Choice(choice) => choice.option_list.as_ref(),
            _ => None,
        }
    }

    fn selected_choice(&self) -> Option<&SharedString> {
        match &self.control {
            FieldControl::Choice(choice) => choice.selected.as_ref(),
            _ => None,
        }
    }

    fn current_value(&self, cx: &Context<QuickActionModal>) -> QuickActionFieldValue {
        match &self.control {
            FieldControl::Text(input) => {
                QuickActionFieldValue::Text(input.read(cx).content().to_owned())
            }
            FieldControl::Multiline(area) => {
                QuickActionFieldValue::Text(area.read(cx).content().to_owned())
            }
            FieldControl::Toggle(value) => QuickActionFieldValue::Toggle(*value),
            FieldControl::Choice(choice) => QuickActionFieldValue::Text(
                choice
                    .selected
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
            ),
            FieldControl::Int {
                input,
                min,
                max,
                fallback,
            } => {
                let raw = input.read(cx).content();
                QuickActionFieldValue::Int(parse_int_in_range(raw, *min, *max).unwrap_or(*fallback))
            }
        }
    }
}

struct FieldSpec {
    key: String,
    label: String,
    hint: Option<String>,
    kind: QuickActionFieldKind,
    default: Option<QuickActionFieldValue>,
    placeholder: Option<String>,
    required: bool,
}

fn build_specs(action: &QuickAction) -> Vec<FieldSpec> {
    if !action.fields.is_empty() {
        return action
            .fields
            .iter()
            .map(|field| FieldSpec {
                key: field.key.clone(),
                label: field.label.clone(),
                hint: field.hint.clone(),
                kind: field.kind.clone(),
                default: field.default.clone(),
                placeholder: field.placeholder.clone(),
                required: field.required,
            })
            .collect();
    }
    match action.picker {
        Some(pk) => vec![FieldSpec {
            key: picker_key(pk).to_owned(),
            label: picker_title(pk),
            hint: None,
            kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::Dynamic(pk)),
            default: None,
            placeholder: None,
            required: true,
        }],
        None => Vec::new(),
    }
}

fn spec_to_field(spec: &FieldSpec) -> QuickActionField {
    QuickActionField {
        key: spec.key.clone(),
        label: spec.label.clone(),
        kind: spec.kind.clone(),
        default: spec.default.clone(),
        placeholder: spec.placeholder.clone(),
        hint: spec.hint.clone(),
        required: spec.required,
    }
}

fn picker_key(pk: PickerKind) -> &'static str {
    match pk {
        PickerKind::Scene => "scene",
        PickerKind::Source | PickerKind::Input => "source",
        PickerKind::Hotkey => "hotkey",
        PickerKind::Expression => "expression",
        PickerKind::MidiPort => "port",
        PickerKind::Transition => "transition",
        PickerKind::Profile => "profile",
        PickerKind::SceneCollection => "collection",
        PickerKind::Model => "model",
        PickerKind::Item | PickerKind::ItemInstance => "item",
    }
}

fn picker_title(pk: PickerKind) -> String {
    match pk {
        PickerKind::Scene => tr!("builtin_picker_scene"),
        PickerKind::Source => tr!("builtin_picker_source"),
        PickerKind::Input => tr!("builtin_picker_audio_input"),
        PickerKind::Hotkey => tr!("builtin_picker_hotkey"),
        PickerKind::Expression => tr!("builtin_picker_expression"),
        PickerKind::MidiPort => tr!("builtin_picker_midi_port"),
        PickerKind::Transition => tr!("builtin_picker_transition"),
        PickerKind::Profile => tr!("builtin_picker_profile"),
        PickerKind::SceneCollection => tr!("builtin_picker_scene_collection"),
        PickerKind::Model => tr!("builtin_picker_model"),
        PickerKind::Item => tr!("builtin_picker_item"),
        PickerKind::ItemInstance => tr!("builtin_picker_item_instance"),
    }
}

fn static_item(opt: &QuickActionChoiceOption) -> PickerItem {
    PickerItem {
        id: opt.value.clone().into(),
        label: opt.label.clone().into(),
        sublabel: None,
        icon: None,
    }
}

fn choice_default(
    default: &Option<QuickActionFieldValue>,
    opts: &[QuickActionChoiceOption],
) -> Option<SharedString> {
    match default {
        Some(QuickActionFieldValue::Text(value)) => Some(value.clone().into()),
        _ => opts.first().map(|opt| opt.value.clone().into()),
    }
}

fn option_item(entry: &OptionEntry) -> PickerItem {
    PickerItem {
        id: entry.value.clone().into(),
        label: entry.label.clone().into(),
        sublabel: entry
            .hint
            .as_ref()
            .map(|hint| option_hint_text(hint).into()),
        icon: entry
            .icon
            .as_ref()
            .map(|icon| Icon::from_name(icon.as_str())),
    }
}

fn option_default(default: &Option<QuickActionFieldValue>) -> Option<SharedString> {
    match default {
        Some(QuickActionFieldValue::Text(value)) if !value.is_empty() => Some(value.clone().into()),
        _ => None,
    }
}

fn option_preselect(
    entries: &[OptionEntry],
    previous: Option<&SharedString>,
    default: Option<&SharedString>,
) -> Option<SharedString> {
    let listed = |id: &&SharedString| entries.iter().any(|entry| entry.value == id.as_ref());
    if let Some(previous) = previous.filter(listed) {
        return Some(previous.clone());
    }
    let mut current = entries.iter().filter(|entry| entry.current);
    if let (Some(only), None) = (current.next(), current.next()) {
        return Some(only.value.clone().into());
    }
    default.filter(listed).cloned()
}

fn selected_label(items: &[PickerItem], selected: &Option<SharedString>) -> SharedString {
    match selected {
        Some(id) => items
            .iter()
            .find(|item| &item.id == id)
            .map(|item| item.label.clone())
            .unwrap_or_else(|| id.clone()),
        None => tr!("integration_qa_field_select").into(),
    }
}

fn default_text(default: &Option<QuickActionFieldValue>) -> String {
    match default {
        Some(QuickActionFieldValue::Text(value)) => value.clone(),
        _ => String::new(),
    }
}

fn default_toggle(default: &Option<QuickActionFieldValue>) -> bool {
    matches!(default, Some(QuickActionFieldValue::Toggle(true)))
}

fn default_int(default: &Option<QuickActionFieldValue>) -> i64 {
    match default {
        Some(QuickActionFieldValue::Int(value)) => *value,
        _ => 0,
    }
}

struct PickerFetch {
    items: Vec<PickerItem>,
    scene: Option<String>,
    preselect: Option<SharedString>,
    hint_scene: Option<String>,
}

fn no_context(items: Vec<PickerItem>) -> PickerFetch {
    PickerFetch {
        items,
        scene: None,
        preselect: None,
        hint_scene: None,
    }
}

struct FetchLabels {
    no_scene: String,
    source_visible: String,
    source_hidden: String,
    unavailable: String,
}

fn fetch_labels() -> FetchLabels {
    FetchLabels {
        no_scene: tr!("integration_qa_field_no_scene"),
        source_visible: tr!("integration_qa_source_visible"),
        source_hidden: tr!("integration_qa_source_hidden"),
        unavailable: tr!("integration_qa_field_unavailable"),
    }
}

async fn fetch_picker_items(
    client: Arc<ObsClient>,
    kind: PickerKind,
    labels: FetchLabels,
) -> Result<PickerFetch, String> {
    match kind {
        PickerKind::Scene => {
            let current = client.current_scene().await.map_err(|e| e.to_string())?;
            let scenes = client.scenes().await.map_err(|e| e.to_string())?;
            let items = scenes
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Some(Icon::from_name("layout")),
                })
                .collect();
            Ok(PickerFetch {
                items,
                scene: None,
                preselect: current.clone().map(SharedString::from),
                hint_scene: current,
            })
        }
        PickerKind::Source => {
            let scene = client
                .current_scene()
                .await
                .map_err(|e| e.to_string())?
                .ok_or(labels.no_scene)?;
            let sources = client.sources(&scene).await.map_err(|e| e.to_string())?;
            let items = sources
                .into_iter()
                .map(|source| PickerItem {
                    id: source.name.clone().into(),
                    label: source.name.into(),
                    sublabel: Some(
                        if source.visible {
                            labels.source_visible.clone()
                        } else {
                            labels.source_hidden.clone()
                        }
                        .into(),
                    ),
                    icon: Some(Icon::from_name("device-desktop")),
                })
                .collect();
            Ok(PickerFetch {
                items,
                scene: Some(scene),
                preselect: None,
                hint_scene: None,
            })
        }
        PickerKind::Input => {
            let inputs = client.audio_inputs().await.map_err(|e| e.to_string())?;
            let items = inputs
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Some(Icon::from_name("volume")),
                })
                .collect();
            Ok(no_context(items))
        }
        PickerKind::Transition => {
            let transitions = client.transitions().await.map_err(|e| e.to_string())?;
            let items = transitions
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Some(Icon::from_name("transition-right")),
                })
                .collect();
            Ok(no_context(items))
        }
        PickerKind::Profile => {
            let profiles = client.profiles().await.map_err(|e| e.to_string())?;
            let items = profiles
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Some(Icon::from_name("user-cog")),
                })
                .collect();
            Ok(no_context(items))
        }
        PickerKind::SceneCollection => {
            let collections = client
                .scene_collections()
                .await
                .map_err(|e| e.to_string())?;
            let items = collections
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Some(Icon::from_name("layout-2")),
                })
                .collect();
            Ok(no_context(items))
        }
        PickerKind::Hotkey
        | PickerKind::Expression
        | PickerKind::MidiPort
        | PickerKind::Model
        | PickerKind::Item
        | PickerKind::ItemInstance => Err(labels.unavailable),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Mutex;

    use forge_components::{Density, Presentation, ThemeId};
    use forge_platform_core::{
        CollectionFailure, CollectionRevisionSignal, CollectionRevisions, OptionHint,
        OptionListMetadata, QuickActionAccent, QuickActionLiveness, SectionIcon,
    };
    use forge_storage::Language;
    use gpui::TestAppContext;
    use tokio::sync::oneshot;

    use super::*;
    use crate::i18n::{install_language, message_in};
    use crate::test_support::{pump, runtime};

    const SETTLE_ROUNDS: usize = 4;

    type Resolved = CollectionOutcome<Vec<OptionEntry>>;
    type ResolveCall = (String, BTreeMap<String, QuickActionFieldValue>);
    type HeldResolve = Option<(String, oneshot::Sender<Resolved>)>;

    struct FakeOptionSource {
        lists: Vec<OptionListMetadata>,
        entries: Mutex<BTreeMap<String, Resolved>>,
        held: bool,
        pending: Mutex<Vec<HeldResolve>>,
        calls: Mutex<Vec<ResolveCall>>,
        signal: CollectionRevisionSignal,
    }

    impl FakeOptionSource {
        fn new(lists: Vec<OptionListMetadata>, held: bool) -> Arc<Self> {
            Arc::new(Self {
                lists,
                entries: Mutex::new(BTreeMap::new()),
                held,
                pending: Mutex::new(Vec::new()),
                calls: Mutex::new(Vec::new()),
                signal: CollectionRevisionSignal::new(),
            })
        }

        fn answering(lists: Vec<OptionListMetadata>, answers: Vec<(&str, Resolved)>) -> Arc<Self> {
            let fake = Self::new(lists, false);
            for (list, answer) in answers {
                fake.answer(list, answer);
            }
            fake
        }

        fn answer(&self, list: &str, answer: Resolved) {
            self.entries.lock().unwrap().insert(list.to_owned(), answer);
        }

        fn release(&self, position: usize, answer: Resolved) {
            let (_, sender) = self.pending.lock().unwrap()[position].take().unwrap();
            sender.send(answer).unwrap();
        }

        fn release_latest(&self, list: &str, answer: Resolved) {
            let position = self
                .pending
                .lock()
                .unwrap()
                .iter()
                .rposition(|slot| slot.as_ref().is_some_and(|(name, _)| name == list))
                .unwrap();
            self.release(position, answer);
        }

        fn take_calls(&self) -> Vec<ResolveCall> {
            std::mem::take(&mut *self.calls.lock().unwrap())
        }
    }

    #[async_trait::async_trait]
    impl BuiltinOptionSource for FakeOptionSource {
        fn option_lists(&self) -> Vec<OptionListMetadata> {
            self.lists.clone()
        }

        fn revisions(&self) -> CollectionRevisions {
            self.signal.subscribe()
        }

        async fn resolve(
            &self,
            list: &OptionListId,
            depends_on_values: &BTreeMap<String, QuickActionFieldValue>,
        ) -> Resolved {
            self.calls
                .lock()
                .unwrap()
                .push((list.to_string(), depends_on_values.clone()));
            if !self.held {
                return self
                    .entries
                    .lock()
                    .unwrap()
                    .get(list.as_str())
                    .cloned()
                    .unwrap_or(Ok(Vec::new()));
            }
            let (sender, receiver) = oneshot::channel();
            self.pending
                .lock()
                .unwrap()
                .push(Some((list.to_string(), sender)));
            receiver.await.unwrap_or(Err(CollectionFailure::Transport))
        }
    }

    #[derive(Debug, PartialEq)]
    enum Seen {
        Loading,
        Ready(Vec<String>),
        Failed(String),
    }

    struct Harness {
        modal: Entity<QuickActionModal>,
        rt: tokio::runtime::Runtime,
    }

    impl Harness {
        fn open(
            cx: &mut TestAppContext,
            source: Option<Arc<FakeOptionSource>>,
            fields: Vec<QuickActionField>,
        ) -> Self {
            install_language(Language::Uk);
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            });
            let rt = runtime();
            let handle = rt.handle().clone();
            let source = source.map(|fake| fake as Arc<dyn BuiltinOptionSource>);
            let modal = cx.update(|cx| {
                cx.new(|cx| QuickActionModal::new(option_action(fields), None, source, handle, cx))
            });
            let harness = Self { modal, rt };
            harness.settle(cx);
            harness
        }

        fn settle(&self, cx: &mut TestAppContext) {
            for _ in 0..SETTLE_ROUNDS {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }

        fn seen(&self, cx: &mut TestAppContext, index: usize) -> Seen {
            self.modal
                .read_with(cx, |modal, _| match &modal.fields[index].control {
                    FieldControl::Choice(choice) => match &choice.state {
                        ChoiceState::Loading => Seen::Loading,
                        ChoiceState::Ready(items) => {
                            Seen::Ready(items.iter().map(|item| item.label.to_string()).collect())
                        }
                        ChoiceState::Failed(text) => Seen::Failed(text.clone()),
                    },
                    _ => panic!("field {index} is not a choice"),
                })
        }

        fn selected(&self, cx: &mut TestAppContext, index: usize) -> Option<String> {
            self.modal.read_with(cx, |modal, _| {
                modal.fields[index]
                    .selected_choice()
                    .map(|selected| selected.to_string())
            })
        }

        fn can_run(&self, cx: &mut TestAppContext) -> bool {
            self.modal.update(cx, |modal, cx| modal.can_run(cx))
        }

        fn pick(&self, cx: &mut TestAppContext, index: usize, value: &str) {
            let event = PickerEvent::Selected(SharedString::from(value.to_owned()));
            self.modal
                .update(cx, |modal, cx| modal.on_choice_picker(index, &event, cx));
            self.settle(cx);
        }

        fn retry(&self, cx: &mut TestAppContext, index: usize) {
            self.modal
                .update(cx, |modal, cx| modal.retry_dynamic(index, cx));
            self.settle(cx);
        }
    }

    fn option_action(fields: Vec<QuickActionField>) -> QuickAction {
        let mut action = picker_only_action(PickerKind::Scene);
        action.picker = None;
        action.fields = fields;
        action
    }

    fn option_field(key: &str, list: &str, default: Option<&str>) -> QuickActionField {
        QuickActionField {
            key: key.to_owned(),
            label: key.to_owned(),
            kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(
                OptionListId::new(list),
            )),
            default: default.map(|value| QuickActionFieldValue::Text(value.to_owned())),
            placeholder: None,
            hint: None,
            required: true,
        }
    }

    fn list(id: &str) -> OptionListMetadata {
        OptionListMetadata::new(OptionListId::new(id))
    }

    fn entry(value: &str) -> OptionEntry {
        OptionEntry::new(value, value)
    }

    fn values_of(selected: Option<SharedString>) -> Option<String> {
        selected.map(|value| value.to_string())
    }

    #[test]
    fn preselect_prefers_a_kept_selection_then_the_single_current_entry_then_a_listed_default() {
        let one_current = vec![entry("Main"), entry("BRB").as_current(), entry("Ending")];
        let no_current = vec![entry("Main"), entry("BRB")];
        let two_current = vec![entry("Main").as_current(), entry("BRB").as_current()];
        for (entries, previous, default, expected) in [
            (&one_current, Some("Ending"), Some("Main"), Some("Ending")),
            (&one_current, Some("Gone"), Some("Main"), Some("BRB")),
            (&one_current, None, None, Some("BRB")),
            (&no_current, Some("Gone"), Some("Main"), Some("Main")),
            (&no_current, None, Some("Gone"), None),
            (&no_current, None, None, None),
            (&two_current, None, None, None),
            (&two_current, None, Some("Main"), Some("Main")),
            (&Vec::new(), Some("Main"), Some("Main"), None),
        ] {
            let previous = previous.map(SharedString::from);
            let default = default.map(SharedString::from);

            assert_eq!(
                values_of(option_preselect(
                    entries,
                    previous.as_ref(),
                    default.as_ref()
                )),
                expected.map(str::to_owned),
                "previous {previous:?} default {default:?}"
            );
        }
    }

    #[test]
    fn an_option_entry_keeps_its_label_verbatim_and_translates_only_its_hint() {
        install_language(Language::Uk);
        let entry = OptionEntry::new("scene-1", "option_hint_live")
            .with_hint(OptionHint::new("option_hint_current_scene").arg("scene", "Main"));

        let item = option_item(&entry);

        assert_eq!(
            (
                item.id.to_string(),
                item.label.to_string(),
                item.sublabel
                    .map(|hint| hint.replace(['\u{2068}', '\u{2069}'], "")),
            ),
            (
                "scene-1".to_owned(),
                "option_hint_live".to_owned(),
                Some("Зараз: Main (В ЕФІРІ)".to_owned()),
            )
        );
    }

    #[gpui::test]
    fn run_stays_disabled_while_the_list_loads_and_enables_once_a_preselected_list_arrives(
        cx: &mut TestAppContext,
    ) {
        let fake = FakeOptionSource::new(vec![list("scenes")], true);
        let harness = Harness::open(
            cx,
            Some(Arc::clone(&fake)),
            vec![option_field("scene", "scenes", Some("Main"))],
        );
        let while_loading = harness.can_run(cx);

        fake.release(0, Ok(vec![entry("Main"), entry("BRB")]));
        harness.settle(cx);

        assert_eq!((while_loading, harness.can_run(cx)), (false, true));
    }

    #[gpui::test]
    fn run_stays_disabled_when_the_ready_list_has_nothing_to_preselect(cx: &mut TestAppContext) {
        let fake = FakeOptionSource::answering(
            vec![list("scenes")],
            vec![("scenes", Ok(vec![entry("Main"), entry("BRB")]))],
        );
        let harness = Harness::open(
            cx,
            Some(fake),
            vec![option_field("scene", "scenes", Some("Gone"))],
        );

        assert_eq!(
            (harness.seen(cx, 0), harness.can_run(cx)),
            (
                Seen::Ready(vec!["Main".to_owned(), "BRB".to_owned()]),
                false
            )
        );
    }

    #[gpui::test]
    fn run_is_disabled_while_a_dependent_list_reloads_even_though_its_selection_is_kept(
        cx: &mut TestAppContext,
    ) {
        let fake = FakeOptionSource::new(
            vec![list("scenes"), list("sources").depending_on("scene")],
            true,
        );
        let harness = Harness::open(
            cx,
            Some(Arc::clone(&fake)),
            vec![
                option_field("scene", "scenes", None),
                option_field("source", "sources", None),
            ],
        );
        fake.release_latest("scenes", Ok(vec![entry("Main").as_current(), entry("BRB")]));
        harness.settle(cx);
        fake.release_latest("sources", Ok(vec![entry("Camera").as_current()]));
        harness.settle(cx);
        let before_change = harness.can_run(cx);

        harness.pick(cx, 0, "BRB");

        assert_eq!(
            (
                before_change,
                harness.seen(cx, 1),
                harness.selected(cx, 1),
                harness.can_run(cx)
            ),
            (true, Seen::Loading, Some("Camera".to_owned()), false)
        );
    }

    #[gpui::test]
    fn a_list_the_source_does_not_declare_renders_unavailable_without_resolving(
        cx: &mut TestAppContext,
    ) {
        for source in [
            Some(FakeOptionSource::new(vec![list("scenes")], false)),
            None,
        ] {
            let harness = Harness::open(
                cx,
                source.clone(),
                vec![option_field("source", "sources", None)],
            );

            assert_eq!(
                harness.seen(cx, 0),
                Seen::Failed(message_in(Language::Uk, "integration_qa_field_unavailable"))
            );
            assert!(source.is_none_or(|fake| fake.take_calls().is_empty()));
        }
    }

    #[gpui::test]
    fn a_failed_resolve_shows_the_forge_failure_text_and_blocks_run(cx: &mut TestAppContext) {
        let fake = FakeOptionSource::answering(
            vec![list("scenes")],
            vec![("scenes", Err(CollectionFailure::RateLimited))],
        );
        let harness = Harness::open(
            cx,
            Some(fake),
            vec![option_field("scene", "scenes", Some("Main"))],
        );

        assert_eq!(
            (harness.seen(cx, 0), harness.can_run(cx)),
            (
                Seen::Failed(message_in(
                    Language::Uk,
                    "integration_qa_options_rate_limited"
                )),
                false
            )
        );
    }

    #[gpui::test]
    fn a_late_result_from_a_superseded_resolve_is_dropped(cx: &mut TestAppContext) {
        let fake = FakeOptionSource::new(vec![list("scenes")], true);
        let harness = Harness::open(
            cx,
            Some(Arc::clone(&fake)),
            vec![option_field("scene", "scenes", None)],
        );
        harness.retry(cx, 0);

        fake.release(1, Ok(vec![entry("Fresh")]));
        harness.settle(cx);
        fake.release(0, Ok(vec![entry("Stale")]));
        harness.settle(cx);

        assert_eq!(harness.seen(cx, 0), Seen::Ready(vec!["Fresh".to_owned()]));
    }

    #[gpui::test]
    fn changing_a_dependency_re_resolves_only_the_dependent_list_with_the_new_value(
        cx: &mut TestAppContext,
    ) {
        let fake = FakeOptionSource::answering(
            vec![
                list("scenes"),
                list("sources").depending_on("scene"),
                list("devices"),
            ],
            vec![
                ("scenes", Ok(vec![entry("Main").as_current(), entry("BRB")])),
                ("sources", Ok(vec![entry("Camera")])),
                ("devices", Ok(vec![entry("Mic")])),
            ],
        );
        let harness = Harness::open(
            cx,
            Some(Arc::clone(&fake)),
            vec![
                option_field("scene", "scenes", None),
                option_field("source", "sources", None),
                option_field("device", "devices", None),
            ],
        );
        fake.take_calls();

        harness.pick(cx, 0, "BRB");

        assert_eq!(
            fake.take_calls(),
            vec![(
                "sources".to_owned(),
                BTreeMap::from([(
                    "scene".to_owned(),
                    QuickActionFieldValue::Text("BRB".to_owned())
                )]),
            )]
        );
    }

    #[gpui::test]
    fn picking_the_already_selected_value_does_not_re_resolve_dependents(cx: &mut TestAppContext) {
        let fake = FakeOptionSource::answering(
            vec![list("scenes"), list("sources").depending_on("scene")],
            vec![("scenes", Ok(vec![entry("Main").as_current()]))],
        );
        let harness = Harness::open(
            cx,
            Some(Arc::clone(&fake)),
            vec![
                option_field("scene", "scenes", None),
                option_field("source", "sources", None),
            ],
        );
        fake.take_calls();

        harness.pick(cx, 0, "Main");

        assert!(fake.take_calls().is_empty());
    }

    #[gpui::test]
    fn a_revision_bump_re_resolves_the_open_modal_with_the_new_entries(cx: &mut TestAppContext) {
        let fake = FakeOptionSource::answering(
            vec![list("scenes")],
            vec![("scenes", Ok(vec![entry("Main")]))],
        );
        let harness = Harness::open(
            cx,
            Some(Arc::clone(&fake)),
            vec![option_field("scene", "scenes", None)],
        );
        fake.answer("scenes", Ok(vec![entry("Main"), entry("Live")]));

        fake.signal.bump();
        harness.settle(cx);

        assert_eq!(
            harness.seen(cx, 0),
            Seen::Ready(vec!["Main".to_owned(), "Live".to_owned()])
        );
    }

    #[gpui::test]
    fn a_revision_refresh_keeps_the_ready_list_and_selection_until_the_new_result_lands(
        cx: &mut TestAppContext,
    ) {
        let fake = FakeOptionSource::new(vec![list("scenes")], true);
        let harness = Harness::open(
            cx,
            Some(Arc::clone(&fake)),
            vec![option_field("scene", "scenes", None)],
        );
        fake.release(0, Ok(vec![entry("Main").as_current()]));
        harness.settle(cx);

        fake.signal.bump();
        harness.settle(cx);

        assert_eq!(
            (
                harness.seen(cx, 0),
                harness.selected(cx, 0),
                harness.can_run(cx),
                fake.take_calls().len()
            ),
            (
                Seen::Ready(vec!["Main".to_owned()]),
                Some("Main".to_owned()),
                true,
                2
            )
        );
    }

    fn picker_only_action(picker: PickerKind) -> QuickAction {
        QuickAction {
            label: "Switch scene".to_owned(),
            icon: SectionIcon::new("arrows-shuffle"),
            enabled: true,
            locked_reason: None,
            liveness: QuickActionLiveness::Unknown,
            group: None,
            group_icon: None,
            group_accent: None,
            destructive: false,
            accent: QuickActionAccent::Brand,
            subaction_template: SubActionStep {
                kind_id: "obs.scenes.switch_current".to_owned(),
                config: BTreeMap::from([("scene".to_owned(), Variant::String(String::new()))]),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            },
            picker: Some(picker),
            fields: Vec::new(),
            collection: None,
        }
    }

    #[test]
    fn a_picker_only_action_merges_the_picked_value_into_the_step_config() {
        let action = picker_only_action(PickerKind::Scene);
        let mut merge_action = action.clone();
        merge_action.fields = build_specs(&action).iter().map(spec_to_field).collect();
        let values = BTreeMap::from([(
            "scene".to_owned(),
            QuickActionFieldValue::Text("Gameplay".to_owned()),
        )]);

        let step = merge_action.merge_config(&values);

        assert_eq!(
            step.config.get("scene"),
            Some(&Variant::String("Gameplay".to_owned())),
        );
    }

    #[test]
    fn declared_fields_take_precedence_over_the_action_picker() {
        let mut action = picker_only_action(PickerKind::Scene);
        action.fields = vec![QuickActionField {
            key: "scene".to_owned(),
            label: "Scene".to_owned(),
            kind: QuickActionFieldKind::Text,
            default: Some(QuickActionFieldValue::Text("BRB".to_owned())),
            placeholder: None,
            hint: None,
            required: false,
        }];

        let specs = build_specs(&action);

        assert_eq!(specs.len(), 1);
        assert!(matches!(specs[0].kind, QuickActionFieldKind::Text));
    }
}
