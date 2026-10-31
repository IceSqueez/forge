use super::*;
use crate::async_bridge::{self, WriteOutcome};
use crate::config_field_label::config_field_labels;
use crate::config_form::INTEGER_PLACEHOLDER;
use crate::config_form::{
    ChoiceDropdown, ChoiceSupport, CollectionChoices, ConfigFieldHandlers, FoldContext,
    collect_field_values, fold_config_field, set_picked_value, sparse_overrides,
};
use crate::repo_failure::RepoFailure;
use crate::trigger_form_shape::{trigger_form_defaults, trigger_form_specs};
use forge_components::ActivePresentation;
use forge_components::FONT_LABEL;
use forge_components::{InputEvent, Picker, PickerEvent, TextInput, tr};
use forge_registry::effective_config;
use gpui::SharedString;

mod sections;
mod sheet;

pub(super) const DETAIL_SHEET_W: Pixels = px(420.0);
const CFG_VAL_FS: Pixels = FONT_LABEL;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailAfterSave {
    Keep,
    Rebuild,
}

impl TriggersRegistryView {
    pub(super) fn load_detail(&self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let action_repo = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { load_detail_data(&*repo, &*action_repo, id).await },
            move |this, result, cx| match result {
                Ok(Some(data)) => this.apply_detail(id, data, cx),
                Ok(None) => {
                    if this.selected == Some(id) {
                        this.detail = None;
                        cx.notify();
                    }
                }
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn apply_detail(
        &mut self,
        id: TriggerInstanceId,
        data: TriggerDetailData,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(id) {
            return;
        }
        let palette = cx.palette();
        let descriptor = self.registry.get(&data.instance.kind_id);
        let default = trigger_form_defaults(descriptor);
        let specs = trigger_form_specs(descriptor);
        let effective = effective_config(&default, &data.instance.overrides);

        let fold = FoldContext {
            kind_id: &data.instance.kind_id,
            config: &effective,
            defaults: &default,
            palette: &palette,
            choices: ChoiceSupport::Text,
            on_committed: Self::on_config_committed,
        };
        let mut fields: Vec<ConfigField> = Vec::new();
        let mut labels: Vec<SharedString> = Vec::new();
        for spec in &specs {
            fold_config_field(spec, None, &fold, &mut fields, cx);
            config_field_labels(&data.instance.kind_id, spec, &mut labels);
        }

        let cooldown_per_user = !data.instance.cooldown_global;
        let cooldown_seed = data.instance.cooldown_secs;
        let cooldown_input = self.build_cooldown_input(cooldown_seed, palette, cx);
        let cooldown_sub = cx.subscribe(&cooldown_input, Self::on_cooldown_committed);
        let permission_rung = data.instance.permission_rung;

        self.detail = Some(TriggerDetail {
            instance: data.instance,
            fields,
            labels,
            used_in: data.used_in,
            cooldown_input,
            cooldown_per_user,
            permission_rung,
            choices: CollectionChoices::for_specs(&specs),
            _cooldown_sub: cooldown_sub,
        });
        self.start_option_choices(cx);
        cx.notify();
    }

    pub(super) fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.detail = None;
        cx.notify();
    }

    fn on_config_committed(&mut self, event: &InputEvent, cx: &mut Context<Self>) {
        if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
            self.commit_config(cx);
        }
    }

    fn on_cooldown_committed(
        &mut self,
        field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) | InputEvent::Blurred(_) => self.commit_config(cx),
            InputEvent::Cancelled => {
                field.update(cx, |input, cx| input.restore_committed(cx));
            }
            InputEvent::Changed(_) => {}
        }
    }

    fn build_cooldown_input(
        &self,
        seed: u32,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        cx.new(|cx| {
            let mut input = TextInput::new(INTEGER_PLACEHOLDER, cx).with_palette(palette);
            if seed > 0 {
                input.set_content(seed.to_string(), cx);
            }
            input
        })
    }

    fn toggle_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            for field in &mut detail.fields {
                if let ConfigField::Bool { key: k, value, .. } = field
                    && *k == key
                {
                    *value = !*value;
                }
            }
        }
        self.commit_config(cx);
    }

    fn slide_config_field(&mut self, key: String, next: i64, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            for field in &mut detail.fields {
                if let ConfigField::Slide { key: k, value, .. } = field
                    && *k == key
                {
                    *value = next;
                }
            }
        }
        self.commit_config(cx);
    }

    fn pick_config_field(&mut self, key: String, choice: String, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            set_picked_value(&mut detail.fields, &key, &choice, cx);
        }
        self.commit_config(cx);
    }

    fn open_detail_choice(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            detail.choices.toggle(
                &detail.fields,
                key,
                Self::on_detail_choice_event,
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn close_detail_choice(&mut self, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            detail.choices.close();
        }
        cx.notify();
    }

    fn on_detail_choice_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(detail) = self.detail.as_mut() else {
            return;
        };
        match event {
            PickerEvent::Selected(value) => {
                if let Some(key) = detail.choices.take_open_key() {
                    self.pick_config_field(key, value.to_string(), cx);
                }
            }
            PickerEvent::Cancelled => {
                detail.choices.close();
                cx.notify();
            }
        }
    }

    fn detail_config_handlers(&self) -> ConfigFieldHandlers<Self> {
        ConfigFieldHandlers {
            toggle: Self::toggle_config_field,
            slide: Self::slide_config_field,
            pick: Self::pick_config_field,
            choice: Some(ChoiceDropdown {
                open: Self::open_detail_choice,
                close: Self::close_detail_choice,
                active: self
                    .detail
                    .as_ref()
                    .and_then(|detail| detail.choices.active()),
            }),
        }
    }

    fn toggle_cooldown_scope(&mut self, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            detail.cooldown_per_user = !detail.cooldown_per_user;
        }
        self.commit_config(cx);
    }

    fn set_permission_rung(&mut self, rung: PermissionRung, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_mut() else {
            return;
        };
        if detail.permission_rung == rung {
            return;
        }
        detail.permission_rung = rung;
        self.commit_config(cx);
        cx.notify();
    }

    fn pending_instance(&self, cx: &App) -> Option<TriggerInstance> {
        let detail = self.detail.as_ref()?;
        let default = trigger_form_defaults(self.registry.get(&detail.instance.kind_id));
        let mut buffer = effective_config(&default, &detail.instance.overrides);
        collect_field_values(&detail.fields, &mut buffer, cx);

        let mut instance = detail.instance.clone();
        instance.overrides = sparse_overrides(&default, &buffer);
        if let Some(cooldown) = parse_cooldown(detail.cooldown_input.read(cx).content()) {
            instance.cooldown_secs = cooldown;
        }
        instance.cooldown_global = !detail.cooldown_per_user;
        instance.permission_rung = detail.permission_rung;
        Some(instance)
    }

    fn flag_cooldown_entry(&self, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_ref() {
            let input = detail.cooldown_input.clone();
            let invalid = parse_cooldown(input.read(cx).content()).is_none();
            input.update(cx, |input, cx| input.set_invalid(invalid, cx));
        }
    }

    pub(super) fn commit_config(&mut self, cx: &mut Context<Self>) {
        self.flag_cooldown_entry(cx);
        let Some(instance) = self.pending_instance(cx) else {
            return;
        };
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        if detail.instance == instance {
            return;
        }
        if self.refuses_instance(&instance, cx) {
            return;
        }
        if let Some(detail) = self.detail.as_mut() {
            detail.instance = instance.clone();
        }
        self.save_detail(instance, DetailAfterSave::Keep, cx);
    }

    fn instance_config(&self, instance: &TriggerInstance) -> TriggerConfig {
        let default = trigger_form_defaults(self.registry.get(&instance.kind_id));
        effective_config(&default, &instance.overrides)
    }

    fn holds_valid_config(&self, instance: &TriggerInstance) -> bool {
        self.registry
            .get(&instance.kind_id)
            .is_none_or(|descriptor| {
                descriptor
                    .check_config(&self.instance_config(instance))
                    .is_ok()
            })
    }

    fn refuses_instance(&self, instance: &TriggerInstance, cx: &mut Context<Self>) -> bool {
        self.refuse_invalid_config(&instance.kind_id, &self.instance_config(instance), cx)
    }

    fn save_detail(
        &mut self,
        instance: TriggerInstance,
        after: DetailAfterSave,
        cx: &mut Context<Self>,
    ) {
        let id = instance.id;
        let repo = Arc::clone(&self.repo);
        self.detail_writers.writer(id).write(
            async move { repo.save(&instance).await },
            move |this, outcome, cx| this.detail_save_settled(id, after, outcome, cx),
            cx,
        );
    }

    fn detail_save_settled(
        &mut self,
        id: TriggerInstanceId,
        after: DetailAfterSave,
        outcome: WriteOutcome,
        cx: &mut Context<Self>,
    ) {
        let rebuild = match outcome {
            WriteOutcome::Written => {
                self.reload(cx);
                after == DetailAfterSave::Rebuild
            }
            WriteOutcome::Superseded => after == DetailAfterSave::Rebuild,
            WriteOutcome::Failed(reason) => {
                self.on_repo_error(&reason, cx);
                true
            }
        };
        if rebuild && self.selected == Some(id) {
            self.load_detail(id, cx);
        }
    }

    pub(super) fn persist_detail_on_release(&mut self, cx: &mut App) {
        let Some(instance) = self.pending_instance(cx) else {
            return;
        };
        if self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.instance == instance)
            || !self.holds_valid_config(&instance)
        {
            return;
        }
        let repo = Arc::clone(&self.repo);
        let failure = tr!(
            "triggers_release_persist_failed",
            name = instance.name.as_str()
        );
        self.detail_writers.writer(instance.id).write_on_release(
            async move { repo.save(&instance).await },
            failure,
            cx,
        );
    }

    fn revert_config_field(&mut self, key: String, cx: &mut Context<Self>) {
        let Some(mut instance) = self.pending_instance(cx) else {
            return;
        };
        instance.overrides.remove(&key);
        if self.refuses_instance(&instance, cx) {
            return;
        }
        if let Some(detail) = self.detail.as_mut() {
            detail.instance = instance.clone();
        }
        self.save_detail(instance, DetailAfterSave::Rebuild, cx);
    }

    fn use_as_template(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let src = detail.instance.clone();
        let new_id = TriggerInstanceId::new();
        let instance = TriggerInstance {
            id: new_id,
            name: tr!("triggers_template_copy_name", name = src.name.as_str()),
            user_defined: true,
            ..src
        };
        let repo = Arc::clone(&self.repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                repo.save(&instance).await.map_err(RepoFailure::from)?;
                load_rows(&*repo).await
            },
            move |this, result, cx| match result {
                Ok(rows) => {
                    this.apply_rows(rows, cx);
                    this.selected = Some(new_id);
                    this.list_cursor = Some(new_id);
                    this.detail = None;
                    this.load_detail(new_id, cx);
                    cx.notify();
                }
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
        cx.notify();
    }
}

fn parse_cooldown(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() {
        return Some(0);
    }
    text.parse::<u32>().ok()
}

async fn load_detail_data(
    repo: &dyn TriggerInstanceRepo,
    action_repo: &dyn ActionRepo,
    id: TriggerInstanceId,
) -> Result<Option<TriggerDetailData>, RepoFailure> {
    let Some(instance) = repo.get(id).await.map_err(RepoFailure::from)? else {
        return Ok(None);
    };
    let action_ids = repo.actions_using(id).await.map_err(RepoFailure::from)?;
    let mut used_in = Vec::with_capacity(action_ids.len());
    for action_id in action_ids {
        let name = action_repo
            .get(action_id)
            .await
            .map_err(RepoFailure::from)?
            .map(|a| a.name)
            .unwrap_or_else(|| action_id.to_string());
        used_in.push((action_id, name));
    }
    Ok(Some(TriggerDetailData { instance, used_in }))
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use forge_components::{Density, ThemeId};
    use forge_platform_core::CollectionFailure;
    use forge_registry::TriggerRegistry;
    use forge_registry::{FormField, TriggerCategory};
    use forge_storage::StorageError;
    use forge_types::{PlatformScope, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant};

    use super::*;
    use crate::screen_shortcuts::ScreenShortcuts;
    use crate::test_support::{
        Declares, FakeCollections, StubActions, StubTrigger, error_toasts, owned_reward, pump,
        pump_until, runtime, test_backend, trigger_registry,
    };
    use forge_components::Presentation;
    use forge_host::integrations::BuiltinRegistry;

    pub(in crate::triggers_screen) const KIND: &str = "midi.input.note_on";
    const NAME: &str = "Note on";
    const SEEDED_COOLDOWN: u32 = 30;
    const SAVE_REFUSED: &str = "database is locked";
    pub(in crate::triggers_screen) const GREETING_FIELD: &str = "greeting";
    pub(in crate::triggers_screen) const REFUSED_GREETING: &str = "refused";

    pub(in crate::triggers_screen) struct RecordingTriggers {
        instance: Mutex<TriggerInstance>,
        saved: Mutex<Vec<u32>>,
        saved_overrides: Mutex<Vec<TriggerConfig>>,
        saves_fail: AtomicBool,
        row_lists: AtomicUsize,
    }

    impl RecordingTriggers {
        fn new(instance: TriggerInstance) -> Arc<Self> {
            Arc::new(Self {
                instance: Mutex::new(instance),
                saved: Mutex::new(Vec::new()),
                saved_overrides: Mutex::new(Vec::new()),
                saves_fail: AtomicBool::new(false),
                row_lists: AtomicUsize::new(0),
            })
        }

        fn stored(&self) -> TriggerInstance {
            self.instance.lock().unwrap().clone()
        }

        fn fail_saves(&self) {
            self.saves_fail.store(true, Ordering::SeqCst);
        }

        pub(in crate::triggers_screen) fn saved_cooldowns(&self) -> Vec<u32> {
            self.saved.lock().unwrap().clone()
        }

        pub(in crate::triggers_screen) fn last_saved_overrides(&self) -> Option<TriggerConfig> {
            self.saved_overrides.lock().unwrap().last().cloned()
        }
    }

    #[async_trait::async_trait]
    impl TriggerInstanceRepo for RecordingTriggers {
        async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError> {
            Ok(vec![self.instance.lock().unwrap().clone()])
        }

        async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError> {
            self.row_lists.fetch_add(1, Ordering::SeqCst);
            Ok(vec![self.instance.lock().unwrap().clone()])
        }

        async fn list_for_action(&self, _: ActionId) -> Result<Vec<TriggerInstance>, StorageError> {
            Ok(Vec::new())
        }

        async fn actions_using(&self, _: TriggerInstanceId) -> Result<Vec<ActionId>, StorageError> {
            Ok(Vec::new())
        }

        async fn link_action(&self, _: ActionId, _: TriggerInstanceId) -> Result<(), StorageError> {
            unreachable!("the detail sheet never links an action")
        }

        async fn link_action_at(
            &self,
            _: ActionId,
            _: TriggerInstanceId,
            _: i64,
        ) -> Result<(), StorageError> {
            unreachable!("the detail sheet never links an action")
        }

        async fn unlink_action(
            &self,
            _: ActionId,
            _: TriggerInstanceId,
        ) -> Result<bool, StorageError> {
            unreachable!("the detail sheet never unlinks an action")
        }

        async fn get(&self, _: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError> {
            Ok(Some(self.instance.lock().unwrap().clone()))
        }

        async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError> {
            if self.saves_fail.load(Ordering::SeqCst) {
                return Err(StorageError::Connection {
                    reason: SAVE_REFUSED.to_owned(),
                });
            }
            self.saved.lock().unwrap().push(instance.cooldown_secs);
            self.saved_overrides
                .lock()
                .unwrap()
                .push(instance.overrides.clone());
            *self.instance.lock().unwrap() = instance.clone();
            Ok(())
        }

        async fn delete(&self, _: TriggerInstanceId) -> Result<bool, StorageError> {
            unreachable!("the detail sheet never deletes an instance")
        }

        async fn upsert_default(
            &self,
            _: &str,
            _: &str,
        ) -> Result<TriggerInstanceId, StorageError> {
            unreachable!("the detail sheet never upserts a default")
        }

        async fn set_enabled(
            &self,
            _: TriggerInstanceId,
            enabled: bool,
        ) -> Result<(), StorageError> {
            self.instance.lock().unwrap().enabled = enabled;
            Ok(())
        }
    }

    pub(in crate::triggers_screen) struct Fixture {
        view: Option<Entity<TriggersRegistryView>>,
        pub(in crate::triggers_screen) repo: Arc<RecordingTriggers>,
        pub(in crate::triggers_screen) rt: tokio::runtime::Runtime,
    }

    impl Fixture {
        fn new(cx: &mut gpui::TestAppContext) -> Self {
            Self::build(
                cx,
                TriggerRegistry::new(),
                KIND,
                TriggerConfig::new(),
                BuiltinRegistry::default(),
            )
        }

        pub(in crate::triggers_screen) fn build(
            cx: &mut gpui::TestAppContext,
            registry: TriggerRegistry,
            kind: &str,
            overrides: TriggerConfig,
            builtins: BuiltinRegistry,
        ) -> Self {
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            });
            let rt = runtime();
            let id = TriggerInstanceId::new();
            let instance = TriggerInstance {
                id,
                kind_id: kind.to_owned(),
                name: NAME.to_owned(),
                overrides,
                enabled: true,
                user_defined: false,
                platform_scope: PlatformScope::Any,
                cooldown_secs: SEEDED_COOLDOWN,
                cooldown_global: true,
                permission_rung: PermissionRung::Everyone,
            };
            let repo = RecordingTriggers::new(instance.clone());
            let (backend, _writes) = test_backend();
            let view = cx.update(|cx| {
                cx.new(|cx| {
                    TriggersRegistryView::new(
                        Arc::clone(&repo) as Arc<dyn TriggerInstanceRepo>,
                        Arc::new(StubActions),
                        Arc::new(registry),
                        backend as Arc<dyn forge_storage::SettingsRepo>,
                        rt.handle().clone(),
                        None,
                        cx,
                    )
                    .with_builtins(builtins)
                })
            });
            let mut fixture = Self {
                view: Some(view),
                repo,
                rt,
            };
            fixture.settle(cx);
            fixture.view().update(cx, |view, cx| {
                view.selected = Some(id);
                view.apply_detail(
                    id,
                    TriggerDetailData {
                        instance,
                        used_in: Vec::new(),
                    },
                    cx,
                );
            });
            fixture.settle(cx);
            fixture
        }

        pub(in crate::triggers_screen) fn view(&self) -> &Entity<TriggersRegistryView> {
            self.view.as_ref().unwrap()
        }

        pub(in crate::triggers_screen) fn settle(&mut self, cx: &mut gpui::TestAppContext) {
            pump(&self.rt);
            cx.update(|_cx| {});
            cx.run_until_parked();
            pump(&self.rt);
            cx.run_until_parked();
        }

        fn cooldown_input(&self, cx: &mut gpui::TestAppContext) -> Entity<TextInput> {
            self.view().read_with(cx, |view, _| {
                view.detail.as_ref().unwrap().cooldown_input.clone()
            })
        }

        fn cooldown_text(&self, cx: &mut gpui::TestAppContext) -> String {
            let input = self.cooldown_input(cx);
            input.read_with(cx, |field, _| field.content().to_owned())
        }

        fn detail_instance(&self, cx: &mut gpui::TestAppContext) -> TriggerInstance {
            self.view()
                .read_with(cx, |view, _| view.detail.as_ref().unwrap().instance.clone())
        }

        fn wait_for_detail(
            &self,
            cx: &mut gpui::TestAppContext,
            mut reached: impl FnMut(&TriggerInstance) -> bool,
        ) {
            pump_until(&self.rt, || {
                cx.run_until_parked();
                reached(&self.detail_instance(cx))
            });
        }

        fn type_cooldown(&self, cx: &mut gpui::TestAppContext, text: &str) {
            self.view().update(cx, |view, cx| {
                let input = view.detail.as_ref().unwrap().cooldown_input.clone();
                input.update(cx, |field, cx| field.set_content(text.to_owned(), cx));
            });
        }

        fn commit(&mut self, cx: &mut gpui::TestAppContext) {
            self.view().update(cx, |view, cx| view.commit_config(cx));
            self.settle(cx);
        }

        fn release(&mut self, cx: &mut gpui::TestAppContext) {
            self.view = None;
            cx.update(|_cx| {});
            cx.run_until_parked();
            pump(&self.rt);
        }

        fn type_greeting(&self, cx: &mut gpui::TestAppContext, text: &str) {
            let input = self.view().read_with(cx, |view, _| {
                view.detail
                    .as_ref()
                    .unwrap()
                    .fields
                    .iter()
                    .find_map(|field| match field {
                        ConfigField::Input { key, input, .. } if key == GREETING_FIELD => {
                            Some(input.clone())
                        }
                        _ => None,
                    })
                    .unwrap()
            });
            input.update(cx, |field, cx| field.set_content(text.to_owned(), cx));
        }

        fn detail_greeting(&self, cx: &mut gpui::TestAppContext) -> Option<Variant> {
            self.detail_instance(cx)
                .overrides
                .get(GREETING_FIELD)
                .cloned()
        }
    }

    #[gpui::test]
    fn a_sheet_torn_down_over_an_uncommitted_cooldown_writes_it_through_the_repo(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        fixture.type_cooldown(cx, "5");

        fixture.release(cx);

        assert_eq!(fixture.repo.saved_cooldowns(), [5]);
    }

    #[gpui::test]
    fn a_sheet_torn_down_over_an_untouched_form_writes_nothing(cx: &mut gpui::TestAppContext) {
        let mut fixture = Fixture::new(cx);

        fixture.release(cx);

        assert!(fixture.repo.saved_cooldowns().is_empty());
    }

    #[gpui::test]
    fn a_sheet_torn_down_after_its_commit_landed_writes_nothing_more(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        fixture.type_cooldown(cx, "5");
        fixture.commit(cx);
        fixture.commit(cx);

        fixture.release(cx);

        assert_eq!(fixture.repo.saved_cooldowns(), [5]);
    }

    #[gpui::test]
    fn an_unreadable_cooldown_keeps_the_stored_value_and_flags_the_field(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        fixture.type_cooldown(cx, "later");
        let flagged = Arc::new(AtomicUsize::new(0));
        let field_changes = Arc::clone(&flagged);
        let field = fixture.cooldown_input(cx);
        let _watch = cx.update(|cx| {
            cx.observe(&field, move |_, _| {
                field_changes.fetch_add(1, Ordering::SeqCst);
            })
        });

        fixture.commit(cx);
        let flagged_by_the_commit = flagged.load(Ordering::SeqCst);
        fixture.commit(cx);
        let stored_after_commits = fixture.detail_instance(cx).cooldown_secs;
        fixture.release(cx);

        assert_eq!(
            (
                flagged_by_the_commit,
                flagged.load(Ordering::SeqCst),
                stored_after_commits,
                fixture.repo.saved_cooldowns(),
            ),
            (1, 1, SEEDED_COOLDOWN, Vec::new())
        );
    }

    #[test]
    fn parse_cooldown_reads_a_blank_field_as_zero_and_refuses_what_is_not_a_u32() {
        for (text, expected) in [
            ("", Some(0)),
            ("   ", Some(0)),
            ("0", Some(0)),
            (" 45 ", Some(45)),
            ("4294967295", Some(u32::MAX)),
            ("4294967296", None),
            ("-1", None),
            ("1.5", None),
            ("later", None),
        ] {
            assert_eq!(parse_cooldown(text), expected, "{text:?}");
        }
    }

    const REWARD_KIND: &str = "twitch.channel_points.redemption";
    const REWARD_FIELD: &str = "reward_id";
    const LEGACY_REWARD_ID: &str = "0c7f3a52-legacy";

    fn reward_registry() -> TriggerRegistry {
        trigger_registry(vec![
            StubTrigger::new(
                REWARD_KIND,
                "Reward redeemed",
                TriggerCategory::ChannelPoints,
                Declares::Nothing,
            )
            .with_fields(vec![FormField::DynamicSelect {
                key: "reward_id",
                label: "Reward",
                options_key: "collections.twitch.rewards",
            }]),
        ])
    }

    fn reward_sheet(
        cx: &mut gpui::TestAppContext,
        fake: &Arc<FakeCollections>,
        stored: Option<&str>,
    ) -> Fixture {
        let overrides: TriggerConfig = stored
            .map(|id| (REWARD_FIELD.to_owned(), Variant::String(id.to_owned())))
            .into_iter()
            .collect();
        Fixture::build(
            cx,
            reward_registry(),
            REWARD_KIND,
            overrides,
            fake.installed_as("twitch"),
        )
    }

    fn reward_choice(fixture: &Fixture, cx: &mut gpui::TestAppContext) -> (String, Vec<String>) {
        fixture.view().read_with(cx, |view, _| {
            view.detail
                .as_ref()
                .unwrap()
                .fields
                .iter()
                .find_map(|field| match field {
                    ConfigField::Choice {
                        key,
                        selected,
                        options,
                        ..
                    } if key == REWARD_FIELD => Some((
                        selected.clone(),
                        options.iter().map(|(value, _)| value.clone()).collect(),
                    )),
                    _ => None,
                })
                .unwrap()
        })
    }

    #[gpui::test]
    fn a_reward_filter_offers_match_any_first_then_every_listed_reward(
        cx: &mut gpui::TestAppContext,
    ) {
        let fake = FakeCollections::listing(Ok(vec![
            owned_reward("r-1", "Hydrate"),
            owned_reward("r-2", "Stretch"),
        ]));

        let fixture = reward_sheet(cx, &fake, None);

        assert_eq!(reward_choice(&fixture, cx).1, ["", "r-1", "r-2"]);
    }

    #[gpui::test]
    fn a_stored_reward_id_the_listing_no_longer_has_still_shows_as_the_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let fake = FakeCollections::listing(Ok(vec![owned_reward("r-1", "Hydrate")]));

        let fixture = reward_sheet(cx, &fake, Some(LEGACY_REWARD_ID));

        assert_eq!(reward_choice(&fixture, cx).0, LEGACY_REWARD_ID);
    }

    #[gpui::test]
    fn a_revision_bump_reoffers_the_fresh_reward_listing(cx: &mut gpui::TestAppContext) {
        let fake = FakeCollections::listing(Ok(vec![owned_reward("r-1", "Hydrate")]));
        let mut fixture = reward_sheet(cx, &fake, None);
        fake.relist(Ok(vec![owned_reward("r-9", "Brand new")]));

        fake.bump();
        fixture.settle(cx);

        assert_eq!(reward_choice(&fixture, cx).1, ["", "r-9"]);
    }

    #[gpui::test]
    fn a_reward_listing_that_fails_after_a_disconnect_withdraws_the_stale_rewards(
        cx: &mut gpui::TestAppContext,
    ) {
        let fake = FakeCollections::listing(Ok(vec![owned_reward("r-1", "Hydrate")]));
        let mut fixture = reward_sheet(cx, &fake, None);
        fake.relist(Err(CollectionFailure::NotConnected));

        fake.bump();
        fixture.settle(cx);

        assert_eq!(reward_choice(&fixture, cx).1, [""]);
    }

    #[gpui::test]
    fn picking_a_reward_saves_its_id_as_the_filter(cx: &mut gpui::TestAppContext) {
        let fake = FakeCollections::listing(Ok(vec![owned_reward("r-1", "Hydrate")]));
        let mut fixture = reward_sheet(cx, &fake, Some(LEGACY_REWARD_ID));

        fixture.view().update(cx, |view, cx| {
            view.pick_config_field(REWARD_FIELD.to_owned(), "r-1".to_owned(), cx)
        });
        fixture.settle(cx);

        assert_eq!(
            fixture
                .repo
                .last_saved_overrides()
                .and_then(|overrides| overrides.get(REWARD_FIELD).cloned()),
            Some(Variant::String("r-1".to_owned()))
        );
    }

    #[gpui::test]
    fn the_save_shortcut_commits_the_open_sheet_unless_a_delete_prompt_is_showing(
        cx: &mut gpui::TestAppContext,
    ) {
        for (delete_prompt, expected, case) in [
            (false, vec![5], "sheet open"),
            (true, Vec::new(), "delete prompt showing"),
        ] {
            let mut fixture = Fixture::new(cx);
            fixture.type_cooldown(cx, "5");
            if delete_prompt {
                fixture.view().update(cx, |view, _| {
                    view.pending_delete.request(TriggerInstanceId::new())
                });
            }

            fixture.view().update(cx, |view, cx| view.save_shortcut(cx));
            fixture.settle(cx);

            assert_eq!(fixture.repo.saved_cooldowns(), expected, "{case}");
        }
    }

    #[gpui::test]
    fn a_landed_commit_keeps_the_field_being_typed_in_with_its_content(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        let input = fixture.cooldown_input(cx);
        fixture.type_cooldown(cx, "5");
        let lists_before = fixture.repo.row_lists.load(Ordering::SeqCst);

        fixture.commit(cx);
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            fixture.repo.row_lists.load(Ordering::SeqCst) > lists_before
        });
        fixture.settle(cx);

        assert_eq!(
            (
                fixture.cooldown_input(cx).entity_id(),
                fixture.cooldown_text(cx)
            ),
            (input.entity_id(), "5".to_owned())
        );
    }

    #[gpui::test]
    fn a_commit_after_a_toggle_keeps_the_toggled_state(cx: &mut gpui::TestAppContext) {
        let mut fixture = Fixture::new(cx);
        let id = fixture.detail_instance(cx).id;

        fixture
            .view()
            .update(cx, |view, cx| view.toggle_enable(id, cx));
        fixture.wait_for_detail(cx, |instance| !instance.enabled);
        fixture.type_cooldown(cx, "5");
        fixture.commit(cx);

        assert_eq!(
            (
                fixture.repo.saved_cooldowns(),
                fixture.repo.stored().enabled
            ),
            (vec![5], false)
        );
    }

    #[gpui::test]
    fn a_failed_save_rolls_the_sheet_back_to_the_stored_trigger_with_one_toast(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let mut fixture = Fixture::new(cx);
        fixture.repo.fail_saves();
        fixture.type_cooldown(cx, "5");

        fixture.commit(cx);
        fixture.wait_for_detail(cx, |instance| instance.cooldown_secs == SEEDED_COOLDOWN);

        assert_eq!(
            (
                fixture.cooldown_text(cx),
                cx.update(|cx| error_toasts(cx)),
                fixture.repo.stored().cooldown_secs,
            ),
            (SEEDED_COOLDOWN.to_string(), 1, SEEDED_COOLDOWN)
        );
    }

    #[gpui::test]
    fn a_failed_write_at_teardown_tells_the_user_which_trigger_lost_its_edit(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::i18n::install_language(forge_storage::Language::En);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let mut fixture = Fixture::new(cx);
        fixture.repo.fail_saves();
        fixture.type_cooldown(cx, "5");

        fixture.release(cx);
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            cx.update(|cx| error_toasts(cx)) == 1
        });

        let message = cx.update(|cx| {
            cx.global::<forge_components::Toasts>().items()[0]
                .message
                .to_string()
        });
        assert!(message.contains(NAME), "{message}");
    }

    fn greeting(text: &str) -> Variant {
        Variant::String(text.to_owned())
    }

    pub(in crate::triggers_screen) fn refusing_registry(
        defaults: TriggerConfig,
    ) -> TriggerRegistry {
        trigger_registry(vec![
            StubTrigger::new(KIND, NAME, TriggerCategory::Midi, Declares::Nothing)
                .with_fields(vec![FormField::Text {
                    key: "greeting",
                    label: "Greeting",
                    placeholder: "",
                }])
                .with_defaults(defaults)
                .refusing(GREETING_FIELD, REFUSED_GREETING),
        ])
    }

    fn refusing_sheet(cx: &mut gpui::TestAppContext, defaults: TriggerConfig) -> Fixture {
        crate::i18n::install_language(forge_storage::Language::En);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let overrides: TriggerConfig = [(GREETING_FIELD.to_owned(), greeting("hello"))]
            .into_iter()
            .collect();
        Fixture::build(
            cx,
            refusing_registry(defaults),
            KIND,
            overrides,
            BuiltinRegistry::default(),
        )
    }

    #[gpui::test]
    fn a_commit_the_kind_refuses_saves_nothing_and_keeps_the_sheet_on_the_stored_value(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = refusing_sheet(cx, TriggerConfig::new());
        fixture.type_greeting(cx, REFUSED_GREETING);

        fixture.commit(cx);

        assert_eq!(
            (
                fixture.repo.last_saved_overrides(),
                fixture.detail_greeting(cx),
            ),
            (None, Some(greeting("hello")))
        );
    }

    #[gpui::test]
    fn a_refused_commit_names_the_field_in_one_error_toast(cx: &mut gpui::TestAppContext) {
        let mut fixture = refusing_sheet(cx, TriggerConfig::new());
        fixture.type_greeting(cx, REFUSED_GREETING);

        fixture.commit(cx);

        let toast = cx.update(|cx| {
            (
                error_toasts(cx),
                cx.global::<forge_components::Toasts>().items()[0]
                    .message
                    .to_string(),
            )
        });
        assert_eq!(toast.0, 1);
        assert!(toast.1.contains("Greeting"), "{}", toast.1);
    }

    #[gpui::test]
    fn a_commit_after_correcting_a_refused_value_saves_the_correction(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = refusing_sheet(cx, TriggerConfig::new());
        fixture.type_greeting(cx, REFUSED_GREETING);
        fixture.commit(cx);
        fixture.type_greeting(cx, "corrected");

        fixture.commit(cx);
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            fixture.repo.last_saved_overrides().is_some()
        });

        assert_eq!(
            fixture
                .repo
                .last_saved_overrides()
                .and_then(|saved| saved.get(GREETING_FIELD).cloned()),
            Some(greeting("corrected"))
        );
    }

    #[gpui::test]
    fn reverting_a_field_to_a_default_the_kind_refuses_keeps_the_override(
        cx: &mut gpui::TestAppContext,
    ) {
        let defaults: TriggerConfig = [(GREETING_FIELD.to_owned(), greeting(REFUSED_GREETING))]
            .into_iter()
            .collect();
        let mut fixture = refusing_sheet(cx, defaults);

        fixture.view().update(cx, |view, cx| {
            view.revert_config_field(GREETING_FIELD.to_owned(), cx)
        });
        fixture.settle(cx);

        assert_eq!(
            (
                fixture.repo.last_saved_overrides(),
                fixture.detail_greeting(cx),
                cx.update(|cx| error_toasts(cx)),
            ),
            (None, Some(greeting("hello")), 1)
        );
    }

    #[gpui::test]
    fn a_sheet_torn_down_over_a_value_the_kind_refuses_writes_nothing(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = refusing_sheet(cx, TriggerConfig::new());
        fixture.type_greeting(cx, REFUSED_GREETING);
        fixture.type_cooldown(cx, "5");

        fixture.release(cx);

        assert!(fixture.repo.saved_cooldowns().is_empty());
    }

    fn greeting_sheet(cx: &mut gpui::TestAppContext) -> Fixture {
        let registry = trigger_registry(vec![
            StubTrigger::new(KIND, NAME, TriggerCategory::Midi, Declares::Nothing).with_fields(
                vec![FormField::Text {
                    key: "greeting",
                    label: "Greeting",
                    placeholder: "",
                }],
            ),
        ]);
        let overrides: TriggerConfig = [(
            GREETING_FIELD.to_owned(),
            Variant::String("hello".to_owned()),
        )]
        .into_iter()
        .collect();
        Fixture::build(cx, registry, KIND, overrides, BuiltinRegistry::default())
    }

    #[gpui::test]
    fn reverting_one_field_keeps_the_other_unsaved_edits(cx: &mut gpui::TestAppContext) {
        let mut fixture = greeting_sheet(cx);
        fixture.type_cooldown(cx, "5");

        fixture.view().update(cx, |view, cx| {
            view.revert_config_field(GREETING_FIELD.to_owned(), cx)
        });
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            !fixture.repo.saved_cooldowns().is_empty()
        });
        fixture.settle(cx);

        let stored = fixture.repo.stored();
        assert_eq!(
            (
                stored.cooldown_secs,
                stored.overrides.contains_key(GREETING_FIELD),
                fixture.cooldown_text(cx),
            ),
            (5, false, "5".to_owned())
        );
    }

    #[gpui::test]
    fn reverting_a_field_shows_its_default_again(cx: &mut gpui::TestAppContext) {
        let mut fixture = greeting_sheet(cx);
        let shown_greeting = |fixture: &Fixture, cx: &mut gpui::TestAppContext| {
            fixture.view().read_with(cx, |view, cx| {
                view.detail
                    .as_ref()
                    .unwrap()
                    .fields
                    .iter()
                    .find_map(|field| match field {
                        ConfigField::Input { key, input, .. } if key == GREETING_FIELD => {
                            Some(input.read(cx).content().to_owned())
                        }
                        _ => None,
                    })
                    .unwrap()
            })
        };
        assert_eq!(shown_greeting(&fixture, cx), "hello");

        fixture.view().update(cx, |view, cx| {
            view.revert_config_field(GREETING_FIELD.to_owned(), cx)
        });
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            shown_greeting(&fixture, cx).is_empty()
        });
        fixture.settle(cx);

        assert_eq!(shown_greeting(&fixture, cx), "");
    }

    #[gpui::test]
    fn a_trigger_used_as_a_template_takes_the_cursor_with_the_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut fixture = Fixture::new(cx);
        let seeded = fixture.view().read_with(cx, |view, _| view.selected);

        fixture
            .view()
            .update(cx, |view, cx| view.use_as_template(cx));
        pump_until(&fixture.rt, || {
            cx.run_until_parked();
            fixture
                .view()
                .read_with(cx, |view, _| view.selected != seeded)
        });
        fixture.settle(cx);

        let (selected, cursor) = fixture
            .view()
            .read_with(cx, |view, _| (view.selected, view.list_cursor));
        assert_eq!(cursor, selected);
    }
}
