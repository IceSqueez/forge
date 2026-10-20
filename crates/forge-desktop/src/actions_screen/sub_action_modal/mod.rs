use super::step_validation::StepRejection;
use super::*;
use crate::config_form::dependent_options;
use build::{build_form_fields, build_step_meta_inputs, push_form_field};
use datetime_picker::DateTimePickerForm;
use field::{SubFormField, field_keys, field_values};
use forge_components::drive_overlay_focus;
use forge_registry::{
    FormField, FormRefinement, FormSchemaSource, SubActionCategory, refined_fields,
};
use forge_types::{SubActionConfig, Variant};
use gpui::FocusHandle;
use select_picker::{SelectPickerForm, select_picker_items};
use variable_insert::VariableInsert;

mod build;
mod datetime_picker;
mod field;
mod field_controls;
mod field_grid;
mod modal;
mod rejection;
mod select_picker;
mod submit;
mod unit_amount;
mod variable_insert;

#[derive(Clone, Copy)]
pub(super) enum SubFormTarget {
    Edit(usize),
    Add,
}

pub(super) struct SubFormLaunch {
    pub kind_id: String,
    pub target: SubFormTarget,
    pub specs: Vec<FormField>,
    pub config: SubActionConfig,
    pub name_value: String,
    pub condition_value: String,
    pub continue_on_error: bool,
    pub kind_label: String,
    pub icon_name: String,
    pub category: Option<SubActionCategory>,
    pub chain_len: usize,
    pub options_seed: HashMap<String, Vec<(String, String)>>,
    pub refinement: Option<FormRefinement>,
    pub schema: Arc<dyn FormSchemaSource>,
}

#[derive(Clone)]
pub(super) struct SubFormCommit {
    pub target: SubFormTarget,
    pub kind_id: String,
    pub overrides: Vec<(String, Variant)>,
    pub cleared: Vec<String>,
    pub continue_on_error: bool,
    pub condition: Option<String>,
    pub label: Option<String>,
}

pub(super) enum SubFormEvent {
    Commit(SubFormCommit),
    Cancel,
}

pub(super) struct EditSubActionForm {
    kind_id: String,
    kind_label: String,
    icon_name: String,
    category: Option<SubActionCategory>,
    target: SubFormTarget,
    chain_len: usize,
    fields: Vec<SubFormField>,
    base_field_count: usize,
    launch_config: SubActionConfig,
    options: HashMap<String, Vec<(String, String)>>,
    refinement: Option<FormRefinement>,
    schema: Arc<dyn FormSchemaSource>,
    name_input: Entity<TextInput>,
    condition_input: Entity<TextInput>,
    continue_on_error: bool,
    select_picker: Option<SelectPickerForm>,
    variables: VariableInsert,
    datetime_picker: Option<DateTimePickerForm>,
    datetime_focus: FocusHandle,
    datetime_focus_restore: Option<FocusHandle>,
    rejection: Option<StepRejection>,
    rt_handle: tokio::runtime::Handle,
}

impl EventEmitter<SubFormEvent> for EditSubActionForm {}

impl EditSubActionForm {
    pub(super) fn new(
        launch: SubFormLaunch,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let SubFormLaunch {
            kind_id,
            target,
            specs,
            config,
            name_value,
            condition_value,
            continue_on_error,
            kind_label,
            icon_name,
            category,
            chain_len,
            options_seed,
            refinement,
            schema,
        } = launch;

        let palette = cx.palette();
        let fields = build_form_fields(&specs, &config, palette, &options_seed, cx);
        let base_field_count = fields.len();
        let (name_input, condition_input) =
            build_step_meta_inputs(&kind_label, &name_value, &condition_value, cx);

        let mut form = Self {
            kind_id,
            kind_label,
            icon_name,
            category,
            target,
            chain_len,
            fields,
            base_field_count,
            launch_config: config,
            options: options_seed,
            refinement,
            schema,
            name_input,
            condition_input,
            continue_on_error,
            select_picker: None,
            variables: VariableInsert::default(),
            datetime_picker: None,
            datetime_focus: cx.focus_handle(),
            datetime_focus_restore: None,
            rejection: None,
            rt_handle,
        };
        form.rebuild_refined(cx);
        form
    }

    pub(super) fn set_schema(&mut self, schema: Arc<dyn FormSchemaSource>, cx: &mut Context<Self>) {
        self.schema = schema;
        self.rebuild_refined(cx);
        cx.notify();
    }

    fn rebuild_refined(&mut self, cx: &mut Context<Self>) {
        let Some(refinement) = self.refinement else {
            return;
        };
        let seed = self.current_values(cx);
        let specs = refined_fields(refinement, &seed, self.schema.as_ref());
        self.fields.truncate(self.base_field_count);
        let palette = cx.palette();
        let options = self.options.clone();
        let mut refined = Vec::new();
        for spec in &specs {
            push_form_field(spec, None, &seed, palette, &options, &mut refined, cx);
        }
        self.fields.append(&mut refined);
        if let Some(key) = self.select_picker.as_ref().map(|form| form.key.clone())
            && !self
                .fields
                .iter()
                .any(|field| field.select_entries(&key).is_some())
        {
            self.select_picker = None;
        }
    }

    fn resolve_dependent_selects(&mut self, cx: &App) {
        let values = self.current_values(cx);
        let Self {
            fields, options, ..
        } = self;
        for field in fields.iter_mut() {
            if let SubFormField::Select {
                options: opts,
                dependency: Some(dependency),
                ..
            } = field
            {
                let sibling = values
                    .get(&dependency.depends_on)
                    .map(forge_types::display_scalar)
                    .unwrap_or_default();
                *opts = dependent_options(options, &dependency.options_prefix, &sibling);
            }
        }
    }

    fn current_values(&self, cx: &App) -> SubActionConfig {
        let mut values = self.launch_config.clone();
        for field in &self.fields {
            values.extend(field_values(field, cx));
        }
        values
    }

    pub(super) fn apply_options(
        &mut self,
        map: &HashMap<String, Vec<(String, String)>>,
        cx: &mut Context<Self>,
    ) {
        self.options = map.clone();
        for field in &mut self.fields {
            if let SubFormField::Select {
                options_key: Some(ok),
                options,
                ..
            } = field
                && let Some(opts) = map.get(ok)
            {
                *options = opts.clone();
            }
        }
        self.resolve_dependent_selects(cx);
        if let Some(picker_form) = self.select_picker.as_ref() {
            let key = picker_form.key.clone();
            if let Some(SubFormField::Select { options, .. }) = self
                .fields
                .iter()
                .find(|field| matches!(field, SubFormField::Select { key: k, .. } if *k == key))
            {
                let items = select_picker_items(options);
                picker_form
                    .picker
                    .update(cx, |picker, cx| picker.set_items(items, cx));
            }
        }
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(SubFormEvent::Cancel);
    }

    pub(super) fn field_keys(&self) -> Vec<String> {
        self.fields
            .iter()
            .flat_map(field_keys)
            .map(str::to_owned)
            .collect()
    }
}

impl Render for EditSubActionForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        drive_overlay_focus(
            self.datetime_picker.is_some(),
            &self.datetime_focus,
            &mut self.datetime_focus_restore,
            window,
            cx,
        );

        let modal = self.render_modal(&palette, cx);
        let datetime_popover = self
            .datetime_picker
            .as_ref()
            .map(|form| self.render_datetime_popover(form, cx));

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(modal)
            .children(datetime_popover)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_components::{ThemeId, bind_picker_keys, bind_text_input_keys};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::actions_screen::overlay_schema::OverlayContentSchema;
    use crate::amount_scale::AmountScale;
    use crate::test_support::runtime;
    use forge_components::Presentation;
    use forge_registry::{CodeLanguage, UnitAmountBounds};

    const REWARD_FIELD: &str = "reward_id";
    const OWNED_REWARDS: &str = "collections.twitch.rewards.manageable";
    const MODE_FIELD: &str = "mode";

    fn specs() -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: REWARD_FIELD,
                label: "Reward",
                options_key: OWNED_REWARDS,
            },
            FormField::Select {
                key: MODE_FIELD,
                label: "Mode",
                options: &["on", "off"],
            },
        ]
    }

    fn launch(stored_reward: Option<&str>) -> SubFormLaunch {
        let config: SubActionConfig = stored_reward
            .map(|id| (REWARD_FIELD.to_owned(), Variant::String(id.to_owned())))
            .into_iter()
            .collect();
        launch_with(specs(), config)
    }

    pub(super) fn launch_with(specs: Vec<FormField>, config: SubActionConfig) -> SubFormLaunch {
        SubFormLaunch {
            kind_id: "twitch.channel_points.enable_reward".to_owned(),
            target: SubFormTarget::Add,
            specs,
            config,
            name_value: String::new(),
            condition_value: String::new(),
            continue_on_error: false,
            kind_label: "Enable reward".to_owned(),
            icon_name: "gift".to_owned(),
            category: None,
            chain_len: 0,
            options_seed: HashMap::new(),
            refinement: None,
            schema: Arc::new(OverlayContentSchema::new(Arc::new(
                OverlayKindRegistry::new(),
            ))) as Arc<dyn FormSchemaSource>,
        }
    }

    fn open<'a>(
        cx: &'a mut TestAppContext,
        stored_reward: Option<&str>,
    ) -> (
        Entity<EditSubActionForm>,
        &'a mut VisualTestContext,
        tokio::runtime::Runtime,
    ) {
        open_launch(cx, launch(stored_reward))
    }

    fn open_launch(
        cx: &mut TestAppContext,
        launch: SubFormLaunch,
    ) -> (
        Entity<EditSubActionForm>,
        &mut VisualTestContext,
        tokio::runtime::Runtime,
    ) {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            bind_picker_keys(cx);
            bind_text_input_keys(cx);
        });
        let rt = runtime();
        let handle = rt.handle().clone();
        let (form, vcx) =
            cx.add_window_view(move |_window, cx| EditSubActionForm::new(launch, handle, cx));
        vcx.run_until_parked();
        (form, vcx, rt)
    }

    fn select(
        form: &Entity<EditSubActionForm>,
        vcx: &mut VisualTestContext,
        key: &str,
    ) -> (String, Vec<String>) {
        vcx.update(|_window, cx| {
            form.read(cx)
                .fields
                .iter()
                .find_map(|field| match field {
                    SubFormField::Select {
                        key: k,
                        selected,
                        options,
                        ..
                    } if k == key => Some((
                        selected.clone(),
                        options.iter().map(|(value, _)| value.clone()).collect(),
                    )),
                    _ => None,
                })
                .unwrap()
        })
    }

    fn deliver_rewards(form: &Entity<EditSubActionForm>, vcx: &mut VisualTestContext) {
        let map = HashMap::from([(
            OWNED_REWARDS.to_owned(),
            vec![("r-1".to_owned(), "Hydrate".to_owned())],
        )]);
        vcx.update(|_window, cx| form.update(cx, |form, cx| form.apply_options(&map, cx)));
    }

    fn type_into_picker(
        form: &Entity<EditSubActionForm>,
        vcx: &mut VisualTestContext,
        key: &str,
        typed: &str,
    ) {
        vcx.update(|window, cx| {
            form.update(cx, |form, cx| {
                form.open_select_picker(key.to_owned(), window, cx)
            })
        });
        vcx.run_until_parked();
        vcx.simulate_input(typed);
        vcx.simulate_keystrokes("enter");
        vcx.run_until_parked();
    }

    #[gpui::test]
    fn arriving_rewards_fill_the_reward_select(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open(cx, None);

        deliver_rewards(&form, vcx);

        assert_eq!(select(&form, vcx, REWARD_FIELD).1, ["r-1"]);
    }

    #[gpui::test]
    fn a_stored_raw_reward_id_stays_selected_after_the_rewards_arrive(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open(cx, Some("%reward.id%"));

        deliver_rewards(&form, vcx);

        assert_eq!(select(&form, vcx, REWARD_FIELD).0, "%reward.id%");
    }

    #[gpui::test]
    fn the_reward_select_accepts_a_typed_variable(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open(cx, None);
        deliver_rewards(&form, vcx);

        type_into_picker(&form, vcx, REWARD_FIELD, "%reward.id%");

        assert_eq!(select(&form, vcx, REWARD_FIELD).0, "%reward.id%");
    }

    #[gpui::test]
    fn a_fixed_option_select_does_not_accept_typed_text(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open(cx, None);

        type_into_picker(&form, vcx, MODE_FIELD, "sideways");

        assert_ne!(select(&form, vcx, MODE_FIELD).0, "sideways");
    }

    const CODE_FIELD: &str = "body";
    const GATE_FIELD: &str = "use_body";
    const RHAI_BODY: &str = "let n = 1;\r\nif n > 0 {\n\tprint(\"привіт 🚀\");\n}\n";
    const JSON_BODY: &str = "{\n  \"items\": [1, 2.5, null],\n  \"name\": \"ü\"\n}";

    fn code_spec(language: CodeLanguage) -> FormField {
        FormField::Code {
            key: CODE_FIELD,
            label: "Body",
            language,
        }
    }

    fn committed_overrides(
        specs: Vec<FormField>,
        config: SubActionConfig,
        cx: &mut TestAppContext,
    ) -> Vec<(String, Variant)> {
        submitted_overrides(specs, config, cx).unwrap()
    }

    fn submitted_overrides(
        specs: Vec<FormField>,
        config: SubActionConfig,
        cx: &mut TestAppContext,
    ) -> Option<Vec<(String, Variant)>> {
        let (form, vcx, _rt) = open_launch(cx, launch_with(specs, config));
        let heard = std::rc::Rc::new(std::cell::RefCell::new(None));
        let sink = std::rc::Rc::clone(&heard);
        let _subscription = vcx.update(|_window, cx| {
            cx.subscribe(&form, move |_, event: &SubFormEvent, _| {
                if let SubFormEvent::Commit(commit) = event {
                    *sink.borrow_mut() = Some(commit.overrides.clone());
                }
            })
        });
        vcx.update(|_window, cx| form.update(cx, |form, cx| form.submit(cx)));
        heard.borrow_mut().take()
    }

    fn override_for<'a>(overrides: &'a [(String, Variant)], key: &str) -> Option<&'a Variant> {
        overrides.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    #[gpui::test]
    fn code_field_commits_the_stored_body_verbatim(cx: &mut TestAppContext) {
        for (language, stored, expected) in [
            (CodeLanguage::Rhai, Some(RHAI_BODY), RHAI_BODY),
            (CodeLanguage::Json, Some(JSON_BODY), JSON_BODY),
            (CodeLanguage::Rhai, None, ""),
            (CodeLanguage::Json, Some(""), ""),
        ] {
            let config: SubActionConfig = stored
                .map(|body| (CODE_FIELD.to_owned(), Variant::String(body.to_owned())))
                .into_iter()
                .collect();
            let overrides = committed_overrides(vec![code_spec(language)], config, cx);
            assert_eq!(
                override_for(&overrides, CODE_FIELD),
                Some(&Variant::String(expected.to_owned())),
                "{language:?} stored {stored:?}"
            );
        }
    }

    #[gpui::test]
    fn gated_code_field_commits_only_while_its_toggle_is_on(cx: &mut TestAppContext) {
        for (toggle, expected) in [
            (true, Some(Variant::String(RHAI_BODY.to_owned()))),
            (false, None),
        ] {
            let spec = FormField::Optional {
                key: GATE_FIELD,
                label: "Use body",
                inner: Box::new(code_spec(CodeLanguage::Rhai)),
            };
            let config: SubActionConfig = [
                (GATE_FIELD.to_owned(), Variant::Bool(toggle)),
                (CODE_FIELD.to_owned(), Variant::String(RHAI_BODY.to_owned())),
            ]
            .into_iter()
            .collect();
            let overrides = committed_overrides(vec![spec], config, cx);
            assert_eq!(
                override_for(&overrides, CODE_FIELD),
                expected.as_ref(),
                "toggle {toggle}"
            );
        }
    }

    const COUNT_FIELD: &str = "count";
    const COUNT_GATE: &str = "use_count";

    fn count_spec(gated: bool) -> FormField {
        let count = FormField::Integer {
            key: COUNT_FIELD,
            label: "Count",
            min: 0,
            max: 127,
        };
        if gated {
            FormField::Optional {
                key: COUNT_GATE,
                label: "Use count",
                inner: Box::new(count),
            }
        } else {
            count
        }
    }

    #[gpui::test]
    fn an_out_of_range_integer_blocks_submit_only_while_its_field_is_in_use(
        cx: &mut TestAppContext,
    ) {
        for (toggle, stored, commits) in [
            (None, Variant::Int(127), true),
            (None, Variant::Int(0), true),
            (None, Variant::Int(128), false),
            (None, Variant::Int(-1), false),
            (None, Variant::String("abc".to_owned()), false),
            (None, Variant::String(String::new()), true),
            (Some(true), Variant::Int(200), false),
            (Some(false), Variant::Int(200), true),
        ] {
            let mut config: SubActionConfig = [(COUNT_FIELD.to_owned(), stored.clone())]
                .into_iter()
                .collect();
            if let Some(on) = toggle {
                config.insert(COUNT_GATE.to_owned(), Variant::Bool(on));
            }
            let outcome = submitted_overrides(vec![count_spec(toggle.is_some())], config, cx);
            assert_eq!(
                outcome.is_some(),
                commits,
                "toggle {toggle:?}, stored {stored:?}"
            );
        }
    }

    const AMOUNT_FIELD: &str = "delay_amount";
    const UNIT_FIELD: &str = "delay_unit";
    const AMOUNT_GATE: &str = "use_delay";
    const AMOUNT_BOUNDS: UnitAmountBounds = UnitAmountBounds {
        min: 1,
        max_base_units: 120,
        units: &[
            forge_registry::AmountUnit {
                value: "seconds",
                base_units: 1,
            },
            forge_registry::AmountUnit {
                value: "minutes",
                base_units: 60,
            },
        ],
    };

    fn unit_amount_spec(gated: bool) -> FormField {
        let amount = FormField::UnitAmount {
            key: AMOUNT_FIELD,
            label: "Run after",
            unit_key: UNIT_FIELD,
            bounds: AMOUNT_BOUNDS,
        };
        if gated {
            FormField::Optional {
                key: AMOUNT_GATE,
                label: "Delay",
                inner: Box::new(amount),
            }
        } else {
            amount
        }
    }

    fn stored_amount(amount: Variant, unit: Option<&str>) -> SubActionConfig {
        let mut config: SubActionConfig = [(AMOUNT_FIELD.to_owned(), amount)].into_iter().collect();
        if let Some(unit) = unit {
            config.insert(UNIT_FIELD.to_owned(), Variant::String(unit.to_owned()));
        }
        config
    }

    #[gpui::test]
    fn an_amount_past_its_units_bound_blocks_submit_only_while_the_field_is_in_use(
        cx: &mut TestAppContext,
    ) {
        for (toggle, amount, unit, commits) in [
            (None, Variant::Int(120), Some("seconds"), true),
            (None, Variant::Int(121), Some("seconds"), false),
            (None, Variant::Int(2), Some("minutes"), true),
            (None, Variant::Int(3), Some("minutes"), false),
            (None, Variant::Int(0), Some("minutes"), false),
            (None, Variant::Int(121), Some("weeks"), false),
            (
                None,
                Variant::String("soon".to_owned()),
                Some("seconds"),
                false,
            ),
            (None, Variant::String(String::new()), Some("seconds"), true),
            (Some(true), Variant::Int(3), Some("minutes"), false),
            (Some(false), Variant::Int(3), Some("minutes"), true),
        ] {
            let mut config = stored_amount(amount.clone(), unit);
            if let Some(on) = toggle {
                config.insert(AMOUNT_GATE.to_owned(), Variant::Bool(on));
            }
            let outcome = submitted_overrides(vec![unit_amount_spec(toggle.is_some())], config, cx);
            assert_eq!(
                outcome.is_some(),
                commits,
                "toggle {toggle:?}, amount {amount:?}, unit {unit:?}"
            );
        }
    }

    #[gpui::test]
    fn a_unit_amount_commits_its_amount_and_its_unit_under_their_own_keys(cx: &mut TestAppContext) {
        for (unit, committed_unit) in [
            (Some("minutes"), "minutes"),
            (Some("weeks"), "seconds"),
            (None, "seconds"),
        ] {
            let overrides = committed_overrides(
                vec![unit_amount_spec(false)],
                stored_amount(Variant::Int(2), unit),
                cx,
            );
            assert_eq!(
                (
                    override_for(&overrides, AMOUNT_FIELD),
                    override_for(&overrides, UNIT_FIELD)
                ),
                (
                    Some(&Variant::Int(2)),
                    Some(&Variant::String(committed_unit.to_owned()))
                ),
                "stored unit {unit:?}"
            );
        }
    }

    #[gpui::test]
    fn picking_a_smaller_unit_lets_an_amount_too_large_for_the_old_unit_commit(
        cx: &mut TestAppContext,
    ) {
        let launch = launch_with(
            vec![unit_amount_spec(false)],
            stored_amount(Variant::Int(100), Some("minutes")),
        );
        let (form, vcx, _rt) = open_launch(cx, launch);
        let heard = std::rc::Rc::new(std::cell::RefCell::new(None));
        let sink = std::rc::Rc::clone(&heard);
        let _subscription = vcx.update(|_window, cx| {
            cx.subscribe(&form, move |_, event: &SubFormEvent, _| {
                if let SubFormEvent::Commit(commit) = event {
                    *sink.borrow_mut() = Some(commit.overrides.clone());
                }
            })
        });

        vcx.update(|window, cx| {
            form.update(cx, |form, cx| {
                form.open_select_picker(UNIT_FIELD.to_owned(), window, cx);
                form.pick_select_option("seconds".to_owned(), cx);
            })
        });
        vcx.update(|_window, cx| form.update(cx, |form, cx| form.submit(cx)));

        let overrides = heard
            .borrow_mut()
            .take()
            .unwrap_or_else(|| panic!("100 seconds is in range, so Save must commit"));
        assert_eq!(
            (
                override_for(&overrides, AMOUNT_FIELD),
                override_for(&overrides, UNIT_FIELD)
            ),
            (
                Some(&Variant::Int(100)),
                Some(&Variant::String("seconds".to_owned()))
            )
        );
    }

    const DURATION_FIELD: &str = "timeout_ms";

    fn duration_spec() -> FormField {
        FormField::Duration {
            key: DURATION_FIELD,
            label: "Timeout",
            bounds: forge_registry::DurationBounds::in_millis(100, 600_000),
        }
    }

    fn duration_unit_picker() -> String {
        AmountScale::of(&duration_spec())
            .map(|scale| scale.unit_picker_key(DURATION_FIELD))
            .unwrap_or_else(|| panic!("a duration spec has an amount scale"))
    }

    fn stored_millis(millis: Variant) -> SubActionConfig {
        [(DURATION_FIELD.to_owned(), millis)].into_iter().collect()
    }

    #[gpui::test]
    fn a_duration_past_its_bounds_in_the_shown_unit_blocks_submit(cx: &mut TestAppContext) {
        for (stored, commits) in [
            (Variant::Int(100), true),
            (Variant::Int(99), false),
            (Variant::Int(600_000), true),
            (Variant::Int(660_000), false),
            (Variant::Int(601_000), false),
            (Variant::Int(0), false),
            (Variant::String("soon".to_owned()), false),
            (Variant::String(String::new()), true),
        ] {
            let outcome =
                submitted_overrides(vec![duration_spec()], stored_millis(stored.clone()), cx);
            assert_eq!(outcome.is_some(), commits, "stored {stored:?}");
        }
    }

    #[gpui::test]
    fn an_unedited_duration_commits_only_the_stored_amount_it_loaded(cx: &mut TestAppContext) {
        let in_seconds = forge_registry::DurationBounds::in_seconds(1, 1_209_600);
        let in_minutes = forge_registry::DurationBounds::in_minutes(1, 1_440);
        let in_millis = forge_registry::DurationBounds::in_millis(100, 600_000);
        for (bounds, stored) in [
            (in_millis, 100),
            (in_millis, 1_500),
            (in_millis, 30_000),
            (in_millis, 120_000),
            (in_seconds, 1),
            (in_seconds, 90),
            (in_seconds, 600),
            (in_seconds, 86_400),
            (in_seconds, 1_209_600),
            (in_minutes, 1),
            (in_minutes, 90),
            (in_minutes, 120),
            (in_minutes, 1_440),
        ] {
            let spec = FormField::Duration {
                key: DURATION_FIELD,
                label: "Timeout",
                bounds,
            };
            let overrides =
                committed_overrides(vec![spec], stored_millis(Variant::Int(stored)), cx);
            assert_eq!(
                overrides,
                [(DURATION_FIELD.to_owned(), Variant::Int(stored))],
                "stored {stored} in {bounds:?}"
            );
        }
    }

    const COOLDOWN_FIELD: &str = "global_cooldown_seconds";

    fn same_key_optional(inner: FormField) -> FormField {
        FormField::Optional {
            key: COOLDOWN_FIELD,
            label: "Global Cooldown Seconds",
            inner: Box::new(inner),
        }
    }

    fn cooldown_integer() -> FormField {
        FormField::Integer {
            key: COOLDOWN_FIELD,
            label: "Global Cooldown Seconds",
            min: 1,
            max: i64::MAX,
        }
    }

    fn cooldown_duration() -> FormField {
        FormField::Duration {
            key: COOLDOWN_FIELD,
            label: "Global Cooldown",
            bounds: forge_registry::DurationBounds::in_seconds(0, i64::MAX),
        }
    }

    fn cooldown_text() -> FormField {
        FormField::Text {
            key: COOLDOWN_FIELD,
            label: "Global Cooldown",
            placeholder: "",
        }
    }

    fn same_key_amount() -> FormField {
        FormField::Optional {
            key: AMOUNT_FIELD,
            label: "Delay",
            inner: Box::new(unit_amount_spec(false)),
        }
    }

    fn stored_cooldown(stored: Option<Variant>) -> SubActionConfig {
        stored
            .map(|value| (COOLDOWN_FIELD.to_owned(), value))
            .into_iter()
            .collect()
    }

    fn submitted_commit(
        specs: Vec<FormField>,
        config: SubActionConfig,
        flipped_toggle: Option<&str>,
        cx: &mut TestAppContext,
    ) -> Option<SubFormCommit> {
        let (form, vcx, _rt) = open_launch(cx, launch_with(specs, config));
        let heard = std::rc::Rc::new(std::cell::RefCell::new(None));
        let sink = std::rc::Rc::clone(&heard);
        let _subscription = vcx.update(|_window, cx| {
            cx.subscribe(&form, move |_, event: &SubFormEvent, _| {
                if let SubFormEvent::Commit(commit) = event {
                    *sink.borrow_mut() = Some(commit.clone());
                }
            })
        });
        if let Some(flipped) = flipped_toggle {
            vcx.update(|_window, cx| {
                form.update(cx, |form, _| {
                    for field in &mut form.fields {
                        if let SubFormField::Bool { key, value, .. } = field
                            && key == flipped
                        {
                            *value = !*value;
                        }
                    }
                })
            });
        }
        vcx.update(|_window, cx| form.update(cx, |form, cx| form.submit(cx)));
        heard.borrow_mut().take()
    }

    fn opened_state(
        spec: FormField,
        config: SubActionConfig,
        key: &str,
        cx: &mut TestAppContext,
    ) -> (bool, String) {
        let (form, vcx, _rt) = open_launch(cx, launch_with(vec![spec], config));
        vcx.update(|_window, cx| {
            let fields = &form.read(cx).fields;
            let toggle = fields
                .iter()
                .find_map(|field| match field {
                    SubFormField::Bool { key: k, value, .. } if k == key => Some(*value),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("an optional builds its toggle"));
            let shown = fields
                .iter()
                .find_map(|field| match field {
                    SubFormField::Input { key: k, input, .. }
                    | SubFormField::UnitAmount { key: k, input, .. }
                        if k == key =>
                    {
                        Some(input.read(cx).content().to_owned())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("an optional builds its inner field"));
            (toggle, shown)
        })
    }

    #[gpui::test]
    fn a_same_key_optional_holding_a_value_opens_on_showing_that_value(cx: &mut TestAppContext) {
        for (inner, stored, shown) in [
            (cooldown_integer(), Variant::Int(300), "300"),
            (cooldown_integer(), Variant::Int(1), "1"),
            (cooldown_duration(), Variant::Int(0), "0"),
            (
                cooldown_text(),
                Variant::String("Launchkey".to_owned()),
                "Launchkey",
            ),
            (cooldown_text(), Variant::String(String::new()), ""),
        ] {
            assert_eq!(
                opened_state(
                    same_key_optional(inner),
                    stored_cooldown(Some(stored.clone())),
                    COOLDOWN_FIELD,
                    cx
                ),
                (true, shown.to_owned()),
                "stored {stored:?}"
            );
        }
    }

    #[gpui::test]
    fn a_same_key_optional_holding_a_value_commits_it_back_unchanged(cx: &mut TestAppContext) {
        for (inner, stored) in [
            (cooldown_integer(), Variant::Int(300)),
            (cooldown_duration(), Variant::Int(0)),
            (cooldown_duration(), Variant::Int(3_600)),
            (cooldown_duration(), Variant::Int(90)),
            (cooldown_text(), Variant::String("Launchkey".to_owned())),
        ] {
            let commit = submitted_commit(
                vec![same_key_optional(inner)],
                stored_cooldown(Some(stored.clone())),
                None,
                cx,
            )
            .unwrap_or_else(|| panic!("an untouched stored value must not block Save"));
            assert_eq!(
                (commit.overrides, commit.cleared),
                (
                    vec![(COOLDOWN_FIELD.to_owned(), stored.clone())],
                    Vec::new()
                ),
                "stored {stored:?}"
            );
        }
    }

    #[gpui::test]
    fn a_same_key_optional_holding_a_toggle_or_nothing_opens_off_with_an_empty_field(
        cx: &mut TestAppContext,
    ) {
        for inner in [cooldown_integer(), cooldown_duration(), cooldown_text()] {
            for stored in [None, Some(Variant::Bool(true)), Some(Variant::Bool(false))] {
                assert_eq!(
                    opened_state(
                        same_key_optional(inner.clone()),
                        stored_cooldown(stored.clone()),
                        COOLDOWN_FIELD,
                        cx
                    ),
                    (false, String::new()),
                    "inner {:?}, stored {stored:?}",
                    inner.key()
                );
            }
        }
    }

    #[gpui::test]
    fn saving_a_same_key_optional_holding_a_toggle_clears_the_key_instead_of_blocking(
        cx: &mut TestAppContext,
    ) {
        for inner in [cooldown_integer(), cooldown_duration(), cooldown_text()] {
            for stored in [Variant::Bool(true), Variant::Bool(false)] {
                let commit = submitted_commit(
                    vec![same_key_optional(inner.clone())],
                    stored_cooldown(Some(stored.clone())),
                    None,
                    cx,
                )
                .unwrap_or_else(|| panic!("a stored toggle must not block Save"));
                assert_eq!(
                    (commit.overrides, commit.cleared),
                    (Vec::new(), vec![COOLDOWN_FIELD.to_owned()]),
                    "stored {stored:?}"
                );
            }
        }
    }

    #[gpui::test]
    fn switching_a_same_key_optional_off_clears_its_keys_and_commits_no_toggle(
        cx: &mut TestAppContext,
    ) {
        for (spec, stored, cleared) in [
            (
                same_key_optional(cooldown_integer()),
                stored_cooldown(Some(Variant::Int(300))),
                vec![COOLDOWN_FIELD],
            ),
            (
                same_key_optional(cooldown_duration()),
                stored_cooldown(Some(Variant::Int(3_600))),
                vec![COOLDOWN_FIELD],
            ),
            (
                same_key_amount(),
                stored_amount(Variant::Int(2), Some("minutes")),
                vec![AMOUNT_FIELD, UNIT_FIELD],
            ),
        ] {
            let gate = spec.key().to_owned();
            let commit = submitted_commit(vec![spec], stored, Some(&gate), cx)
                .unwrap_or_else(|| panic!("switching an optional off must not block Save"));
            assert_eq!(
                (commit.overrides, commit.cleared),
                (
                    Vec::new(),
                    cleared.iter().map(|key| (*key).to_owned()).collect()
                ),
                "gate {gate}"
            );
        }
    }

    #[gpui::test]
    fn switching_a_same_key_optional_on_commits_what_was_typed(cx: &mut TestAppContext) {
        let (form, vcx, _rt) = open_launch(
            cx,
            launch_with(
                vec![same_key_optional(cooldown_integer())],
                stored_cooldown(Some(Variant::Bool(false))),
            ),
        );
        let heard = std::rc::Rc::new(std::cell::RefCell::new(None));
        let sink = std::rc::Rc::clone(&heard);
        let _subscription = vcx.update(|_window, cx| {
            cx.subscribe(&form, move |_, event: &SubFormEvent, _| {
                if let SubFormEvent::Commit(commit) = event {
                    *sink.borrow_mut() = Some(commit.clone());
                }
            })
        });
        vcx.update(|_window, cx| {
            form.update(cx, |form, cx| {
                for field in &mut form.fields {
                    match field {
                        SubFormField::Bool { value, .. } => *value = true,
                        SubFormField::Input { input, .. } => {
                            input.update(cx, |input, cx| input.set_content("45".to_owned(), cx));
                        }
                        _ => {}
                    }
                }
                form.submit(cx);
            })
        });

        let commit = heard
            .borrow_mut()
            .take()
            .unwrap_or_else(|| panic!("45 is in range, so Save must commit"));
        assert_eq!(
            (commit.overrides, commit.cleared),
            (
                vec![(COOLDOWN_FIELD.to_owned(), Variant::Int(45))],
                Vec::new()
            )
        );
    }

    #[gpui::test]
    fn a_distinct_key_toggle_commits_its_own_bool_and_clears_nothing(cx: &mut TestAppContext) {
        for (stored_toggle, flipped, committed_toggle) in [
            (true, false, true),
            (false, false, false),
            (true, true, false),
            (false, true, true),
        ] {
            let config: SubActionConfig = [
                (COUNT_GATE.to_owned(), Variant::Bool(stored_toggle)),
                (COUNT_FIELD.to_owned(), Variant::Int(5)),
            ]
            .into_iter()
            .collect();
            let commit = submitted_commit(
                vec![count_spec(true)],
                config,
                flipped.then_some(COUNT_GATE),
                cx,
            )
            .unwrap_or_else(|| panic!("an in-range count must not block Save"));
            assert_eq!(
                (
                    override_for(&commit.overrides, COUNT_GATE),
                    commit.cleared.is_empty()
                ),
                (Some(&Variant::Bool(committed_toggle)), true),
                "stored {stored_toggle}, flipped {flipped}"
            );
        }
    }

    #[gpui::test]
    fn picking_minutes_and_typing_an_amount_commits_it_as_millis(cx: &mut TestAppContext) {
        let launch = launch_with(vec![duration_spec()], stored_millis(Variant::Int(30_000)));
        let (form, vcx, _rt) = open_launch(cx, launch);
        let heard = std::rc::Rc::new(std::cell::RefCell::new(None));
        let sink = std::rc::Rc::clone(&heard);
        let _subscription = vcx.update(|_window, cx| {
            cx.subscribe(&form, move |_, event: &SubFormEvent, _| {
                if let SubFormEvent::Commit(commit) = event {
                    *sink.borrow_mut() = Some(commit.overrides.clone());
                }
            })
        });

        vcx.update(|window, cx| {
            form.update(cx, |form, cx| {
                form.open_select_picker(duration_unit_picker(), window, cx);
                form.pick_select_option(forge_registry::DURATION_UNIT_MINUTES.to_owned(), cx);
            })
        });
        vcx.update(|_window, cx| {
            let input = form
                .read(cx)
                .fields
                .iter()
                .find_map(|field| match field {
                    SubFormField::UnitAmount { input, .. } => Some(input.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("a duration spec builds an amount field"));
            input.update(cx, |input, cx| input.set_content("2".to_owned(), cx));
        });
        vcx.update(|_window, cx| form.update(cx, |form, cx| form.submit(cx)));

        let overrides = heard
            .borrow_mut()
            .take()
            .unwrap_or_else(|| panic!("2 minutes is in range, so Save must commit"));
        assert_eq!(
            overrides,
            [(DURATION_FIELD.to_owned(), Variant::Int(120_000))]
        );
    }
}
