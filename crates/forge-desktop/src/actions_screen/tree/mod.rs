use super::*;
use crate::load_notice::{LoadNotice, NoticeWidth, load_notice};
use crate::navigable_list::{ListCursor, ListStep, NavigableList, navigable_list};
use forge_components::{
    Confirm, FONT_XS, InlineEditEvent, InputEvent, Spacing, body_family, inline_edit, spacing,
};
use forge_storage::ScheduledRunRepo;
use gpui::ScrollHandle;
use std::sync::atomic::AtomicU64;

mod delete_confirm_count;
mod render;
#[cfg(test)]
mod tests;

const LEFT_PANEL_W: Pixels = px(290.0);
const LEFT_PANEL_MIN: Pixels = px(220.0);
const LEFT_PANEL_MAX: Pixels = px(480.0);
const ROW_HEIGHT: Pixels = px(30.0);
const RIGHT_SLOT_W: Pixels = px(46.0);
const ROW_INDENT: Pixels = px(32.0);
const ROW_GUTTER: Pixels = px(14.0);
const STRIPE_W: Pixels = px(2.0);
const TREE_GUTTER: Pixels = px(14.0);
const TREE_GLYPH: Pixels = px(11.0);
const HEADER_DIV_H: Pixels = px(16.0);

const TRIGGER_FILTERS: [(ActionsFilter, TriggerCategory); 3] = [
    (ActionsFilter::Chat, TriggerCategory::Chat),
    (ActionsFilter::Timers, TriggerCategory::Timer),
    (ActionsFilter::Points, TriggerCategory::ChannelPoints),
];

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum ActionsFilter {
    All,
    Chat,
    Timers,
    Points,
}

struct ActionsTreeResizeDrag;

pub(super) struct ActionSummary {
    id: ActionId,
    pub(in crate::actions_screen) name: String,
    enabled: bool,
    sub_action_count: usize,
}

struct ActionGroup {
    name: SharedString,
    collapsed: bool,
    actions: Vec<ActionSummary>,
}

struct Renaming {
    id: ActionId,
    editor: Entity<InlineEdit>,
    _sub: Subscription,
}

pub(super) struct ActionTreeDeps {
    pub(super) action_repo: Arc<dyn ActionRepo>,
    pub(super) trigger_instance_repo: Arc<dyn TriggerInstanceRepo>,
    pub(super) trigger_registry: Arc<TriggerRegistry>,
    pub(super) rt_handle: tokio::runtime::Handle,
}

pub(super) enum ActionTreeEvent {
    Selected(ActionId),
    SelectedListed,
    SelectionLost,
    Write(ActionId, ActionWrite),
    DeletePromptChanged,
}

pub(super) struct ActionTree {
    deps: ActionTreeDeps,
    scheduled_runs: Option<Arc<dyn ScheduledRunRepo>>,
    width: Pixels,
    keys: ListCursor<ActionId>,
    scroll: ScrollHandle,
    list_load: Loadable<()>,
    list_reads: Arc<AtomicU64>,
    landed_list_read: u64,
    groups: Vec<ActionGroup>,
    filter_action_ids: HashMap<ActionsFilter, HashSet<ActionId>>,
    filter: ActionsFilter,
    search: SearchState,
    _search_sub: Subscription,
    selected: Option<ActionId>,
    hovered: Option<ActionId>,
    renaming: Option<Renaming>,
    pending_delete: Confirm<ActionId>,
    delete_scheduled_count: Option<(ActionId, u64)>,
    row_open: Option<ActionId>,
    menu_pos: Option<Point<Pixels>>,
    menu_focus: FocusHandle,
    menu_focus_restore: Option<FocusHandle>,
}

impl EventEmitter<ActionTreeEvent> for ActionTree {}

pub(super) fn next_list_read(list_reads: &AtomicU64) -> u64 {
    list_reads.fetch_add(1, Ordering::Relaxed) + 1
}

impl ActionTree {
    pub(super) fn new(
        deps: ActionTreeDeps,
        selected: Option<ActionId>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("actions_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let tree = Self {
            deps,
            scheduled_runs: None,
            width: LEFT_PANEL_W,
            keys: ListCursor::new(cx),
            scroll: ScrollHandle::new(),
            list_load: Loadable::Loading,
            list_reads: Arc::new(AtomicU64::new(0)),
            landed_list_read: 0,
            groups: Vec::new(),
            filter_action_ids: HashMap::new(),
            filter: ActionsFilter::All,
            search,
            _search_sub: search_sub,
            selected,
            hovered: None,
            renaming: None,
            pending_delete: Confirm::default(),
            delete_scheduled_count: None,
            row_open: None,
            menu_pos: None,
            menu_focus: cx.focus_handle(),
            menu_focus_restore: None,
        };
        tree.reload(cx);
        tree
    }

    pub(in crate::actions_screen) fn set_scheduled_runs(
        &mut self,
        repo: Arc<dyn ScheduledRunRepo>,
    ) {
        self.scheduled_runs = Some(repo);
    }

    pub(super) fn list_reads(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.list_reads)
    }

    #[cfg(test)]
    pub(in crate::actions_screen) fn set_selected(&mut self, id: Option<ActionId>) {
        self.selected = id;
    }

    #[cfg(test)]
    pub(in crate::actions_screen) fn ask_delete(&mut self, id: ActionId) {
        self.pending_delete.request(id);
    }

    pub(super) fn selected(&self) -> Option<ActionId> {
        self.selected
    }

    pub(super) fn action_names(&self) -> HashMap<ActionId, SharedString> {
        self.groups
            .iter()
            .flat_map(|group| group.actions.iter())
            .map(|action| (action.id, SharedString::from(action.name.clone())))
            .collect()
    }

    pub(super) fn delete_pending(&self) -> bool {
        self.pending_delete.get().is_some()
    }

    fn set_tree_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.width != width {
            self.width = width;
            cx.notify();
        }
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.deps.action_repo);
        self.spawn_reload(
            async move { repo.list().await.map_err(RepoFailure::from) },
            cx,
        );
    }

    pub(in crate::actions_screen) fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<Action>, RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let list_read = next_list_read(&self.list_reads);
        async_bridge::run_async(
            &self.deps.rt_handle,
            work,
            move |this, result, cx| this.land_list(list_read, result, cx),
            cx,
        );
    }

    pub(in crate::actions_screen) fn land_list(
        &mut self,
        list_read: u64,
        result: Result<Vec<Action>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        if list_read <= self.landed_list_read {
            return;
        }
        self.landed_list_read = list_read;
        match result {
            Ok(actions) => self.apply_actions(actions, cx),
            Err(failure) => {
                self.on_list_load_failed(failure, cx);
                if self.selected.is_some() {
                    cx.emit(ActionTreeEvent::SelectedListed);
                }
            }
        }
    }

    fn on_list_load_failed(&mut self, failure: RepoFailure, cx: &mut Context<Self>) {
        if self.total_actions() > 0 {
            on_repo_error(&failure, cx);
        }
        self.list_load = Loadable::Failed(failure);
        cx.notify();
    }

    fn retry_list_load(&mut self, cx: &mut Context<Self>) {
        self.list_load = Loadable::Loading;
        self.reload(cx);
        cx.notify();
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
        self.reload_filter_actions(cx);

        if let Some(selected) = self.selected {
            if self.find(selected).is_some() {
                cx.emit(ActionTreeEvent::SelectedListed);
            } else {
                self.selected = None;
                cx.emit(ActionTreeEvent::SelectionLost);
            }
        }
        self.list_load = Loadable::Loaded(());
        cx.notify();
    }

    fn filter_by_kind_id(&self) -> HashMap<String, ActionsFilter> {
        self.deps
            .trigger_registry
            .all()
            .filter_map(|d| {
                let filter = TRIGGER_FILTERS
                    .iter()
                    .find(|(_, category)| *category == d.category())
                    .map(|(filter, _)| *filter)?;
                Some((d.id().to_owned(), filter))
            })
            .collect()
    }

    fn reload_filter_actions(&self, cx: &mut Context<Self>) {
        let filter_by_kind = self.filter_by_kind_id();
        let repo = Arc::clone(&self.deps.trigger_instance_repo);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                actions_by_trigger_filter(&*repo, &filter_by_kind)
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| match result {
                Ok(ids) => this.apply_filter_actions(ids, cx),
                Err(failure) => on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn apply_filter_actions(
        &mut self,
        ids: HashMap<ActionsFilter, HashSet<ActionId>>,
        cx: &mut Context<Self>,
    ) {
        self.filter_action_ids = ids;
        cx.notify();
    }

    pub(in crate::actions_screen) fn sync_filter_membership(
        &mut self,
        id: ActionId,
        trigger_kinds: &[String],
    ) {
        let filter_by_kind = self.filter_by_kind_id();
        for (filter, _) in TRIGGER_FILTERS {
            let has_trigger = trigger_kinds
                .iter()
                .any(|kind| filter_by_kind.get(kind) == Some(&filter));
            let members = self.filter_action_ids.entry(filter).or_default();
            if has_trigger {
                members.insert(id);
            } else {
                members.remove(&id);
            }
        }
    }
}

impl NavigableList for ActionTree {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        if self.renaming.is_some() {
            return;
        }
        let rows = self.tree_action_rows();
        let ids: Vec<ActionId> = rows.iter().map(|row| row.id).collect();
        if let Some(next) = self.keys.step(&ids, self.selected.as_ref(), step) {
            if let Some(row) = rows.get(next) {
                self.scroll.scroll_to_item(row.child_index);
            }
            cx.notify();
        }
    }

    fn activate_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming.is_some() {
            return;
        }
        let ids: Vec<ActionId> = self.tree_action_rows().iter().map(|row| row.id).collect();
        if let Some(id) = self.keys.activation_target(&ids) {
            self.select(id, cx);
        }
    }
}

impl ActionTree {
    pub(in crate::actions_screen) fn find(&self, id: ActionId) -> Option<&ActionSummary> {
        self.groups
            .iter()
            .flat_map(|g| g.actions.iter())
            .find(|a| a.id == id)
    }

    pub(super) fn total_actions(&self) -> usize {
        self.groups.iter().map(|g| g.actions.len()).sum()
    }

    fn enabled_count(&self) -> usize {
        self.groups
            .iter()
            .flat_map(|g| g.actions.iter())
            .filter(|a| a.enabled)
            .count()
    }

    fn action_passes(&self, action: &ActionSummary) -> bool {
        let in_filter = self.filter == ActionsFilter::All
            || self
                .filter_action_ids
                .get(&self.filter)
                .is_some_and(|ids| ids.contains(&action.id));
        in_filter && self.search.matches(&action.name)
    }

    fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            cx.notify();
        }
    }

    fn set_filter(&mut self, filter: ActionsFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        cx.notify();
    }

    fn toggle_group(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(group) = self.groups.get_mut(index) {
            group.collapsed = !group.collapsed;
            cx.notify();
        }
    }

    pub(in crate::actions_screen) fn select(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.keys.move_to(id);
        cx.emit(ActionTreeEvent::Selected(id));
        cx.notify();
    }

    fn set_hover(&mut self, id: ActionId, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.hovered != Some(id) {
                self.hovered = Some(id);
                cx.notify();
            }
        } else if self.hovered == Some(id) {
            self.hovered = None;
            cx.notify();
        }
    }

    fn open_row_menu(&mut self, id: ActionId, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.row_open = Some(id);
        self.menu_pos = Some(position);
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.row_open = None;
        cx.notify();
    }

    pub(super) fn set_enabled(&mut self, id: ActionId, enabled: bool, cx: &mut Context<Self>) {
        self.row_open = None;
        cx.emit(ActionTreeEvent::Write(id, ActionWrite::SetEnabled(enabled)));
        cx.notify();
    }

    pub(super) fn duplicate(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.row_open = None;
        cx.emit(ActionTreeEvent::Write(id, ActionWrite::duplicate_copy()));
        cx.notify();
    }

    fn start_rename(&mut self, id: ActionId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self.find(id) else {
            return;
        };
        let palette = cx.palette();
        let seed = action.name.clone();
        let editor = inline_edit(seed, palette, FONT_XS, window, cx);
        let sub = cx.subscribe(
            &editor,
            |this, _e, event: &InlineEditEvent, cx| match event {
                InlineEditEvent::Commit(next) => this.commit_rename(next.clone(), cx),
                InlineEditEvent::Cancel => this.cancel_rename(cx),
            },
        );
        self.row_open = None;
        self.renaming = Some(Renaming {
            id,
            editor,
            _sub: sub,
        });
        cx.notify();
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }

    fn commit_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(renaming) = self.renaming.take() else {
            cx.notify();
            return;
        };
        let trimmed = name.trim().to_owned();
        cx.notify();
        if trimmed.is_empty() {
            return;
        }
        let id = renaming.id;
        let taken = self
            .groups
            .iter()
            .flat_map(|g| g.actions.iter())
            .any(|a| a.id != id && a.name.to_lowercase() == trimmed.to_lowercase());
        if taken {
            cx.push_toast(
                ToastKind::Error,
                tr!("actions_rename_taken", name = trimmed.as_str()),
            );
            return;
        }
        cx.emit(ActionTreeEvent::Write(id, ActionWrite::Rename(trimmed)));
    }

    pub(in crate::actions_screen) fn request_delete(
        &mut self,
        id: ActionId,
        cx: &mut Context<Self>,
    ) {
        self.pending_delete.request(id);
        self.row_open = None;
        self.load_delete_scheduled_count(id, cx);
        cx.emit(ActionTreeEvent::DeletePromptChanged);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        self.delete_scheduled_count = None;
        cx.emit(ActionTreeEvent::DeletePromptChanged);
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_delete.take() else {
            return;
        };
        let name = self.find(id).map(|a| a.name.clone()).unwrap_or_default();
        self.delete_scheduled_count = None;
        cx.emit(ActionTreeEvent::Write(id, ActionWrite::Archive { name }));
        cx.emit(ActionTreeEvent::DeletePromptChanged);
        cx.notify();
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
            .unwrap_or_else(|| tr!("actions_group_ungrouped").to_uppercase());
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
            ActionGroup {
                name: name.into(),
                collapsed: false,
                actions,
            }
        })
        .collect()
}

async fn actions_by_trigger_filter(
    repo: &dyn TriggerInstanceRepo,
    filter_by_kind: &HashMap<String, ActionsFilter>,
) -> Result<HashMap<ActionsFilter, HashSet<ActionId>>, StorageError> {
    let mut links = repo.actions_by_instance().await?;
    let mut members: HashMap<ActionsFilter, HashSet<ActionId>> = HashMap::new();
    for instance in repo.list_all().await? {
        if let (Some(filter), Some(actions)) = (
            filter_by_kind.get(&instance.kind_id),
            links.remove(&instance.id),
        ) {
            members.entry(*filter).or_default().extend(actions);
        }
    }
    Ok(members)
}
