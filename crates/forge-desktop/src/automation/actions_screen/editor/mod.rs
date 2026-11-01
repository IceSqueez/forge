use super::*;
use crate::shared::option_choices::{ChoiceOption, ChoiceOptions};
use forge_components::OverflowRevealingFocus;
use forge_components::{
    ActivePresentation, Density, FONT_SM, FONT_XS, ForgePalette, Icon, Spacing, icon, scaled,
    spacing, tr,
};
use forge_host::integrations::BuiltinRegistry;
use forge_types::SubActionStep;
use gpui::{AnyElement, App, Context, Task, div};

pub(in crate::automation::actions_screen) mod action_writes;
mod branch;
mod cards;
mod collection_choices;
mod detail_load;
mod header;
pub(in crate::automation::actions_screen) mod integration_gating;
mod select_options;
mod stats;
mod step_controls;
mod step_drag;
mod step_list;
mod step_picker;
mod step_presentation;
mod step_scope;
mod sub_form;
mod test_launch;
pub(in crate::automation::actions_screen) mod trigger_fill;
mod trigger_list;
mod trigger_picker;

pub(in crate::automation::actions_screen) use action_writes::{ActionSettings, ActionWrite};
pub(crate) use step_presentation::parse_variable_segments;
pub(super) use step_presentation::{StepGroup, UNKNOWN_STEP_ICON, step_card_detail, step_glyph};

use cards::inline_warning_card;
use step_drag::StepDragSession;

pub(in crate::automation::actions_screen) enum ActionEditorEvent {
    SelectAction(ActionId),
    ListRead {
        list_read: u64,
        listed: Result<Vec<Action>, RepoFailure>,
    },
    TriggerKindsRead {
        id: ActionId,
        kinds: Vec<String>,
    },
    RequestDelete(ActionId),
    EditAction(Action),
    OpenHistory,
    ExportPack(ActionId),
    Navigate(crate::app::navigation::Screen),
}

struct StepChoices {
    select_options: HashMap<String, Vec<ChoiceOption>>,
    select_options_failed: bool,
    builtins: BuiltinRegistry,
    integration_options: ChoiceOptions,
    option_choices_gen: async_bridge::Generation,
    option_keys: Vec<String>,
    _option_watch: Option<Task<()>>,
}

impl StepChoices {
    fn new() -> Self {
        Self {
            select_options: HashMap::new(),
            select_options_failed: false,
            builtins: BuiltinRegistry::default(),
            integration_options: ChoiceOptions::new(),
            option_choices_gen: async_bridge::Generation::default(),
            option_keys: Vec::new(),
            _option_watch: None,
        }
    }
}

#[derive(Default)]
struct EditorModals {
    test_run: Option<Entity<test_run::TestRunModal>>,
    _test_run_sub: Option<Subscription>,
    sub_form: Option<Entity<sub_action_modal::EditSubActionForm>>,
    _sub_form_sub: Option<Subscription>,
    grid_picker: Option<GridPickerForm>,
    add_trigger: Option<AddTriggerStage>,
}

pub(in crate::automation::actions_screen) struct ActionEditor {
    deps: ActionsDeps,
    list_reads: Arc<AtomicU64>,
    overlay_schema: Arc<overlay_schema::OverlayContentSchema>,
    pub(in crate::automation::actions_screen) sub_action_favorites: Entity<StoredFavorites>,
    pub(in crate::automation::actions_screen) trigger_favorites: Entity<StoredFavorites>,
    integrations: Option<SwitchWatch>,
    selected: Option<ActionId>,
    detail: Option<ActionDetail>,
    telemetry: Option<ActionTelemetry>,
    last_outcome: Option<ExecutionOutcome>,
    step_drag: Option<StepDragSession>,
    step_drag_focus: FocusHandle,
    step_drag_locked: bool,
    action_writes: HashMap<ActionId, action_writes::ActionWriteQueue>,
    nav_path: Vec<nav::NavFrame>,
    case_fields: BTreeMap<(usize, usize), CaseField>,
    step_health: Vec<forge_registry::StepHealth>,
    concurrent_queue_ids: HashSet<String>,
    header_open: Option<Point<Pixels>>,
    step_open: Option<usize>,
    step_menu_pos: Option<Point<Pixels>>,
    choices: StepChoices,
    modals: EditorModals,
}

impl EventEmitter<ActionEditorEvent> for ActionEditor {}

impl Render for ActionEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        self.render_editor_pane(&palette, cx)
    }
}

impl ActionEditor {
    pub(in crate::automation::actions_screen) fn new(
        deps: ActionsDeps,
        settings_repo: Arc<dyn SettingsRepo>,
        overlay_kinds: Arc<OverlayKindRegistry>,
        list_reads: Arc<AtomicU64>,
        selected: Option<ActionId>,
        cx: &mut Context<Self>,
    ) -> Self {
        let sub_action_favorites = StoredFavorites::shared(
            SUB_ACTION_FAVORITES,
            Arc::clone(&settings_repo),
            &deps.rt_handle,
            cx,
        );
        let trigger_favorites =
            StoredFavorites::shared(TRIGGER_FAVORITES, settings_repo, &deps.rt_handle, cx);
        let editor = Self {
            deps,
            list_reads,
            overlay_schema: Arc::new(overlay_schema::OverlayContentSchema::new(overlay_kinds)),
            sub_action_favorites,
            trigger_favorites,
            integrations: None,
            selected,
            detail: None,
            telemetry: None,
            last_outcome: None,
            step_drag: None,
            step_drag_focus: cx.focus_handle(),
            step_drag_locked: false,
            action_writes: HashMap::new(),
            nav_path: Vec::new(),
            case_fields: BTreeMap::new(),
            step_health: Vec::new(),
            concurrent_queue_ids: HashSet::new(),
            header_open: None,
            step_open: None,
            step_menu_pos: None,
            choices: StepChoices::new(),
            modals: EditorModals::default(),
        };
        editor.fetch_select_options(cx);
        editor
    }

    pub(in crate::automation::actions_screen) fn set_builtins(
        &mut self,
        builtins: BuiltinRegistry,
    ) {
        self.choices.builtins = builtins;
    }

    pub(in crate::automation::actions_screen) fn set_step_drag_locked(&mut self, locked: bool) {
        self.step_drag_locked = locked;
    }

    #[cfg(test)]
    pub(in crate::automation::actions_screen) fn show_detail(&mut self, detail: ActionDetail) {
        self.selected = Some(detail.action.id);
        self.detail = Some(detail);
    }

    #[cfg(test)]
    pub(in crate::automation::actions_screen) fn open_action_id(&self) -> Option<ActionId> {
        self.detail.as_ref().map(|detail| detail.action.id)
    }

    pub(in crate::automation::actions_screen) fn open_action_name(&self) -> Option<String> {
        self.detail.as_ref().map(|d| d.action.name.clone())
    }

    pub(in crate::automation::actions_screen) fn test_run_modal(
        &self,
    ) -> Option<Entity<test_run::TestRunModal>> {
        self.modals.test_run.clone()
    }

    pub(in crate::automation::actions_screen) fn has_open_modal(&self) -> bool {
        self.modals.sub_form.is_some()
            || self.modals.add_trigger.is_some()
            || self.modals.grid_picker.is_some()
            || self.modals.test_run.is_some()
    }

    pub(in crate::automation::actions_screen) fn save_open_form(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(form) = self.modals.sub_form.clone() {
            form.update(cx, |form, cx| form.submit(cx));
            return true;
        }
        if matches!(self.modals.add_trigger, Some(AddTriggerStage::Fill(_))) {
            self.submit_trigger_fill(cx);
            return true;
        }
        false
    }

    pub(in crate::automation::actions_screen) fn render_modal_layers(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let sub_form = self
            .modals
            .sub_form
            .clone()
            .map(IntoElement::into_any_element);
        let grid_picker = self.modals.grid_picker.as_ref().map(|form| {
            let view = cx.entity();
            overlay(form.picker.clone(), palette)
                .position(OverlayPosition::Center)
                .on_dismiss("actions-grid-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_grid_picker(cx));
                })
                .into_any_element()
        });
        let add_trigger = self
            .modals
            .add_trigger
            .as_ref()
            .map(|stage| self.render_add_trigger(stage, palette, cx));
        let drag_watch = self.step_drag_release_watch(cx);
        [sub_form, grid_picker, add_trigger, drag_watch]
            .into_iter()
            .flatten()
            .collect()
    }

    pub(super) fn current_chain(&self) -> Vec<nav::ChainItem> {
        match &self.detail {
            Some(detail) => nav::resolve_chain(&detail.action.sub_actions, &self.nav_path),
            None => Vec::new(),
        }
    }

    pub(super) fn render_editor_pane(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        match (self.selected, self.detail.as_ref()) {
            (Some(sel), Some(detail)) if detail.action.id == sel => {
                self.render_editor(detail, palette, cx)
            }
            (Some(_), _) => self.render_loading(palette, density, cx),
            (None, _) => self.render_empty(palette, density, cx),
        }
    }

    fn render_empty(&self, palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
        let placeholder = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::Bolt, EMPTY_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child(tr!("actions_detail_empty_title")),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_detail_empty_hint")),
            );

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .child(placeholder)
            .into_any_element()
    }

    fn render_loading(&self, palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .font_family(cx.body_family())
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(tr!("action_editor_loading"))
            .into_any_element()
    }

    pub(super) fn overlay_order_at_risk(&self, action: &Action) -> bool {
        (action.concurrent
            || self
                .concurrent_queue_ids
                .contains(&action.queue_id.to_string()))
            && analyzer::sends_order_sensitive_overlay(
                &action.sub_actions,
                &self.deps.sub_action_registry,
                &|identity| self.overlay_schema.is_order_sensitive(identity),
            )
    }

    fn render_order_warning(
        &self,
        action: &Action,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> Option<AnyElement> {
        self.overlay_order_at_risk(action).then(|| {
            inline_warning_card(
                tr!("action_editor_overlay_order_warning"),
                tr!("action_editor_overlay_order_warning_hint"),
                palette,
                density,
                cx,
            )
        })
    }

    fn render_editor(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(self.render_editor_header(detail, palette, cx))
            .children(self.render_order_warning(&detail.action, palette, density, cx))
            .child(self.render_triggers_section(detail, palette, cx))
            .child(self.render_sub_actions_section(detail, palette, cx));

        div()
            .id("actions-editor-scroll")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_revealing_focus()
            .py(scaled(PANE_PAD_V, density))
            .px(scaled(PANE_PAD_H, density))
            .child(body)
            .into_any_element()
    }
}

struct GridPickerForm {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, String>,
    action_id: ActionId,
    _sub: Subscription,
    _favorites: Subscription,
}

enum AddTriggerStage {
    Pick(AddTriggerPicker),
    Fill(TriggerFillHost),
}

struct TriggerFillHost {
    form: Entity<trigger_fill::TriggerFillModal>,
    _sub: Subscription,
}

struct AddTriggerPicker {
    picker: Entity<GridPicker>,
    picks_kind: HashMap<SharedString, String>,
    picks_instance: HashMap<SharedString, TriggerInstanceId>,
    action_id: ActionId,
    _sub: Subscription,
    _favorites: Subscription,
}

struct CaseField {
    field: Entity<TextInput>,
    cases_key: &'static str,
    seed: String,
    _sub: Subscription,
}

fn step_icon_btn(
    id: impl Into<ElementId>,
    glyph: Icon,
    disabled: bool,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let color = if disabled {
        palette.disabled
    } else {
        palette.text_faint
    };
    let base = div()
        .flex()
        .items_center()
        .justify_center()
        .size(STEP_BTN)
        .rounded(STEP_BTN_RADIUS)
        .child(icon(glyph, STEP_BTN_GLYPH, color));
    if disabled {
        return base.into_any_element();
    }
    let hover = palette.surface_overlay;
    base.id(id.into())
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .key_activated(palette.text_primary)
        .on_click(handler)
        .into_any_element()
}

#[cfg(test)]
pub(in crate::automation::actions_screen) mod tests {
    use forge_components::ThemeId;
    use forge_storage::MockGlobalsRepo;
    use forge_storage::MockQueueRepo;
    use forge_storage::MockScriptRepo;
    use forge_storage::MockSoundboardClipsRepo;
    use forge_storage::QueueRepo;
    use forge_storage::{
        DataProvider, MockMediaRepo, MockTriggerInstanceRepo, SoundboardClipsRepo, StoredClip,
    };

    use super::*;
    use crate::test_support::{
        ActionsRepos, actions_deps, actions_screen, test_backend_with_media,
    };
    use forge_components::Presentation;

    pub(in crate::automation::actions_screen) fn screen_parts(
        cx: &mut gpui::TestAppContext,
        screen: &Entity<ScreenActionsView>,
    ) -> (Entity<ActionEditor>, Entity<ActionTree>) {
        screen.read_with(cx, |screen, _| (screen.editor.clone(), screen.tree.clone()))
    }

    pub(in crate::automation::actions_screen) fn view_with(
        cx: &mut gpui::TestAppContext,
        rt: &tokio::runtime::Runtime,
        clips: Vec<StoredClip>,
        media: MockMediaRepo,
        sub_actions: SubActionRegistry,
        action_repo: Arc<dyn ActionRepo>,
    ) -> Entity<ScreenActionsView> {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
        });
        let mut queues = MockQueueRepo::new();
        queues.expect_list().returning(|| Ok(Vec::new()));
        let queue_repo: Arc<dyn QueueRepo> = Arc::new(queues);
        let mut triggers = MockTriggerInstanceRepo::new();
        triggers.expect_list_all().returning(|| Ok(Vec::new()));
        triggers
            .expect_list_for_action()
            .returning(|_| Ok(Vec::new()));
        triggers
            .expect_instances_by_action()
            .returning(|| Ok(Default::default()));
        triggers
            .expect_actions_by_instance()
            .returning(|| Ok(Default::default()));
        let trigger_repo: Arc<dyn TriggerInstanceRepo> = Arc::new(triggers);
        let mut scripts = MockScriptRepo::new();
        scripts.expect_list().returning(|| Ok(Vec::new()));
        let mut globals = MockGlobalsRepo::new();
        globals.expect_list().returning(|| Ok(Vec::new()));
        let mut clip_repo = MockSoundboardClipsRepo::new();
        clip_repo.expect_list().returning(move || Ok(clips.clone()));
        let clip_repo: Arc<dyn SoundboardClipsRepo> = Arc::new(clip_repo);
        let (backend, _writes) = test_backend_with_media(Arc::new(media));
        let deps = actions_deps(
            rt,
            ActionsRepos {
                actions: action_repo,
                queues: queue_repo,
                triggers: trigger_repo,
                scripts: Arc::new(scripts),
                clips: clip_repo,
                media: backend.media_repo(),
                globals: Arc::new(globals),
                sub_actions,
                trigger_kinds: TriggerRegistry::new(),
            },
        );
        cx.update(|cx| cx.new(|cx| actions_screen(deps, backend.settings_repo(), cx)))
    }
}
