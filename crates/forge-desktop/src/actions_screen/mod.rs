use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;
use forge_components::{
    ForgePalette, GridPicker, Icon, OverlayPosition, TextArea, TextInput, ToastKind, icon, overlay,
    search_input, tr,
};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::EventBus;
use forge_runtime::actions::{ActionDetail, ActionsService};
use forge_storage::{ActionRepo, QueueRepo};
use forge_types::{Action, ActionId, QueueId, SubActionStep, TriggerInstanceId};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;

mod branch;
mod editor;
mod list;
mod nav;
mod test_trigger;

const LEFT_PANEL_W: Pixels = px(290.0);
const ROW_HEIGHT: Pixels = px(30.0);
const RIGHT_SLOT_W: Pixels = px(46.0);
const ROW_INDENT: Pixels = px(32.0);
const ROW_GUTTER: Pixels = px(14.0);
const STRIPE_W: Pixels = px(2.0);
const TREE_GUTTER: Pixels = px(14.0);
const TREE_GLYPH: Pixels = px(11.0);
const SEARCH_W: Pixels = px(180.0);
const HEADER_DIV_W: Pixels = px(0.5);
const HEADER_DIV_H: Pixels = px(16.0);
const GROUP_DOT: Pixels = px(8.0);
const EMPTY_GLYPH: Pixels = px(28.0);
const NAME_LIMIT: usize = 64;

const PILL_RADIUS: Pixels = px(8.0);
const PILL_DOT: Pixels = px(5.0);
const CARD_GLYPH: Pixels = px(13.0);
const TRIGGER_DOT: Pixels = px(7.0);
const TRIGGER_GLYPH: Pixels = px(13.0);
const UNLINK_GLYPH: Pixels = px(13.0);
const STEP_CIRCLE: Pixels = px(22.0);
const STEP_CIRCLE_RADIUS: Pixels = px(11.0);
const STEP_COL_W: Pixels = px(24.0);
const STEP_CONNECTOR_W: Pixels = px(2.0);
const STEP_CONNECTOR_H: Pixels = px(14.0);
const STEP_BTN: Pixels = px(22.0);
const STEP_BTN_GLYPH: Pixels = px(12.0);
const STEP_BTN_RADIUS: Pixels = px(4.0);
const CARD_PAD_V: Pixels = px(9.0);
const CARD_PAD_H: Pixels = px(12.0);
const EMPTY_CARD_PAD_V: Pixels = px(18.0);
const EMPTY_CARD_PAD_H: Pixels = px(12.0);
const EMPTY_CARD_GLYPH: Pixels = px(16.0);
const HALF_BORDER: Pixels = px(0.5);
const PANE_PAD_V: Pixels = px(18.0);
const PANE_PAD_H: Pixels = px(22.0);
const STEP_GAP: Pixels = px(6.0);
const SUB_SHEET_W: Pixels = px(480.0);

const CHIP_RADIUS: Pixels = px(6.0);
const BRANCH_GLYPH: Pixels = px(11.0);
const CASE_MATCH_W: Pixels = px(160.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionCategory {
    Chat,
    Timers,
    Points,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionsFilter {
    All,
    Chat,
    Timers,
    Points,
}

struct ActionSummary {
    id: ActionId,
    name: String,
    enabled: bool,
    sub_action_count: usize,
}

struct ActionGroup {
    name: SharedString,
    category: ActionCategory,
    collapsed: bool,
    actions: Vec<ActionSummary>,
}

struct Renaming {
    id: ActionId,
    field: Entity<TextInput>,
    _sub: Subscription,
}

struct AddActionForm {
    name: Entity<TextInput>,
    group: Entity<TextInput>,
    description: Entity<TextArea>,
    queues: Vec<(QueueId, SharedString)>,
    selected_queue: usize,
    enabled: bool,
    concurrent: bool,
    bypass_pause: bool,
    random_pick: bool,
    _name_sub: Subscription,
}

pub struct ScreenActionsView {
    action_repo: Arc<dyn ActionRepo>,
    queue_repo: Arc<dyn QueueRepo>,
    actions_service: Arc<ActionsService>,
    sub_action_registry: Arc<SubActionRegistry>,
    trigger_registry: Arc<TriggerRegistry>,
    rt_handle: tokio::runtime::Handle,
    bus: Arc<EventBus>,
    loading: bool,
    groups: Vec<ActionGroup>,
    filter: ActionsFilter,
    search: String,
    search_field: Entity<TextInput>,
    selected: Option<ActionId>,
    hovered: Option<ActionId>,
    menu_open: Option<ActionId>,
    renaming: Option<Renaming>,
    add_modal: Option<AddActionForm>,
    pending_delete: Option<ActionId>,
    detail: Option<ActionDetail>,
    sub_form: Option<EditSubActionForm>,
    step_menu_open: Option<usize>,
    grid_picker: Option<GridPickerForm>,
    add_trigger: Option<AddTriggerForm>,
    nav_path: Vec<nav::NavFrame>,
    /// Keyed by `(step_index, case_index)` within the current chain.
    case_fields: BTreeMap<(usize, usize), CaseField>,
    _search_sub: Subscription,
}

impl ScreenActionsView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        action_repo: Arc<dyn ActionRepo>,
        queue_repo: Arc<dyn QueueRepo>,
        actions_service: Arc<ActionsService>,
        sub_action_registry: Arc<SubActionRegistry>,
        trigger_registry: Arc<TriggerRegistry>,
        rt_handle: tokio::runtime::Handle,
        bus: Arc<EventBus>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search_field =
            cx.new(|cx| search_input(tr!("actions_search_placeholder"), palette, cx));
        let search_sub = cx.subscribe(&search_field, Self::on_search_event);

        let view = Self {
            action_repo,
            queue_repo,
            actions_service,
            sub_action_registry,
            trigger_registry,
            rt_handle,
            bus,
            loading: true,
            groups: Vec::new(),
            filter: ActionsFilter::All,
            search: String::new(),
            search_field,
            selected: None,
            hovered: None,
            menu_open: None,
            renaming: None,
            add_modal: None,
            pending_delete: None,
            detail: None,
            sub_form: None,
            step_menu_open: None,
            grid_picker: None,
            add_trigger: None,
            nav_path: Vec::new(),
            case_fields: BTreeMap::new(),
            _search_sub: search_sub,
        };
        view.reload(cx);
        view
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.action_repo);
        self.spawn_reload(
            async move { repo.list().await.map_err(|e| e.to_string()) },
            cx,
        );
    }

    fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<Action>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        Self::reload_entity(cx.entity(), self.rt_handle.clone(), work, cx);
    }

    fn reload_entity(
        view: Entity<ScreenActionsView>,
        rt_handle: tokio::runtime::Handle,
        work: impl Future<Output = Result<Vec<Action>, String>> + Send + 'static,
        app: &mut App,
    ) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        rt_handle.spawn(async move {
            let _ = tx.send(work.await);
        });
        app.spawn(async move |cx| match rx.await {
            Ok(Ok(actions)) => {
                view.update(cx, |this, cx| this.apply_actions(actions, cx));
            }
            Ok(Err(message)) => {
                view.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn apply_actions(&mut self, actions: Vec<Action>, cx: &mut Context<Self>) {
        let collapsed: HashSet<SharedString> = self
            .groups
            .iter()
            .filter(|g| g.collapsed)
            .map(|g| g.name.clone())
            .collect();
        let mut groups = group_actions(actions);
        for group in &mut groups {
            if collapsed.contains(&group.name) {
                group.collapsed = true;
            }
        }
        self.groups = groups;

        if let Some(selected) = self.selected {
            if self.find(selected).is_some() {
                self.reload_detail(cx);
            } else {
                self.selected = None;
                self.detail = None;
                self.nav_path.clear();
                self.case_fields.clear();
            }
        }
        self.loading = false;
        cx.notify();
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: actions operation failed: {message}");
        self.loading = false;
        cx.push_toast(
            ToastKind::Error,
            tr!("actions_toast_error", message = message),
        );
        cx.notify();
    }

    fn reload_detail(&self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected {
            self.load_detail_for(id, cx);
        }
    }

    pub(super) fn load_detail_for(&self, id: ActionId, cx: &mut Context<Self>) {
        let service = Arc::clone(&self.actions_service);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(service.load_detail(id).await.map_err(|e| e.to_string()));
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(detail)) => {
                let _ = this.update(cx, |this, cx| this.apply_detail(id, detail, cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn apply_detail(&mut self, id: ActionId, detail: ActionDetail, cx: &mut Context<Self>) {
        if self.selected != Some(id) {
            return;
        }
        self.detail = Some(detail);
        self.sync_case_fields(cx);
        cx.notify();
    }

    pub(super) fn persist_action(&self, action: Action, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.action_repo);
        self.spawn_reload(
            async move {
                repo.save(&action).await.map_err(|e| e.to_string())?;
                repo.list().await.map_err(|e| e.to_string())
            },
            cx,
        );
    }

    pub(super) fn persist_chain_mutation(
        &mut self,
        mutate: impl FnOnce(&mut Vec<SubActionStep>),
        cx: &mut Context<Self>,
    ) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let mut action = detail.action.clone();
        let path = self.nav_path.clone();
        let mut chain = nav::resolve_chain(&action.sub_actions, &path);
        mutate(&mut chain);
        if !nav::set_chain(&mut action.sub_actions, &path, &chain) {
            return;
        }
        self.persist_action(action, cx);
    }
}

impl Render for ScreenActionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let header = self.render_header(&palette, cx);
        let tree = self.render_tree(&palette, cx);
        let editor = self.render_editor_pane(&palette, cx);

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(tree)
            .child(editor);

        let add_modal = self
            .add_modal
            .as_ref()
            .map(|form| self.render_add_modal(form, &palette, cx));
        let delete_modal = self
            .pending_delete
            .map(|id| self.render_delete_confirm(id, &palette, cx));
        let sub_modal = self
            .sub_form
            .as_ref()
            .map(|form| self.render_sub_action_modal(form, &palette, cx));
        let grid_picker = self.grid_picker.as_ref().map(|form| {
            let view = cx.entity();
            overlay(form.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("actions-grid-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_grid_picker(cx));
                })
                .into_any_element()
        });
        let trigger_grid = self.add_trigger.as_ref().map(|form| {
            let view = cx.entity();
            overlay(form.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("actions-trigger-grid-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_trigger_picker(cx));
                })
                .into_any_element()
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(body)
            .children(add_modal)
            .children(delete_modal)
            .children(sub_modal)
            .children(grid_picker)
            .children(trigger_grid)
    }
}

fn group_actions(actions: Vec<Action>) -> Vec<ActionGroup> {
    let mut by_name: BTreeMap<String, Vec<ActionSummary>> = BTreeMap::new();
    for action in actions {
        let group_name = action
            .group
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_uppercase)
            .unwrap_or_else(|| "UNGROUPED".to_owned());
        let summary = ActionSummary {
            id: action.id,
            name: action.name,
            enabled: action.enabled,
            sub_action_count: action.sub_actions.len(),
        };
        by_name.entry(group_name).or_default().push(summary);
    }
    by_name
        .into_iter()
        .map(|(name, mut actions)| {
            actions.sort_by_key(|a| a.name.to_lowercase());
            let category = category_from_group_name(&name);
            ActionGroup {
                name: name.into(),
                category,
                collapsed: false,
                actions,
            }
        })
        .collect()
}

fn category_from_group_name(name: &str) -> ActionCategory {
    match name {
        "CHAT COMMANDS" => ActionCategory::Chat,
        "TIMERS" => ActionCategory::Timers,
        "CHANNEL POINTS" => ActionCategory::Points,
        _ => ActionCategory::Other,
    }
}

struct GridPickerForm {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, String>,
    action_id: ActionId,
    _sub: Subscription,
}

struct AddTriggerForm {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, TriggerInstanceId>,
    action_id: ActionId,
    _sub: Subscription,
}

struct EditSubActionForm {
    kind_id: String,
    index: usize,
    fields: Vec<SubFormField>,
}

enum SubFormField {
    Input {
        key: String,
        label: String,
        integer: bool,
        gate: Option<String>,
        input: Entity<TextInput>,
    },
    Bool {
        key: String,
        label: String,
        gate: Option<String>,
        value: bool,
    },
    Hint {
        label: String,
    },
}

struct CaseField {
    field: Entity<TextInput>,
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
        .on_click(handler)
        .into_any_element()
}
