use super::*;
use crate::async_bridge::{self, WriteOutcome};
use crate::config_field_label::config_field_labels;
use crate::config_form::INTEGER_PLACEHOLDER;
use crate::config_form::{
    ChoiceDropdown, ChoiceSupport, CollectionChoices, ConfigFieldHandlers, FoldContext,
    collect_field_values, fold_config_field, set_picked_value, set_slide_field, sparse_overrides,
    toggle_bool_field,
};
use crate::trigger_form_shape::{trigger_form_defaults, trigger_form_specs};
use forge_components::FONT_LABEL;
use forge_components::{InputEvent, Picker, PickerEvent, TextInput, tr};
use forge_registry::effective_config;
use gpui::{App, SharedString};

mod sections;
mod sheet;
#[cfg(test)]
pub(super) mod tests;

pub(super) const DETAIL_SHEET_W: Pixels = px(420.0);
const CFG_VAL_FS: Pixels = FONT_LABEL;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailAfterSave {
    Keep,
    Rebuild,
}

pub(super) enum TriggerPaneEvent {
    Closed,
    RowsChanged,
    ToggleEnable(TriggerInstanceId),
    RequestDelete(TriggerInstanceId),
    StartRename(TriggerInstanceId),
    OpenAction(ActionId),
    Duplicated {
        id: TriggerInstanceId,
        rows: Vec<TriggerInstanceRow>,
    },
}

impl EventEmitter<TriggerPaneEvent> for TriggerDetailPane {}

pub(super) struct TriggerDetailPane {
    deps: TriggerScreenDeps,
    detail_width: Pixels,
    detail_writers: KeyedSnapshotWriters<TriggerInstanceId>,
    selected: Option<TriggerInstanceId>,
    listed_uses: usize,
    detail: Option<TriggerDetail>,
    integrations: Option<SwitchWatch>,
    choice_feed: ChoiceFeed,
    _release: Subscription,
}

impl Render for TriggerDetailPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        match self.selected {
            Some(id) => self.render_detail_sheet(id, &palette, cx),
            None => div().into_any_element(),
        }
    }
}

impl TriggerDetailPane {
    pub(super) fn new(
        deps: TriggerScreenDeps,
        preselect: Option<TriggerInstanceId>,
        cx: &mut Context<Self>,
    ) -> Self {
        let detail_writers = KeyedSnapshotWriters::new(&deps.rt_handle);
        let choice_feed = ChoiceFeed::new(deps.rt_handle.clone());
        Self {
            deps,
            detail_width: DETAIL_SHEET_W,
            detail_writers,
            selected: preselect,
            listed_uses: 0,
            detail: None,
            integrations: None,
            choice_feed,
            _release: cx.on_release(|this, cx| this.persist_detail_on_release(cx)),
        }
    }

    pub(super) fn set_builtins(&mut self, builtins: BuiltinRegistry) {
        self.choice_feed.set_builtins(builtins);
    }

    pub(super) fn attach_switch(&mut self, switch: IntegrationSwitch, cx: &mut Context<Self>) {
        self.integrations = Some(switch.watch(cx, |this: &mut Self, off, cx| {
            if let Some(integrations) = &mut this.integrations {
                integrations.replace(off, cx);
            }
        }));
    }

    fn switched_off_owner(&self, kind_id: &str) -> Option<IntegrationId> {
        switched_off_owner(self.integrations.as_ref(), &self.deps.registry, kind_id)
    }

    fn uncovered_category(
        &self,
        kind_id: &str,
    ) -> Option<forge_platform_core::IntegrationCategory> {
        self.integrations.as_ref()?.uncovered_category(kind_id)
    }

    fn set_detail_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.detail_width != width {
            self.detail_width = width;
            cx.notify();
        }
    }

    pub(super) fn open_row(&mut self, row: &ListedRow, cx: &mut Context<Self>) {
        if self.selected != Some(row.id) {
            self.detail = None;
        }
        self.selected = Some(row.id);
        self.listed_uses = row.used_in_count;
        self.load_detail(row.id, cx);
        cx.notify();
    }

    pub(super) fn row_listed(&mut self, row: &ListedRow, cx: &mut Context<Self>) {
        if self.selected != Some(row.id) {
            return;
        }
        self.listed_uses = row.used_in_count;
        match self.detail.as_mut().filter(|d| d.instance.id == row.id) {
            Some(detail) => {
                detail.instance.enabled = row.enabled;
                detail.instance.name.clone_from(&row.name);
            }
            None => self.load_detail(row.id, cx),
        }
        cx.notify();
    }

    pub(super) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.detail = None;
        cx.notify();
    }

    pub(super) fn show_enabled(
        &mut self,
        id: TriggerInstanceId,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(detail) = self.detail.as_mut().filter(|d| d.instance.id == id) {
            detail.instance.enabled = enabled;
            cx.notify();
        }
    }

    pub(super) fn load_detail(&self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.deps.repo);
        let action_repo = Arc::clone(&self.deps.action_repo);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { load_detail_data(&*repo, &*action_repo, id).await },
            move |this, result, cx| match result {
                Ok(Some(data)) => this.apply_detail(id, data, cx),
                Ok(None) => {
                    if this.selected == Some(id) {
                        this.detail = None;
                        cx.notify();
                    }
                }
                Err(failure) => on_repo_error(&failure, cx),
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
        let descriptor = self.deps.registry.get(&data.instance.kind_id);
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
        start_option_choices(self, cx);
        cx.notify();
    }

    fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        cx.emit(TriggerPaneEvent::Closed);
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
            toggle_bool_field(&mut detail.fields, &key);
        }
        self.commit_config(cx);
    }

    fn slide_config_field(&mut self, key: String, next: i64, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut() {
            set_slide_field(&mut detail.fields, &key, next);
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
        let default = trigger_form_defaults(self.deps.registry.get(&detail.instance.kind_id));
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
        let default = trigger_form_defaults(self.deps.registry.get(&instance.kind_id));
        effective_config(&default, &instance.overrides)
    }

    fn holds_valid_config(&self, instance: &TriggerInstance) -> bool {
        self.deps
            .registry
            .get(&instance.kind_id)
            .is_none_or(|descriptor| {
                descriptor
                    .check_config(&self.instance_config(instance))
                    .is_ok()
            })
    }

    fn refuses_instance(&self, instance: &TriggerInstance, cx: &mut Context<Self>) -> bool {
        refuse_invalid_config(
            &self.deps.registry,
            &instance.kind_id,
            &self.instance_config(instance),
            cx,
        )
    }

    fn save_detail(
        &mut self,
        instance: TriggerInstance,
        after: DetailAfterSave,
        cx: &mut Context<Self>,
    ) {
        let id = instance.id;
        let repo = Arc::clone(&self.deps.repo);
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
                cx.emit(TriggerPaneEvent::RowsChanged);
                after == DetailAfterSave::Rebuild
            }
            WriteOutcome::Superseded => after == DetailAfterSave::Rebuild,
            WriteOutcome::Failed(reason) => {
                on_repo_error(&reason, cx);
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
        let repo = Arc::clone(&self.deps.repo);
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
        let repo = Arc::clone(&self.deps.repo);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { save_then_list(&*repo, &instance).await },
            move |_, result, cx| match result {
                Ok(rows) => cx.emit(TriggerPaneEvent::Duplicated { id: new_id, rows }),
                Err(failure) => on_repo_error(&failure, cx),
            },
            cx,
        );
        cx.notify();
    }
}

impl ChoiceHost for TriggerDetailPane {
    fn feed(&mut self) -> &mut ChoiceFeed {
        &mut self.choice_feed
    }

    fn option_keys(&self) -> Vec<String> {
        self.detail
            .as_ref()
            .map(|detail| detail.choices.option_keys())
            .unwrap_or_default()
    }

    fn refresh_choices(&mut self, blank: &str) {
        if let Some(detail) = self.detail.as_mut() {
            detail
                .choices
                .refresh(&mut detail.fields, self.choice_feed.options(), Some(blank));
        }
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
