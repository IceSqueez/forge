use super::*;
use crate::app::async_bridge;
use crate::app::navigable_list::{ListStep, NavigableList, navigable_list, stepped_index};
use crate::integrations::integration_switch::{category_inactive_badge, inactive_badge};
use crate::shared::load_notice::{LoadNotice, NoticeWidth, load_notice};
use forge_components::OverflowRevealingFocus;
use forge_components::ROW_OFF_OPACITY;
use forge_components::SEARCH_FIELD_WIDTH;
use forge_components::{
    BORDER_THIN, ChipGlyph, ConfirmTone, Density, FONT_XXS, ForgePalette, Icon, InlineEditEvent,
    InputEvent, KeyActivated, OverlayPosition, Spacing, TextInput, badge, chip, confirm_modal,
    context_menu, ghost_button_with_icon, header_stat, header_stats, icon, inline_edit,
    menu_button, menu_divider, menu_item, overlay, primary_button_with_icon, scaled, spacing,
    status_dot, toggle, tr,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent,
    Pixels, Point, Rgba, ScrollStrategy, SharedString, Window, div, px, uniform_list,
};
use std::sync::Arc;

mod modal_layers;
mod render;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct ListedRow {
    pub(super) id: TriggerInstanceId,
    pub(super) name: String,
    pub(super) enabled: bool,
    pub(super) used_in_count: usize,
}

pub(super) enum TriggerListEvent {
    Selected(ListedRow),
    SelectedListed(ListedRow),
    SelectionLost,
    EnabledShown(TriggerInstanceId, bool),
    OpenCreate,
}

impl EventEmitter<TriggerListEvent> for TriggerList {}

pub(super) struct TriggerList {
    deps: TriggerScreenDeps,
    list_load: Loadable<()>,
    reload_generation: async_bridge::Generation,
    enabled_writers: KeyedSnapshotWriters<TriggerInstanceId>,
    instances: Vec<TriggerInstanceRow>,
    visible: Vec<usize>,
    list_scroll: UniformListScrollHandle,
    list_focus: FocusHandle,
    focused_once: bool,
    pub(super) list_cursor: Option<TriggerInstanceId>,
    pub(super) selected: Option<TriggerInstanceId>,
    hovered: Option<TriggerInstanceId>,
    menu_open: Option<TriggerInstanceId>,
    menu_focus: FocusHandle,
    menu_focus_restore: Option<FocusHandle>,
    menu_click_pos: Option<Point<Pixels>>,
    search: SearchState,
    platforms: Vec<Platform>,
    usage_filter: UsageFilter,
    rename: Option<RenameForm>,
    rename_requested: Option<TriggerInstanceId>,
    pending_delete: Confirm<TriggerInstanceId>,
    confirm_disable: Confirm<TriggerInstanceId>,
    integrations: Option<SwitchWatch>,
    _search_sub: Subscription,
}

impl TriggerList {
    pub(super) fn new(
        deps: TriggerScreenDeps,
        preselect: Option<TriggerInstanceId>,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = SearchState::new(cx, tr!("triggers_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let enabled_writers = KeyedSnapshotWriters::new(&deps.rt_handle);
        let list = Self {
            deps,
            list_load: Loadable::Loading,
            reload_generation: async_bridge::Generation::default(),
            enabled_writers,
            instances: Vec::new(),
            visible: Vec::new(),
            list_scroll: UniformListScrollHandle::new(),
            list_focus: cx.focus_handle(),
            focused_once: false,
            list_cursor: preselect,
            selected: preselect,
            hovered: None,
            menu_open: None,
            menu_focus: cx.focus_handle(),
            menu_focus_restore: None,
            menu_click_pos: None,
            search,
            platforms: Vec::new(),
            usage_filter: UsageFilter::All,
            rename: None,
            rename_requested: None,
            pending_delete: Confirm::default(),
            confirm_disable: Confirm::default(),
            integrations: None,
            _search_sub: search_sub,
        };
        list.reload(cx);
        list
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

    pub(super) fn delete_pending(&self) -> bool {
        self.pending_delete.get().is_some()
    }

    pub(super) fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.deps.repo);
        self.spawn_reload(async move { load_rows(&*repo).await }, cx);
    }

    fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<TriggerInstanceRow>, RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let ticket = self.reload_generation.next();
        async_bridge::run_async(
            &self.deps.rt_handle,
            work,
            move |this, result, cx| match result {
                _ if !this.reload_generation.is_current(ticket) => {}
                Ok(rows) => this.apply_rows(rows, cx),
                Err(failure) => this.on_list_load_failed(failure, cx),
            },
            cx,
        );
    }

    fn on_list_load_failed(&mut self, failure: RepoFailure, cx: &mut Context<Self>) {
        if !self.instances.is_empty() {
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

    fn spawn_write<T: Send + 'static>(
        &self,
        write: impl Future<Output = Result<T, RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.deps.rt_handle,
            write,
            |this, written, cx| this.on_write_settled(written.map(drop), cx),
            cx,
        );
    }

    fn on_write_settled(&mut self, written: Result<(), RepoFailure>, cx: &mut Context<Self>) {
        match written {
            Ok(()) => self.reload(cx),
            Err(failure) => {
                on_repo_error(&failure, cx);
                self.reload(cx);
            }
        }
    }

    fn listed_row(&self, id: TriggerInstanceId) -> Option<ListedRow> {
        self.find(id).map(|row| ListedRow {
            id,
            name: row.name.clone(),
            enabled: row.enabled,
            used_in_count: row.used_in_count,
        })
    }

    fn apply_rows(&mut self, rows: Vec<TriggerInstanceRow>, cx: &mut Context<Self>) {
        self.instances = rows;
        self.rebuild_visible();
        self.list_load = Loadable::Loaded(());
        match self.selected.map(|id| self.listed_row(id)) {
            Some(Some(row)) => cx.emit(TriggerListEvent::SelectedListed(row)),
            Some(None) => {
                self.selected = None;
                cx.emit(TriggerListEvent::SelectionLost);
            }
            None => {}
        }
        cx.notify();
    }

    pub(super) fn land_created(
        &mut self,
        id: TriggerInstanceId,
        rows: Vec<TriggerInstanceRow>,
        cx: &mut Context<Self>,
    ) {
        self.instances = rows;
        self.rebuild_visible();
        self.list_load = Loadable::Loaded(());
        self.select(id, cx);
    }

    pub(super) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        cx.notify();
    }

    pub(super) fn request_rename(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.rename_requested = Some(id);
        cx.notify();
    }

    fn visible_position(&self, id: TriggerInstanceId) -> Option<usize> {
        self.visible
            .iter()
            .position(|&idx| self.instances.get(idx).map(|r| r.id) == Some(id))
    }

    pub(super) fn find(&self, id: TriggerInstanceId) -> Option<&TriggerInstanceRow> {
        self.instances.iter().find(|i| i.id == id)
    }

    fn used_count(&self) -> usize {
        self.instances
            .iter()
            .filter(|i| i.used_in_count > 0)
            .count()
    }

    fn disabled_count(&self) -> usize {
        self.instances.iter().filter(|i| !i.enabled).count()
    }

    fn platform_counts(&self) -> Vec<(Platform, usize)> {
        Platform::ordered()
            .into_iter()
            .filter_map(|p| {
                let count = self
                    .instances
                    .iter()
                    .filter(|i| {
                        Platform::from_kind_id(&i.kind_id, &self.deps.registry).as_ref() == Some(&p)
                    })
                    .count();
                (count > 0).then_some((p, count))
            })
            .collect()
    }

    fn has_active_filter(&self) -> bool {
        !self.search.is_empty()
            || !self.platforms.is_empty()
            || self.usage_filter != UsageFilter::All
    }

    fn kind_label(&self, kind_id: &str) -> String {
        self.deps
            .registry
            .get(kind_id)
            .map(crate::shared::kind_copy::trigger_name)
            .unwrap_or_else(|| kind_id.to_owned())
    }

    pub(super) fn rebuild_visible(&mut self) {
        let visible: Vec<usize> = self
            .instances
            .iter()
            .enumerate()
            .filter(|(_, i)| self.passes(i))
            .map(|(index, _)| index)
            .collect();
        self.visible = visible;
    }

    fn passes(&self, instance: &TriggerInstanceRow) -> bool {
        if !self.platforms.is_empty()
            && !Platform::from_kind_id(&instance.kind_id, &self.deps.registry)
                .is_some_and(|p| self.platforms.contains(&p))
        {
            return false;
        }
        match self.usage_filter {
            UsageFilter::Used if instance.used_in_count == 0 => return false,
            UsageFilter::Unused if instance.used_in_count > 0 => return false,
            _ => {}
        }
        if self.search.is_empty() {
            return true;
        }
        self.search.matches(&instance.name)
            || self.search.matches(&instance.kind_id)
            || self.search.matches(&self.kind_label(&instance.kind_id))
    }

    pub(super) fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            self.rebuild_visible();
            cx.notify();
        }
    }

    fn toggle_platform(&mut self, platform: Platform, cx: &mut Context<Self>) {
        if let Some(pos) = self.platforms.iter().position(|p| *p == platform) {
            self.platforms.remove(pos);
        } else {
            self.platforms.push(platform);
        }
        self.rebuild_visible();
        cx.notify();
    }

    fn clear_platforms(&mut self, cx: &mut Context<Self>) {
        self.platforms.clear();
        self.rebuild_visible();
        cx.notify();
    }

    fn set_usage_filter(&mut self, filter: UsageFilter, cx: &mut Context<Self>) {
        self.usage_filter = if self.usage_filter == filter {
            UsageFilter::All
        } else {
            filter
        };
        self.rebuild_visible();
        cx.notify();
    }

    fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.search.clear(cx);
        self.platforms.clear();
        self.usage_filter = UsageFilter::All;
        self.rebuild_visible();
        cx.notify();
    }

    fn select(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.list_cursor = Some(id);
        match self.listed_row(id) {
            Some(row) => cx.emit(TriggerListEvent::Selected(row)),
            None => {
                self.selected = None;
                cx.emit(TriggerListEvent::SelectionLost);
            }
        }
        cx.notify();
    }

    fn set_hover(&mut self, id: TriggerInstanceId, hovered: bool, cx: &mut Context<Self>) {
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

    fn open_row_menu(
        &mut self,
        id: TriggerInstanceId,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.menu_open = Some(id);
        self.menu_click_pos = Some(position);
        cx.notify();
    }

    pub(super) fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    fn persist_enabled(&mut self, id: TriggerInstanceId, enabled: bool, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.show_enabled(id, enabled, cx);
        cx.notify();
        let repo = Arc::clone(&self.deps.repo);
        self.enabled_writers.writer(id).write(
            async move {
                repo.set_enabled(id, enabled)
                    .await
                    .map(drop)
                    .map_err(RepoFailure::from)
            },
            Self::on_enabled_write_settled,
            cx,
        );
    }

    fn on_enabled_write_settled(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        match outcome {
            WriteOutcome::Written => self.on_write_settled(Ok(()), cx),
            WriteOutcome::Failed(failure) => self.on_write_settled(Err(failure), cx),
            WriteOutcome::Superseded => {}
        }
    }

    fn show_enabled(&mut self, id: TriggerInstanceId, enabled: bool, cx: &mut Context<Self>) {
        self.reload_generation.next();
        if let Some(row) = self.instances.iter_mut().find(|row| row.id == id) {
            row.enabled = enabled;
        }
        self.rebuild_visible();
        cx.emit(TriggerListEvent::EnabledShown(id, enabled));
    }

    pub(super) fn toggle_enable(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        let Some(instance) = self.find(id) else {
            return;
        };
        if !instance.enabled {
            self.persist_enabled(id, true, cx);
        } else if instance.used_in_count > 0 {
            self.confirm_disable.request(id);
            cx.notify();
        } else {
            self.persist_enabled(id, false, cx);
        }
    }

    fn confirm_disable_now(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.confirm_disable.take() {
            self.persist_enabled(id, false, cx);
        } else {
            cx.notify();
        }
    }

    fn cancel_disable(&mut self, cx: &mut Context<Self>) {
        self.confirm_disable.cancel();
        cx.notify();
    }

    pub(super) fn request_delete(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        self.pending_delete.request(id);
        self.menu_open = None;
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_delete.take() else {
            return;
        };
        let Some(instance) = self.find(id).filter(|i| i.used_in_count == 0) else {
            cx.notify();
            return;
        };
        let name = instance.name.clone();
        cx.notify();

        let repo = Arc::clone(&self.deps.repo);
        let restore_repo = Arc::clone(&self.deps.repo);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                repo.archive(id).await.map_err(RepoFailure::from)?;
                load_rows(&*repo).await
            },
            move |this, result, cx| match result {
                Ok(rows) => {
                    this.apply_rows(rows, cx);
                    this.raise_undo_toast(id, name, restore_repo, cx);
                }
                Err(failure) => on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn raise_undo_toast(
        &self,
        id: TriggerInstanceId,
        name: String,
        repo: Arc<dyn forge_storage::TriggerInstanceRepo>,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let message = tr!("triggers_toast_deleted", name = name.as_str());
        cx.push_undo_toast(message, move |_window, app: &mut App| {
            let repo = Arc::clone(&repo);
            view.update(app, |this, cx| {
                this.spawn_write(
                    async move { repo.restore(id).await.map_err(RepoFailure::from) },
                    cx,
                );
            });
        });
    }

    fn start_rename(&mut self, id: TriggerInstanceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(instance) = self.find(id) else {
            return;
        };
        let seed = instance.name.clone();
        let editor = inline_edit(seed, NAME_FS, window, cx);
        let sub = cx.subscribe(
            &editor,
            |this, _e, event: &InlineEditEvent, cx| match event {
                InlineEditEvent::Commit(next) => this.commit_rename(next.clone(), cx),
                InlineEditEvent::Cancel => this.cancel_rename(cx),
            },
        );
        self.menu_open = None;
        self.rename = Some(RenameForm {
            id,
            editor,
            _sub: sub,
        });
        cx.notify();
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename = None;
        cx.notify();
    }

    fn commit_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(form) = self.rename.take() else {
            cx.notify();
            return;
        };
        let trimmed = name.trim().to_owned();
        cx.notify();
        if trimmed.is_empty() {
            return;
        }
        let id = form.id;
        let repo = Arc::clone(&self.deps.repo);
        self.spawn_write(
            async move {
                let mut instance = repo
                    .get(id)
                    .await
                    .map_err(RepoFailure::from)?
                    .ok_or_else(RepoFailure::trigger_gone)?;
                instance.name = trimmed;
                repo.save(&instance).await.map_err(RepoFailure::from)
            },
            cx,
        );
    }
}

impl NavigableList for TriggerList {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        let current = self
            .list_cursor
            .and_then(|id| self.visible_position(id))
            .or_else(|| self.selected.and_then(|id| self.visible_position(id)));
        let Some(next) = stepped_index(current, self.visible.len(), step) else {
            return;
        };
        if let Some(row) = self
            .visible
            .get(next)
            .and_then(|&idx| self.instances.get(idx))
        {
            self.list_cursor = Some(row.id);
            self.list_scroll
                .scroll_to_item(next, ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    fn activate_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self
            .list_cursor
            .filter(|&id| self.visible_position(id).is_some())
        {
            self.select(id, cx);
        }
    }
}
