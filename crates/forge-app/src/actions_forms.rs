use std::collections::BTreeMap;

use forge_types::{ActionId, ClipId, QueueId, SubActionStep, TriggerInstanceId, Variant};

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
    pub available_actions: Vec<(ActionId, String)>,
    pub available_queues: Vec<(QueueId, String)>,
    pub available_trigger_instances: Vec<(TriggerInstanceId, String)>,
    pub available_scripts: Vec<String>,
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
            available_actions: vec![],
            available_queues: vec![],
            available_trigger_instances: vec![],
            available_scripts: vec![],
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
    QueuesLoaded(Vec<(QueueId, String)>),
    TriggerInstancesLoaded(Vec<(TriggerInstanceId, String)>),
    ScriptNamesLoaded(Vec<String>),
    Cancel,
    Submit,
    Saved(Result<(), String>),
    DuplicateRequested(ActionId, usize),
    Duplicated(Result<ActionId, String>),
}

#[derive(Debug, Clone)]
pub enum RemoveSubActionMsg {
    /// Arms the shared destructive-confirm gate — no longer removes directly.
    Requested(ActionId, usize),
    /// Confirmed via the modal: performs the removal previously done by `Requested`.
    ConfirmAccepted(ActionId, usize),
    /// Cancelled via the modal (button or backdrop click).
    ConfirmDismissed,
    Removed(Result<(), String>),
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use forge_registry::SubActionRunner;
    use forge_runtime::audio_runners::PlaySoundRunner;
    use forge_runtime::sound_player::{SoundPlayer, SoundPlayerError};
    use forge_types::{ClipId, OutputDevice};

    use super::*;

    struct NoopPlayer;

    #[async_trait::async_trait]
    impl SoundPlayer for NoopPlayer {
        async fn play(
            &self,
            _clip_id: ClipId,
            _override_device: Option<OutputDevice>,
        ) -> Result<(), SoundPlayerError> {
            Ok(())
        }
    }

    fn step_with_config(kind_id: &str, config: BTreeMap<String, Variant>) -> SubActionStep {
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            label: None,
        }
    }

    #[test]
    fn build_step_returns_none_while_no_kind_is_selected() {
        let form = AddSubActionForm::new(ActionId::new());
        assert!(form.build_step().is_none());
    }

    #[test]
    fn build_step_carries_the_override_buffer_as_the_step_config() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.selected_kind_id = Some("twitch.chat.send_message".to_owned());
        form.overrides_buffer
            .insert("message".to_owned(), Variant::String("hello".to_owned()));

        let step = form.build_step().unwrap();
        assert_eq!(step.kind_id, "twitch.chat.send_message");
        assert_eq!(
            step.config.get("message"),
            Some(&Variant::String("hello".to_owned()))
        );
    }

    #[test]
    fn seed_from_default_then_edited_field_overlays_while_untouched_field_keeps_default() {
        let mut form = AddSubActionForm::new(ActionId::new());
        form.selected_kind_id = Some("soundboard.sound.play".to_owned());
        let mut defaults = BTreeMap::new();
        defaults.insert("clip_id".to_owned(), Variant::String(String::new()));
        defaults.insert("volume".to_owned(), Variant::Int(100));
        form.seed_from_default(defaults);

        // User overrides only clip_id; volume is left untouched.
        form.overrides_buffer
            .insert("clip_id".to_owned(), Variant::String("clip-7".to_owned()));

        let step = form.build_step().unwrap();
        assert_eq!(
            step.config.get("clip_id"),
            Some(&Variant::String("clip-7".to_owned()))
        );
        assert_eq!(step.config.get("volume"), Some(&Variant::Int(100)));
    }

    #[test]
    fn populate_then_build_reproduces_a_twitch_step_config() {
        let mut config = BTreeMap::new();
        config.insert(
            "message".to_owned(),
            Variant::String("gg %user%".to_owned()),
        );
        config.insert("reply_to".to_owned(), Variant::String(String::new()));
        let original = step_with_config("twitch.chat.send_message", config);

        let mut form = AddSubActionForm::new(ActionId::new());
        form.populate_from_step(&original);

        assert_eq!(form.step, SubActionFormStep::FillForm);
        let rebuilt = form.build_step().unwrap();
        assert_eq!(rebuilt.kind_id, original.kind_id);
        assert_eq!(rebuilt.config, original.config);
    }

    #[test]
    fn populate_then_build_reproduces_a_builtin_step_config() {
        let mut config = BTreeMap::new();
        config.insert("clip_id".to_owned(), Variant::String("clip-42".to_owned()));
        let original = step_with_config("soundboard.sound.play", config);

        let mut form = AddSubActionForm::new(ActionId::new());
        form.populate_from_step(&original);

        let rebuilt = form.build_step().unwrap();
        assert_eq!(rebuilt.kind_id, original.kind_id);
        assert_eq!(rebuilt.config, original.config);
    }

    #[test]
    fn real_runner_rejects_the_seeded_empty_required_config() {
        // The Submit path runs `runner.validate_config(&form.overrides_buffer)`.
        // A freshly seeded PlaySound form has an empty clip_id and must fail.
        let runner = PlaySoundRunner::new(Arc::new(NoopPlayer));
        let mut form = AddSubActionForm::new(ActionId::new());
        form.selected_kind_id = Some(runner.id().to_owned());
        form.seed_from_default(runner.default_config());

        assert!(runner.validate_config(&form.overrides_buffer).is_err());
    }

    #[test]
    fn real_runner_accepts_a_filled_required_config() {
        let runner = PlaySoundRunner::new(Arc::new(NoopPlayer));
        let mut form = AddSubActionForm::new(ActionId::new());
        form.selected_kind_id = Some(runner.id().to_owned());
        form.seed_from_default(runner.default_config());
        form.overrides_buffer.insert(
            "clip_id".to_owned(),
            Variant::String(ClipId::new().to_string()),
        );

        assert!(runner.validate_config(&form.overrides_buffer).is_ok());
    }
}
