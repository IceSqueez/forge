use forge_components::{
    ActivePresentation, InputEvent, Picker, PickerEvent, PickerItem, PickerLabels, TextInput,
    VariablePicker, VariablePickerEvent, VariablePickerLabels, tr,
};
use forge_overlay::OverlayConfig;
use forge_registry::ScopedVariable;
use forge_runtime::{ActionSummary, WiringTarget};
use forge_types::{ActionId, QueueId};
use gpui::{App, Context, Entity, SharedString, Subscription, Window, prelude::*};
use time::OffsetDateTime;

use super::plan::{self, WordingSpec};
use super::{EventWiringView, WiringStage};
use crate::automation::actions_screen::variable_groups;

const NO_QUEUE_ITEM: &str = "none";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TargetMode {
    New,
    Existing,
}

pub(super) struct WordingField {
    pub key: String,
    pub label: String,
    pub input: Entity<TextInput>,
}

pub(super) struct ActionOption {
    pub id: ActionId,
    pub name: String,
    pub queue_name: String,
}

#[derive(Default)]
pub(super) struct WiringChoices {
    pub actions: Vec<ActionOption>,
    pub queues: Vec<(QueueId, String)>,
}

pub(super) enum Popover {
    Queue {
        picker: Entity<Picker>,
        _sub: Subscription,
    },
    Action {
        picker: Entity<Picker>,
        _sub: Subscription,
    },
    Variable {
        key: String,
        picker: Entity<VariablePicker>,
        _sub: Subscription,
    },
}

pub(super) struct WiringForm {
    pub mode: TargetMode,
    pub name: Entity<TextInput>,
    pub queue: Option<QueueId>,
    pub existing: Option<ActionId>,
    pub wording: Vec<WordingField>,
    pub choices: WiringChoices,
    pub scope: Vec<ScopedVariable>,
    pub popover: Option<Popover>,
    _subs: Vec<Subscription>,
}

impl WiringForm {
    pub(super) fn target(&self, cx: &App) -> Option<WiringTarget> {
        match self.mode {
            TargetMode::New => Some(WiringTarget::NewAction {
                name: self.name.read(cx).content().to_owned(),
                queue_id: self.queue,
            }),
            TargetMode::Existing => self.existing.map(WiringTarget::ExistingAction),
        }
    }

    pub(super) fn content(&self, cx: &App) -> OverlayConfig {
        plan::written_content(
            self.wording
                .iter()
                .map(|field| (field.key.as_str(), field.input.read(cx).content())),
        )
    }

    pub(super) fn queue_label(&self) -> Option<String> {
        let id = self.queue?;
        self.choices
            .queues
            .iter()
            .find(|(queue_id, _)| *queue_id == id)
            .map(|(_, name)| name.clone())
    }

    pub(super) fn existing_label(&self) -> Option<String> {
        let id = self.existing?;
        self.choices
            .actions
            .iter()
            .find(|option| option.id == id)
            .map(|option| option.name.clone())
    }
}

pub(super) fn action_options(mut summaries: Vec<ActionSummary>) -> Vec<ActionOption> {
    summaries.sort_by_key(|summary| summary.name.to_lowercase());
    summaries
        .into_iter()
        .map(|summary| ActionOption {
            id: summary.id,
            name: summary.name,
            queue_name: summary.queue_name,
        })
        .collect()
}

impl EventWiringView {
    pub(super) fn build_form(
        &self,
        action_name: String,
        wording: Vec<WordingSpec>,
        cx: &mut Context<Self>,
    ) -> WiringForm {
        let name = cx.new(|cx| {
            let mut input = TextInput::new(tr!("actions_name_placeholder"), cx);
            input.set_content(action_name, cx);
            input
        });
        let mut subs = vec![cx.subscribe(&name, Self::on_form_input)];
        let wording: Vec<WordingField> = wording
            .into_iter()
            .map(|spec| {
                let input = cx.new(|cx| {
                    let mut input = TextInput::new(spec.placeholder, cx);
                    input.set_content(spec.seed, cx);
                    input
                });
                subs.push(cx.subscribe(&input, Self::on_form_input));
                WordingField {
                    key: spec.key,
                    label: spec.label,
                    input,
                }
            })
            .collect();
        WiringForm {
            mode: TargetMode::New,
            name,
            queue: None,
            existing: None,
            wording,
            choices: WiringChoices::default(),
            scope: Vec::new(),
            popover: None,
            _subs: subs,
        }
    }

    fn on_form_input(
        &mut self,
        _input: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Changed(_)) {
            self.replan(cx);
        }
    }

    pub(super) fn form_mut(&mut self) -> Option<&mut WiringForm> {
        match self.stage.as_mut() {
            Some(WiringStage::Confirm(stage)) => Some(&mut stage.form),
            _ => None,
        }
    }

    pub(super) fn set_mode(&mut self, mode: TargetMode, cx: &mut Context<Self>) {
        let Some(form) = self.form_mut() else {
            return;
        };
        if form.mode == mode {
            return;
        }
        form.mode = mode;
        form.popover = None;
        self.replan(cx);
    }

    pub(super) fn open_queue_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form_mut() else {
            return;
        };
        if matches!(form.popover, Some(Popover::Queue { .. })) {
            form.popover = None;
            cx.notify();
            return;
        }
        let mut items = vec![PickerItem {
            id: NO_QUEUE_ITEM.into(),
            label: tr!("overlays_wire_queue_none").into(),
            sublabel: Some(tr!("overlays_wire_queue_none_hint").into()),
            icon: None,
        }];
        items.extend(
            form.choices
                .queues
                .iter()
                .enumerate()
                .map(|(index, (_, name))| PickerItem {
                    id: index.to_string().into(),
                    label: name.clone().into(),
                    sublabel: None,
                    icon: None,
                }),
        );
        let current = match form.queue {
            None => Some(SharedString::from(NO_QUEUE_ITEM)),
            Some(id) => form
                .choices
                .queues
                .iter()
                .position(|(queue_id, _)| *queue_id == id)
                .map(|index| index.to_string().into()),
        };
        let labels = PickerLabels {
            placeholder: tr!("overlays_wire_queue_search").into(),
            empty: tr!("widget_picker_no_results").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let picker = cx.new(|cx| Picker::new(labels, items, cx).with_current(current));
        let sub = cx.subscribe_in(&picker, window, Self::on_queue_picked);
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        if let Some(form) = self.form_mut() {
            form.popover = Some(Popover::Queue { picker, _sub: sub });
        }
        cx.notify();
    }

    fn on_queue_picked(
        &mut self,
        _picker: &Entity<Picker>,
        event: &PickerEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.form_mut() else {
            return;
        };
        form.popover = None;
        let PickerEvent::Selected(id) = event else {
            cx.notify();
            return;
        };
        form.queue = id
            .parse::<usize>()
            .ok()
            .and_then(|index| form.choices.queues.get(index))
            .map(|(queue_id, _)| *queue_id);
        self.replan(cx);
    }

    pub(super) fn open_action_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form_mut() else {
            return;
        };
        if matches!(form.popover, Some(Popover::Action { .. })) {
            form.popover = None;
            cx.notify();
            return;
        }
        let items: Vec<PickerItem> = form
            .choices
            .actions
            .iter()
            .enumerate()
            .map(|(index, option)| PickerItem {
                id: index.to_string().into(),
                label: option.name.clone().into(),
                sublabel: Some(option.queue_name.clone().into()),
                icon: None,
            })
            .collect();
        let current = form.existing.and_then(|id| {
            form.choices
                .actions
                .iter()
                .position(|option| option.id == id)
                .map(|index| SharedString::from(index.to_string()))
        });
        let labels = PickerLabels {
            placeholder: tr!("overlays_wire_action_search").into(),
            empty: tr!("overlays_wire_action_none").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let picker = cx.new(|cx| Picker::new(labels, items, cx).with_current(current));
        let sub = cx.subscribe_in(&picker, window, Self::on_action_picked);
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        if let Some(form) = self.form_mut() {
            form.popover = Some(Popover::Action { picker, _sub: sub });
        }
        cx.notify();
    }

    fn on_action_picked(
        &mut self,
        _picker: &Entity<Picker>,
        event: &PickerEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.form_mut() else {
            return;
        };
        form.popover = None;
        let PickerEvent::Selected(id) = event else {
            cx.notify();
            return;
        };
        let picked = id
            .parse::<usize>()
            .ok()
            .and_then(|index| form.choices.actions.get(index))
            .map(|option| option.id);
        if picked.is_some() {
            form.existing = picked;
        }
        self.replan(cx);
    }

    pub(super) fn open_variable_picker(
        &mut self,
        key: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let language = crate::app::presentation::active_names_language(cx);
        let Some(form) = self.form_mut() else {
            return;
        };
        if matches!(&form.popover, Some(Popover::Variable { key: open, .. }) if *open == key) {
            form.popover = None;
            cx.notify();
            return;
        }
        let groups = variable_groups(&form.scope, language, OffsetDateTime::now_utc(), &palette);
        let labels = VariablePickerLabels {
            placeholder: tr!("actions_var_picker_search").into(),
            empty: tr!("actions_var_picker_empty").into(),
            no_format: tr!("actions_var_picker_no_format").into(),
        };
        let picker = cx.new(|cx| VariablePicker::new(labels, groups, window, cx));
        let sub = cx.subscribe_in(&picker, window, Self::on_variable_picked);
        picker.update(cx, |picker, cx| picker.focus(window, cx));
        if let Some(form) = self.form_mut() {
            form.popover = Some(Popover::Variable {
                key,
                picker,
                _sub: sub,
            });
        }
        cx.notify();
    }

    fn on_variable_picked(
        &mut self,
        _picker: &Entity<VariablePicker>,
        event: &VariablePickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.form_mut() else {
            return;
        };
        let Some(Popover::Variable { key, .. }) = form.popover.take() else {
            cx.notify();
            return;
        };
        let Some(input) = form
            .wording
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.input.clone())
        else {
            cx.notify();
            return;
        };
        if let VariablePickerEvent::Picked(token) = event {
            input.update(cx, |input, cx| input.insert_at_caret(token, cx));
        }
        input.update(cx, |input, cx| input.focus(window, cx));
        self.replan(cx);
    }

    pub(super) fn close_popover(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.form_mut() {
            form.popover = None;
        }
        cx.notify();
    }
}
