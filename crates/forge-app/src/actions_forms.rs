use std::collections::BTreeMap;

use forge_types::{ActionId, ClipId, QueueId, SubActionStep, Variant};

use crate::actions_field_form::variant_to_display_str;

pub struct AddActionForm {
    pub name: String,
    pub group: String,
    pub queue_id: Option<QueueId>,
    pub description: String,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    pub random_pick: bool,
    pub queue_options: Vec<(QueueId, String)>,
    pub selected_queue_name: Option<String>,
    pub error: Option<String>,
    pub saving: bool,
}

impl AddActionForm {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            group: String::new(),
            queue_id: None,
            description: String::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            random_pick: false,
            queue_options: vec![],
            selected_queue_name: None,
            error: None,
            saving: false,
        }
    }

    pub fn set_queue_options(&mut self, opts: Vec<(QueueId, String)>) {
        let default = opts
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case("default"))
            .cloned();
        self.queue_options = opts;
        if let Some((id, name)) = default {
            self.queue_id = Some(id);
            self.selected_queue_name = Some(name);
        }
    }

    pub fn select_queue_by_name(&mut self, name: String) {
        let found = self.queue_options.iter().find(|(_, n)| *n == name);
        if let Some((id, _)) = found {
            self.queue_id = Some(*id);
        }
        self.selected_queue_name = Some(name);
    }

    pub fn is_valid(&self) -> bool {
        !self.name.trim().is_empty() && self.queue_id.is_some()
    }
}

impl Default for AddActionForm {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum AddActionMsg {
    OpenRequested,
    QueueOptionsLoaded(Result<Vec<(QueueId, String)>, String>),
    NameChanged(String),
    GroupChanged(String),
    QueueSelected(String),
    DescriptionChanged(String),
    EnabledToggled(bool),
    ConcurrentToggled(bool),
    BypassPauseToggled(bool),
    RandomPickToggled(bool),
    Cancel,
    Submit,
    Saved(Result<ActionId, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubActionFormStep {
    PickKind,
    FillForm,
}

#[derive(Debug)]
pub struct AddSubActionForm {
    pub for_action_id: ActionId,
    pub step: SubActionFormStep,
    pub selected_kind_id: Option<String>,
    pub search: String,
    pub overrides_buffer: BTreeMap<String, Variant>,
    pub text_buffer: BTreeMap<String, String>,
    pub available_clips: Vec<(ClipId, String)>,
    pub error: Option<String>,
    pub saving: bool,
    pub editing_index: Option<usize>,
}

impl AddSubActionForm {
    pub fn new(for_action_id: ActionId) -> Self {
        Self {
            for_action_id,
            step: SubActionFormStep::PickKind,
            selected_kind_id: None,
            search: String::new(),
            overrides_buffer: BTreeMap::new(),
            text_buffer: BTreeMap::new(),
            available_clips: vec![],
            error: None,
            saving: false,
            editing_index: None,
        }
    }

    /// Seeds the override + display buffers from a runner's `default_config()`.
    pub fn seed_from_default(&mut self, default_config: BTreeMap<String, Variant>) {
        let mut text_buf = BTreeMap::new();
        for (k, v) in &default_config {
            text_buf.insert(k.clone(), variant_to_display_str(v));
        }
        self.overrides_buffer = default_config;
        self.text_buffer = text_buf;
    }

    /// Fills the buffers from a persisted step keyed by config entry, independent
    /// of `kind_id` — every value is mapped without per-kind branching.
    pub fn populate_from_step(&mut self, step: &SubActionStep) {
        self.selected_kind_id = Some(step.kind_id.clone());
        self.step = SubActionFormStep::FillForm;
        let mut text_buf = BTreeMap::new();
        for (k, v) in &step.config {
            text_buf.insert(k.clone(), variant_to_display_str(v));
        }
        self.overrides_buffer = step.config.clone();
        self.text_buffer = text_buf;
    }

    pub fn build_step(&self) -> Option<SubActionStep> {
        let kind_id = self.selected_kind_id.clone()?;
        Some(SubActionStep {
            kind_id,
            config: self.overrides_buffer.clone(),
            enabled: true,
            label: None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum AddSubActionMsg {
    OpenRequested(ActionId),
    EditRequested(ActionId, usize),
    KindSelected(String),
    BackToKindPicker,
    SearchChanged(String),
    FieldChanged(String, Variant),
    IntInputChanged(String, String),
    FieldCleared(String),
    ClipsLoaded(Vec<(ClipId, String)>),
    Cancel,
    Submit,
    Saved(Result<(), String>),
    DuplicateRequested(ActionId, usize),
    Duplicated(Result<ActionId, String>),
}

#[derive(Debug, Clone)]
pub enum RemoveSubActionMsg {
    Requested(ActionId, usize),
    Removed(Result<(), String>),
}
