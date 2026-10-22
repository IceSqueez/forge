use super::*;
use crate::actions_screen::step_validation::{StepRejection, step_rejection};
use crate::actions_screen::sub_action_modal::{
    EditSubActionForm, SubFormCommit, SubFormEvent, SubFormLaunch, SubFormTarget,
};
use crate::collection_options::collection_choice_fields;
use crate::obs_catalog_options::obs_catalog_fields;
use crate::vtube_catalog_options::vtube_catalog_fields;
use forge_registry::{FormField, FormSchemaSource};
use forge_types::{SubActionConfig, SubActionStep};
use gpui::{Context, Entity};

impl ScreenActionsView {
    pub(super) fn kind_label(&self, kind_id: &str) -> String {
        self.sub_action_registry
            .get(kind_id)
            .map(|r| r.label().to_owned())
            .unwrap_or_else(|| kind_id.to_owned())
    }

    pub(super) fn open_sub_form(&mut self, launch: SubFormLaunch, cx: &mut Context<Self>) {
        self.sub_form_choice_fields = collection_choice_fields(&launch.specs);
        self.sub_form_obs_fields = obs_catalog_fields(&launch.specs);
        self.sub_form_vtube_fields = vtube_catalog_fields(&launch.specs);
        let scope = self.step_scope(launch.target);
        let form = cx.new(|cx| {
            EditSubActionForm::new(launch, self.rt_handle.clone(), cx).with_variable_scope(scope)
        });
        self._sub_form_sub = Some(cx.subscribe(&form, Self::on_sub_form_event));
        self.sub_form = Some(form);
        self.fetch_select_options(cx);
        self.start_collection_options(cx);
        cx.notify();
    }

    fn close_sub_form(&mut self) {
        self.sub_form = None;
        self._sub_form_sub = None;
        self.sub_form_choice_fields.clear();
        self.sub_form_obs_fields.clear();
        self.sub_form_vtube_fields.clear();
    }

    fn on_sub_form_event(
        &mut self,
        form: Entity<EditSubActionForm>,
        event: &SubFormEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SubFormEvent::Commit(commit) => {
                let commit = commit.clone();
                let field_keys = form.read(cx).field_keys();
                if let Some(rejection) = self.sub_commit_rejection(&commit, &field_keys) {
                    form.update(cx, |form, cx| form.show_rejection(rejection, cx));
                    return;
                }
                self.close_sub_form();
                self.step_menu_open = None;
                self.apply_sub_form_commit(commit, cx);
                cx.notify();
            }
            SubFormEvent::Cancel => {
                self.close_sub_form();
                cx.notify();
            }
        }
    }

    fn committed_step_config(&self, commit: &SubFormCommit) -> SubActionConfig {
        let mut config = match commit.target {
            SubFormTarget::Edit(index) => self
                .current_chain()
                .get(index)
                .map(|step| step.config.clone())
                .unwrap_or_default(),
            SubFormTarget::Add => self
                .sub_action_registry
                .get(&commit.kind_id)
                .map(|r| r.default_config())
                .unwrap_or_default(),
        };
        config.extend(commit.overrides.iter().cloned());
        for key in &commit.cleared {
            config.remove(key);
        }
        config
    }

    fn sub_commit_rejection(
        &self,
        commit: &SubFormCommit,
        field_keys: &[String],
    ) -> Option<StepRejection> {
        let runner = self.sub_action_registry.get(&commit.kind_id)?;
        step_rejection(runner, &self.committed_step_config(commit), field_keys)
    }

    fn apply_sub_form_commit(&mut self, commit: SubFormCommit, cx: &mut Context<Self>) {
        match commit.target {
            SubFormTarget::Edit(index) => {
                let SubFormCommit {
                    overrides,
                    cleared,
                    continue_on_error,
                    condition,
                    label,
                    ..
                } = commit;
                self.persist_chain_mutation(
                    move |chain| {
                        if let Some(step) = chain.get_mut(index) {
                            for (key, value) in overrides {
                                step.config.insert(key, value);
                            }
                            for key in &cleared {
                                step.config.remove(key);
                            }
                            step.continue_on_error = continue_on_error;
                            step.condition = condition;
                            step.label = label;
                        }
                    },
                    cx,
                );
            }
            SubFormTarget::Add => {
                let config = self.committed_step_config(&commit);
                let SubFormCommit {
                    kind_id,
                    continue_on_error,
                    condition,
                    label,
                    ..
                } = commit;
                self.persist_chain_mutation(
                    move |chain| {
                        chain.push(SubActionStep {
                            kind_id,
                            config,
                            enabled: true,
                            continue_on_error,
                            condition,
                            label,
                        });
                    },
                    cx,
                );
            }
        }
    }

    pub(super) fn open_edit_sub_action(&mut self, i: usize, cx: &mut Context<Self>) {
        let chain = self.current_chain();
        let Some(step) = chain.get(i) else {
            return;
        };
        let kind_id = step.kind_id.clone();
        let mut config = step.config.clone();
        let continue_on_error = step.continue_on_error;
        let Some((specs, defaults, icon_name, category, refinement)) =
            self.sub_action_registry.get(&kind_id).map(|r| {
                (
                    r.config_fields(),
                    r.default_config(),
                    r.icon_name().to_owned(),
                    r.category(),
                    r.config_refinement(),
                )
            })
        else {
            return;
        };
        seed_absent_choices(&specs, &defaults, &mut config);
        let kind_label = self.kind_label(&kind_id);
        let name_value = step.label.clone().unwrap_or_else(|| kind_label.clone());
        let condition_value = step.condition.clone().unwrap_or_default();
        let chain_len = chain.len();
        let launch = SubFormLaunch {
            kind_id,
            target: SubFormTarget::Edit(i),
            specs,
            config,
            name_value,
            condition_value,
            continue_on_error,
            kind_label,
            icon_name,
            category: Some(category),
            chain_len,
            options_seed: self.select_options.clone(),
            refinement,
            schema: Arc::clone(&self.overlay_schema) as Arc<dyn FormSchemaSource>,
        };
        self.step_menu_open = None;
        self.open_sub_form(launch, cx);
    }
}

fn seed_absent_choices(
    specs: &[FormField],
    defaults: &SubActionConfig,
    config: &mut SubActionConfig,
) {
    for spec in specs {
        if let FormField::Select { key, .. } | FormField::Swatch { key, .. } = spec
            && !config.contains_key(*key)
            && let Some(default) = defaults.get(*key)
        {
            config.insert((*key).to_owned(), default.clone());
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_storage::MockMediaRepo;
    use forge_types::{ClipId, OutputDevice, Variant};
    use time::OffsetDateTime;

    use super::super::tests::view_with;
    use super::*;
    use crate::screen_shortcuts::ScreenShortcuts;
    use crate::test_support::{pump, runtime};

    const PLAY_SOUND: &str = "soundboard.sound.play";
    const CLIP_FIELD: &str = "clip_id";

    struct SilentPlayer;

    #[async_trait::async_trait]
    impl forge_runtime::SoundPlayer for SilentPlayer {
        async fn play(
            &self,
            _: ClipId,
            _: Option<OutputDevice>,
        ) -> Result<(), forge_runtime::SoundPlayerError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct SavedActions(std::sync::Mutex<Vec<Action>>);

    impl SavedActions {
        fn saved(&self) -> Vec<Action> {
            self.0.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl ActionRepo for SavedActions {
        async fn list(&self) -> Result<Vec<Action>, forge_storage::StorageError> {
            Ok(Vec::new())
        }

        async fn get(&self, _: ActionId) -> Result<Option<Action>, forge_storage::StorageError> {
            Ok(None)
        }

        async fn save(&self, action: &Action) -> Result<(), forge_storage::StorageError> {
            self.0.lock().unwrap().push(action.clone());
            Ok(())
        }

        async fn delete(&self, _: ActionId) -> Result<bool, forge_storage::StorageError> {
            Ok(false)
        }

        async fn telemetry(
            &self,
            _: ActionId,
        ) -> Result<forge_storage::ActionTelemetry, forge_storage::StorageError> {
            Ok(forge_storage::ActionTelemetry::default())
        }

        async fn record_execution(
            &self,
            _: ActionId,
            _: OffsetDateTime,
            _: u64,
            _: forge_storage::ExecutionStatus,
        ) -> Result<(), forge_storage::StorageError> {
            Ok(())
        }

        async fn prune_executions_before(
            &self,
            _: OffsetDateTime,
        ) -> Result<u64, forge_storage::StorageError> {
            Ok(0)
        }
    }

    struct PlaySoundEditor {
        view: Entity<ScreenActionsView>,
        saves: Arc<SavedActions>,
        rt: tokio::runtime::Runtime,
    }

    impl PlaySoundEditor {
        fn open(cx: &mut gpui::TestAppContext) -> Self {
            Self::open_storing(cx, SubActionConfig::new())
        }

        fn open_storing(cx: &mut gpui::TestAppContext, stored: SubActionConfig) -> Self {
            crate::i18n::install_language(forge_storage::Language::En);
            let rt = runtime();
            let mut sub_actions = SubActionRegistry::new();
            sub_actions
                .register(Box::new(
                    forge_runtime::audio_runners::PlaySoundRunner::new(Arc::new(SilentPlayer)),
                ))
                .unwrap();
            let saves = Arc::new(SavedActions::default());
            let mut media = MockMediaRepo::new();
            media.expect_blob_of().returning(|_| Ok(None));
            let view = view_with(
                cx,
                &rt,
                Vec::new(),
                media,
                sub_actions,
                Arc::clone(&saves) as Arc<dyn ActionRepo>,
            );
            view.update(cx, |view, cx| {
                let mut step = crate::test_support::step(PLAY_SOUND, true);
                step.config = stored;
                view.detail = Some(crate::actions_screen::integration_gating::tests::detail(
                    vec![step],
                ));
                view.open_edit_sub_action(0, cx);
            });
            Self { view, saves, rt }
        }

        fn form(&self, cx: &mut gpui::TestAppContext) -> Option<Entity<EditSubActionForm>> {
            self.view.read_with(cx, |view, _| view.sub_form.clone())
        }

        fn save(&self, cx: &mut gpui::TestAppContext, target: SubFormTarget, clip: &str) {
            self.commit(
                cx,
                SubFormCommit {
                    target,
                    kind_id: PLAY_SOUND.to_owned(),
                    overrides: vec![(CLIP_FIELD.to_owned(), Variant::String(clip.to_owned()))],
                    cleared: Vec::new(),
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
            );
        }

        fn commit(&self, cx: &mut gpui::TestAppContext, commit: SubFormCommit) {
            let form = self.form(cx).unwrap();
            form.update(cx, |_, cx| cx.emit(SubFormEvent::Commit(commit)));
            for _ in 0..3 {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }
    }

    #[gpui::test]
    fn saving_a_play_sound_step_without_a_clip_keeps_the_dialog_open_and_saves_nothing(
        cx: &mut gpui::TestAppContext,
    ) {
        for (target, case) in [
            (SubFormTarget::Edit(0), "edit"),
            (SubFormTarget::Add, "add"),
        ] {
            let editor = PlaySoundEditor::open(cx);

            editor.save(cx, target, "");

            assert_eq!(
                (editor.form(cx).is_some(), editor.saves.saved().len()),
                (true, 0),
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn a_clipless_play_sound_save_is_attributed_to_the_clip_field(cx: &mut gpui::TestAppContext) {
        let editor = PlaySoundEditor::open(cx);
        let form = editor.form(cx).unwrap();
        let commit = SubFormCommit {
            target: SubFormTarget::Edit(0),
            kind_id: PLAY_SOUND.to_owned(),
            overrides: vec![(CLIP_FIELD.to_owned(), Variant::String(String::new()))],
            cleared: Vec::new(),
            continue_on_error: false,
            condition: None,
            label: None,
        };

        let rejection = editor.view.read_with(cx, |view, cx| {
            view.sub_commit_rejection(&commit, &form.read(cx).field_keys())
        });

        assert_eq!(
            rejection,
            Some(StepRejection {
                field_key: Some(CLIP_FIELD.to_owned()),
                message: "Required".to_owned(),
            })
        );
    }

    #[gpui::test]
    fn saving_a_play_sound_step_after_picking_a_clip_closes_the_dialog_and_stores_the_clip(
        cx: &mut gpui::TestAppContext,
    ) {
        let editor = PlaySoundEditor::open(cx);
        let clip = ClipId::new().to_string();

        editor.save(cx, SubFormTarget::Edit(0), &clip);

        let stored = editor
            .saves
            .saved()
            .first()
            .and_then(|action| action.sub_actions.first().cloned())
            .and_then(|step| step.config.get(CLIP_FIELD).cloned());
        assert_eq!(
            (editor.form(cx).is_none(), stored),
            (true, Some(Variant::String(clip)))
        );
    }

    #[gpui::test]
    fn a_cleared_key_is_absent_from_the_saved_step_whether_stored_or_overridden(
        cx: &mut gpui::TestAppContext,
    ) {
        const COOLDOWN: &str = "cooldown_amount";
        const COOLDOWN_UNIT: &str = "cooldown_unit";
        for (target, case) in [
            (SubFormTarget::Edit(0), "edit"),
            (SubFormTarget::Add, "add"),
        ] {
            let stored: SubActionConfig = [
                (COOLDOWN.to_owned(), Variant::Int(300)),
                (
                    COOLDOWN_UNIT.to_owned(),
                    Variant::String("minutes".to_owned()),
                ),
            ]
            .into_iter()
            .collect();
            let editor = PlaySoundEditor::open_storing(cx, stored);
            let clip = Variant::String(ClipId::new().to_string());

            editor.commit(
                cx,
                SubFormCommit {
                    target,
                    kind_id: PLAY_SOUND.to_owned(),
                    overrides: vec![
                        (CLIP_FIELD.to_owned(), clip.clone()),
                        (COOLDOWN.to_owned(), Variant::Int(5)),
                    ],
                    cleared: vec![COOLDOWN.to_owned(), COOLDOWN_UNIT.to_owned()],
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
            );

            let saved = editor
                .saves
                .saved()
                .last()
                .and_then(|action| action.sub_actions.last().cloned())
                .map(|step| step.config);
            assert_eq!(
                saved,
                Some([(CLIP_FIELD.to_owned(), clip)].into_iter().collect()),
                "{case}"
            );
        }
    }

    fn settle(editor: &PlaySoundEditor, cx: &mut gpui::TestAppContext) {
        for _ in 0..3 {
            pump(&editor.rt);
            cx.run_until_parked();
        }
    }

    #[gpui::test]
    fn the_save_shortcut_submits_the_step_dialog_through_its_validation(
        cx: &mut gpui::TestAppContext,
    ) {
        let clip = Variant::String(ClipId::new().to_string());
        for (stored, closes, saves, case) in [
            (SubActionConfig::new(), false, 0, "no clip picked"),
            (
                [(CLIP_FIELD.to_owned(), clip.clone())]
                    .into_iter()
                    .collect(),
                true,
                1,
                "clip picked",
            ),
        ] {
            let editor = PlaySoundEditor::open_storing(cx, stored);

            editor.view.update(cx, |view, cx| view.save_shortcut(cx));
            settle(&editor, cx);

            assert_eq!(
                (editor.form(cx).is_none(), editor.saves.saved().len()),
                (closes, saves),
                "{case}"
            );
        }
    }

    const SENDER_OPTIONS: &[&str] = &["default", "streamer", "bot"];
    const COLOR_OPTIONS: &[&str] = &["primary", "blue"];

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn config(entries: &[(&str, &str)]) -> SubActionConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), text(value)))
            .collect()
    }

    fn choice_specs() -> Vec<FormField> {
        vec![
            FormField::Select {
                key: "sender",
                label: "Send As",
                options: SENDER_OPTIONS,
            },
            FormField::Swatch {
                key: "color",
                label: "Color",
                options: COLOR_OPTIONS,
            },
        ]
    }

    #[test]
    fn absent_select_and_swatch_values_are_filled_from_the_runner_defaults() {
        let defaults = config(&[("sender", "default"), ("color", "primary")]);
        let mut stored = config(&[("message", "hello")]);

        seed_absent_choices(&choice_specs(), &defaults, &mut stored);

        assert_eq!(
            stored,
            config(&[
                ("message", "hello"),
                ("sender", "default"),
                ("color", "primary")
            ])
        );
    }

    #[test]
    fn stored_choices_are_never_replaced_by_the_defaults() {
        let defaults = config(&[("sender", "default"), ("color", "primary")]);
        let mut stored = config(&[("sender", "bot"), ("color", "blue")]);

        seed_absent_choices(&choice_specs(), &defaults, &mut stored);

        assert_eq!(stored, config(&[("sender", "bot"), ("color", "blue")]));
    }

    #[test]
    fn a_choice_without_a_declared_default_stays_absent() {
        let mut stored = SubActionConfig::new();

        seed_absent_choices(&choice_specs(), &SubActionConfig::new(), &mut stored);

        assert!(stored.is_empty());
    }

    #[test]
    fn only_static_choice_fields_are_seeded() {
        let specs = vec![
            FormField::Text {
                key: "message",
                label: "Message",
                placeholder: "",
            },
            FormField::DynamicSelect {
                key: "reward",
                label: "Reward",
                options_key: "rewards",
            },
            FormField::DependentSelect {
                key: "source",
                label: "Source",
                options_prefix: "sources",
                depends_on: "scene",
            },
            FormField::Optional {
                key: "voice",
                label: "Voice",
                inner: Box::new(FormField::Select {
                    key: "voice",
                    label: "Voice",
                    options: SENDER_OPTIONS,
                }),
            },
        ];
        let defaults = config(&[
            ("message", "hi"),
            ("reward", "r1"),
            ("source", "cam"),
            ("voice", "default"),
        ]);
        let mut stored = SubActionConfig::new();

        seed_absent_choices(&specs, &defaults, &mut stored);

        assert!(stored.is_empty(), "{stored:?}");
    }
}
