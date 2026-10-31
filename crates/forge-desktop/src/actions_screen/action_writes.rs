use super::*;
use crate::repo_failure::RepoFailure;
use forge_types::ExecutionMode;

pub(super) struct ActionSettings {
    pub name: String,
    pub group: Option<String>,
    pub queue_id: QueueId,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    pub execution_mode: ExecutionMode,
    pub description: Option<String>,
}

pub(super) enum ActionWrite {
    Create(Action),
    Settings(ActionSettings),
    SetEnabled(bool),
    Rename(String),
    SetChain {
        path: Vec<nav::NavFrame>,
        chain: Vec<nav::ChainItem>,
    },
    InsertStep {
        path: Vec<nav::NavFrame>,
        owners: Vec<String>,
        index: usize,
        item: nav::ChainItem,
    },
    Duplicate {
        new_id: ActionId,
        copy_suffix: String,
    },
    Archive {
        name: String,
    },
    Restore,
}

impl ActionWrite {
    fn is_edit(&self) -> bool {
        matches!(
            self,
            Self::Settings(_)
                | Self::SetEnabled(_)
                | Self::Rename(_)
                | Self::SetChain { .. }
                | Self::InsertStep { .. }
        )
    }
}

enum WriteEffect {
    Select(ActionId),
    ArchiveUndo { name: String },
}

#[derive(Default)]
pub(super) struct ActionWriteQueue {
    running: bool,
    pending: Vec<ActionWrite>,
    latest: Option<Action>,
}

enum ActionWriteError {
    MissingAction,
    MissingChain,
    Storage(RepoFailure),
}

impl ActionWriteError {
    fn storage(error: StorageError) -> Self {
        Self::Storage(error.into())
    }

    fn failure(&self) -> RepoFailure {
        match self {
            Self::MissingAction => RepoFailure::action_gone(),
            Self::MissingChain => RepoFailure::steps_gone(),
            Self::Storage(failure) => failure.clone(),
        }
    }
}

struct WriteBatchOutcome {
    effects: Vec<WriteEffect>,
    errors: Vec<ActionWriteError>,
    saved: Option<Action>,
    list_read: u64,
    listed: Result<Vec<Action>, RepoFailure>,
}

pub(super) fn apply_action_edit(action: &mut Action, write: &ActionWrite) -> bool {
    match write {
        ActionWrite::Settings(settings) => {
            action.name = settings.name.clone();
            action.group = settings.group.clone();
            action.queue_id = settings.queue_id;
            action.enabled = settings.enabled;
            action.concurrent = settings.concurrent;
            action.bypass_pause = settings.bypass_pause;
            action.execution_mode = settings.execution_mode;
            action.description = settings.description.clone();
            true
        }
        ActionWrite::SetEnabled(enabled) => {
            action.enabled = *enabled;
            true
        }
        ActionWrite::Rename(name) => {
            action.name = name.clone();
            true
        }
        ActionWrite::SetChain { path, chain } => {
            nav::set_chain(&mut action.sub_actions, path, chain)
        }
        ActionWrite::InsertStep {
            path,
            owners,
            index,
            item,
        } => nav::insert_item(&mut action.sub_actions, path, owners, *index, item),
        ActionWrite::Create(_)
        | ActionWrite::Duplicate { .. }
        | ActionWrite::Archive { .. }
        | ActionWrite::Restore => false,
    }
}

async fn stored_action(repo: &dyn ActionRepo, id: ActionId) -> Result<Action, ActionWriteError> {
    repo.get(id)
        .await
        .map_err(ActionWriteError::storage)?
        .ok_or(ActionWriteError::MissingAction)
}

async fn perform_operation(
    repo: &dyn ActionRepo,
    id: ActionId,
    write: ActionWrite,
) -> Result<Option<WriteEffect>, ActionWriteError> {
    match write {
        ActionWrite::Create(action) => {
            repo.save(&action)
                .await
                .map_err(ActionWriteError::storage)?;
            Ok(Some(WriteEffect::Select(action.id)))
        }
        ActionWrite::Duplicate {
            new_id,
            copy_suffix,
        } => {
            let source = stored_action(repo, id).await?;
            let new_name = format!("{} {copy_suffix}", source.name);
            repo.duplicate(id, new_id, &new_name)
                .await
                .map_err(ActionWriteError::storage)?;
            Ok(None)
        }
        ActionWrite::Archive { name } => {
            repo.archive(id).await.map_err(ActionWriteError::storage)?;
            Ok(Some(WriteEffect::ArchiveUndo { name }))
        }
        ActionWrite::Restore => {
            repo.restore(id).await.map_err(ActionWriteError::storage)?;
            Ok(None)
        }
        ActionWrite::Settings(_)
        | ActionWrite::SetEnabled(_)
        | ActionWrite::Rename(_)
        | ActionWrite::SetChain { .. }
        | ActionWrite::InsertStep { .. } => Ok(None),
    }
}

struct EditBatch {
    working: Option<Action>,
    dirty: bool,
    save_failed: bool,
}

impl EditBatch {
    async fn apply(
        &mut self,
        repo: &dyn ActionRepo,
        id: ActionId,
        write: &ActionWrite,
    ) -> Result<(), ActionWriteError> {
        if self.working.is_none() {
            self.working = Some(stored_action(repo, id).await?);
        }
        let Some(action) = self.working.as_mut() else {
            return Err(ActionWriteError::MissingAction);
        };
        if !apply_action_edit(action, write) {
            return Err(ActionWriteError::MissingChain);
        }
        self.dirty = true;
        Ok(())
    }

    async fn flush(&mut self, repo: &dyn ActionRepo) -> Result<(), ActionWriteError> {
        if !self.dirty {
            return Ok(());
        }
        self.dirty = false;
        let Some(action) = self.working.as_ref() else {
            return Ok(());
        };
        let saved = repo.save(action).await.map_err(ActionWriteError::storage);
        if saved.is_err() {
            self.save_failed = true;
        }
        saved
    }
}

async fn perform_writes(
    repo: Arc<dyn ActionRepo>,
    id: ActionId,
    latest: Option<Action>,
    writes: Vec<ActionWrite>,
    list_reads: Arc<AtomicU64>,
) -> WriteBatchOutcome {
    let mut effects = Vec::new();
    let mut errors = Vec::new();
    let mut batch = EditBatch {
        working: latest,
        dirty: false,
        save_failed: false,
    };
    let mut settings_changed = false;
    for write in writes {
        if write.is_edit() {
            settings_changed |= matches!(write, ActionWrite::Settings(_));
            if let Err(message) = batch.apply(&*repo, id, &write).await {
                errors.push(message);
            }
            continue;
        }
        if let Err(message) = batch.flush(&*repo).await {
            errors.push(message);
        }
        if let ActionWrite::Create(action) = &write {
            batch.working = Some(action.clone());
        }
        match perform_operation(&*repo, id, write).await {
            Ok(effect) => effects.extend(effect),
            Err(message) => errors.push(message),
        }
    }
    if let Err(message) = batch.flush(&*repo).await {
        errors.push(message);
    }
    if settings_changed {
        effects.push(WriteEffect::Select(id));
    }
    let list_read = next_list_read(&list_reads);
    let listed = repo.list().await.map_err(RepoFailure::from);
    WriteBatchOutcome {
        effects,
        errors,
        saved: batch.working.filter(|_| !batch.save_failed),
        list_read,
        listed,
    }
}

impl ScreenActionsView {
    fn settled_against_known_action(
        &self,
        id: ActionId,
        write: ActionWrite,
    ) -> Option<ActionWrite> {
        let ActionWrite::InsertStep {
            path,
            owners,
            index,
            item,
        } = &write
        else {
            return Some(write);
        };
        let shown = self
            .detail
            .as_ref()
            .filter(|detail| detail.action.id == id)
            .map(|detail| &detail.action);
        let Some(known) = shown.or_else(|| self.latest_written_action(id)) else {
            return Some(write);
        };
        let mut steps = known.sub_actions.clone();
        nav::insert_item(&mut steps, path, owners, *index, item).then(|| ActionWrite::SetChain {
            path: path.clone(),
            chain: nav::resolve_chain(&steps, path),
        })
    }

    pub(super) fn write_action(
        &mut self,
        id: ActionId,
        write: ActionWrite,
        cx: &mut Context<Self>,
    ) {
        let Some(write) = self.settled_against_known_action(id, write) else {
            self.on_repo_error(&RepoFailure::steps_gone(), cx);
            return;
        };
        let mut shown: Option<Action> = None;
        if let Some(detail) = self.detail.as_mut()
            && detail.action.id == id
        {
            if apply_action_edit(&mut detail.action, &write) {
                shown = Some(detail.action.clone());
            } else if write.is_edit() {
                return;
            }
        }
        let is_new = !self.action_writes.contains_key(&id);
        let queue = self.action_writes.entry(id).or_default();
        match (queue.latest.as_mut(), &write) {
            (_, ActionWrite::Create(action)) => queue.latest = Some(action.clone()),
            (Some(latest), _) => {
                apply_action_edit(latest, &write);
            }
            (None, _) if is_new => queue.latest = shown,
            (None, _) => {}
        }
        queue.pending.push(write);
        if self.detail.as_ref().is_some_and(|d| d.action.id == id) {
            self.sync_case_fields(cx);
            self.recompute_step_health();
        }
        self.run_action_writes(id, cx);
        cx.notify();
    }

    pub(super) fn has_pending_action_writes(&self, id: ActionId) -> bool {
        self.action_writes.contains_key(&id)
    }

    pub(super) fn latest_written_action(&self, id: ActionId) -> Option<&Action> {
        self.action_writes.get(&id)?.latest.as_ref()
    }

    fn run_action_writes(&mut self, id: ActionId, cx: &mut Context<Self>) {
        let Some(queue) = self.action_writes.get_mut(&id) else {
            return;
        };
        if queue.running || queue.pending.is_empty() {
            return;
        }
        queue.running = true;
        let writes = std::mem::take(&mut queue.pending);
        let latest = queue.latest.clone();
        let repo = Arc::clone(&self.action_repo);
        let list_reads = Arc::clone(&self.list_reads);
        async_bridge::run_async(
            &self.rt_handle,
            perform_writes(repo, id, latest, writes, list_reads),
            move |this, outcome, cx| this.finish_action_writes(id, outcome, cx),
            cx,
        );
    }

    fn finish_action_writes(
        &mut self,
        id: ActionId,
        outcome: WriteBatchOutcome,
        cx: &mut Context<Self>,
    ) {
        let save_failed = outcome.saved.is_none();
        let drained = match self.action_writes.get_mut(&id) {
            Some(queue) => {
                queue.running = false;
                if save_failed {
                    queue.latest = None;
                } else if queue.latest.is_none() {
                    queue.latest = outcome.saved;
                }
                queue.pending.is_empty()
            }
            None => true,
        };
        if drained {
            self.action_writes.remove(&id);
        }
        for error in &outcome.errors {
            self.on_repo_error(&error.failure(), cx);
        }
        for effect in outcome.effects {
            match effect {
                WriteEffect::Select(new_id) => self.select(new_id, cx),
                WriteEffect::ArchiveUndo { name } => self.raise_undo_toast(id, name, cx),
            }
        }
        if drained {
            self.land_list(outcome.list_read, outcome.listed, cx);
        } else {
            self.run_action_writes(id, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
pub(in crate::actions_screen) mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use forge_storage::{
        ActionRepo, ActionTelemetry, ExecutionStatus, MockMediaRepo, StorageError,
    };
    use gpui::TestAppContext;
    use time::OffsetDateTime;

    use super::super::editor::tests::view_with;
    use super::*;
    use crate::test_support::{action_running, error_toasts, pump, pump_until, runtime, step};

    pub(in crate::actions_screen) const FIRST: &str = "test.first";
    pub(in crate::actions_screen) const SECOND: &str = "test.second";
    pub(in crate::actions_screen) const THIRD: &str = "test.third";
    const ALPHA: &str = "Alpha";
    const BRAVO: &str = "Bravo";
    const RENAMED: &str = "Renamed";
    const SETTLE_ROUNDS: usize = 4;
    const SAVE_OP: &str = "save";
    const ARCHIVE_OP: &str = "archive";

    fn chain() -> Vec<SubActionStep> {
        vec![step(FIRST, true), step(SECOND, true), step(THIRD, true)]
    }

    pub(in crate::actions_screen) fn kinds(steps: &[SubActionStep]) -> Vec<String> {
        steps.iter().map(|step| step.kind_id.clone()).collect()
    }

    fn rotated() -> Vec<String> {
        [SECOND, THIRD, FIRST].map(str::to_owned).to_vec()
    }

    fn original() -> Vec<String> {
        [FIRST, SECOND, THIRD].map(str::to_owned).to_vec()
    }

    fn settings_from(action: &Action) -> ActionSettings {
        ActionSettings {
            name: RENAMED.to_owned(),
            group: Some(BRAVO.to_owned()),
            queue_id: QueueId::new(),
            enabled: !action.enabled,
            concurrent: !action.concurrent,
            bypass_pause: !action.bypass_pause,
            execution_mode: ExecutionMode::RandomPick,
            description: Some(ALPHA.to_owned()),
        }
    }

    #[test]
    fn each_edit_changes_only_its_own_fields() {
        let base = action_running(ALPHA, chain());
        let settings = settings_from(&base);
        let mut with_settings = base.clone();
        with_settings.name = settings.name.clone();
        with_settings.group = settings.group.clone();
        with_settings.queue_id = settings.queue_id;
        with_settings.enabled = settings.enabled;
        with_settings.concurrent = settings.concurrent;
        with_settings.bypass_pause = settings.bypass_pause;
        with_settings.execution_mode = settings.execution_mode;
        with_settings.description = settings.description.clone();
        let mut disabled = base.clone();
        disabled.enabled = false;
        let mut renamed = base.clone();
        renamed.name = RENAMED.to_owned();
        let mut reordered = base.clone();
        reordered.sub_actions.rotate_left(1);
        for (write, expected, case) in [
            (ActionWrite::Settings(settings), with_settings, "settings"),
            (ActionWrite::SetEnabled(false), disabled, "enabled"),
            (ActionWrite::Rename(RENAMED.to_owned()), renamed, "rename"),
            (
                ActionWrite::SetChain {
                    path: Vec::new(),
                    chain: reordered
                        .sub_actions
                        .iter()
                        .cloned()
                        .map(nav::ChainItem::fresh)
                        .collect(),
                },
                reordered,
                "chain",
            ),
        ] {
            let mut action = base.clone();

            let applied = apply_action_edit(&mut action, &write);

            assert_eq!((applied, action), (true, expected), "{case}");
        }
    }

    #[test]
    fn a_chain_edit_on_a_missing_nested_path_changes_nothing() {
        let base = action_running(ALPHA, chain());
        let mut action = base.clone();
        let write = ActionWrite::SetChain {
            path: vec![nav::NavFrame {
                step_index: chain().len(),
                chain_key: FIRST.to_owned(),
                case_index: None,
            }],
            chain: Vec::new(),
        };

        let applied = apply_action_edit(&mut action, &write);

        assert_eq!((applied, action), (false, base));
    }

    #[test]
    fn repository_operations_are_not_applied_as_edits() {
        let base = action_running(ALPHA, chain());
        for (write, case) in [
            (
                ActionWrite::Create(action_running(BRAVO, Vec::new())),
                "create",
            ),
            (
                ActionWrite::Duplicate {
                    new_id: ActionId::new(),
                    copy_suffix: String::new(),
                },
                "duplicate",
            ),
            (
                ActionWrite::Archive {
                    name: ALPHA.to_owned(),
                },
                "archive",
            ),
            (ActionWrite::Restore, "restore"),
        ] {
            let mut action = base.clone();

            let applied = apply_action_edit(&mut action, &write);

            assert_eq!((applied, action), (false, base.clone()), "{case}");
        }
    }

    #[test]
    fn write_errors_read_in_the_ui_language() {
        crate::i18n::install_language(forge_storage::Language::Uk);
        for (error, expected) in [
            (ActionWriteError::MissingAction, "Цієї дії більше немає."),
            (
                ActionWriteError::MissingChain,
                "Списку кроків цієї дії більше немає.",
            ),
            (
                ActionWriteError::Storage(RepoFailure::operation("disk full")),
                "Операцію не виконано. Подробиці в журналі.",
            ),
        ] {
            assert_eq!(error.failure().text(), expected);
        }
    }

    #[derive(Default)]
    pub(in crate::actions_screen) struct MemActions {
        actions: Mutex<HashMap<ActionId, Action>>,
        archived: Mutex<HashSet<ActionId>>,
        failing_saves: Mutex<HashMap<ActionId, usize>>,
        ops: Mutex<Vec<(&'static str, ActionId)>>,
        next_list_held: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
        detail_reads: std::sync::atomic::AtomicUsize,
    }

    impl MemActions {
        pub(in crate::actions_screen) fn stored(&self, id: ActionId) -> Action {
            self.actions.lock().unwrap().get(&id).cloned().unwrap()
        }

        fn fail_next_save(&self, id: ActionId) {
            *self.failing_saves.lock().unwrap().entry(id).or_default() += 1;
        }
    }

    #[async_trait::async_trait]
    impl ActionRepo for MemActions {
        async fn list(&self) -> Result<Vec<Action>, StorageError> {
            let archived = self.archived.lock().unwrap().clone();
            let mut listed: Vec<Action> = self
                .actions
                .lock()
                .unwrap()
                .values()
                .filter(|action| !archived.contains(&action.id))
                .cloned()
                .collect();
            listed.sort_by(|a, b| a.name.cmp(&b.name));
            let held = self.next_list_held.lock().unwrap().take();
            if let Some(gate) = held {
                gate.await.ok();
            }
            Ok(listed)
        }

        async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
            self.detail_reads
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.actions.lock().unwrap().get(&id).cloned())
        }

        async fn save(&self, action: &Action) -> Result<(), StorageError> {
            if let Some(left) = self.failing_saves.lock().unwrap().get_mut(&action.id)
                && *left > 0
            {
                *left -= 1;
                return Err(StorageError::NotFound {
                    key: action.id.to_string(),
                });
            }
            self.ops.lock().unwrap().push((SAVE_OP, action.id));
            self.actions
                .lock()
                .unwrap()
                .insert(action.id, action.clone());
            Ok(())
        }

        async fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
            Ok(self.actions.lock().unwrap().remove(&id).is_some())
        }

        async fn telemetry(&self, _: ActionId) -> Result<ActionTelemetry, StorageError> {
            Ok(ActionTelemetry::default())
        }

        async fn record_execution(
            &self,
            _: ActionId,
            _: OffsetDateTime,
            _: u64,
            _: ExecutionStatus,
        ) -> Result<(), StorageError> {
            Ok(())
        }

        async fn prune_executions_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn archive(&self, id: ActionId) -> Result<bool, StorageError> {
            self.ops.lock().unwrap().push((ARCHIVE_OP, id));
            Ok(self.archived.lock().unwrap().insert(id))
        }

        async fn restore(&self, id: ActionId) -> Result<bool, StorageError> {
            Ok(self.archived.lock().unwrap().remove(&id))
        }
    }

    pub(in crate::actions_screen) struct Screen {
        pub(in crate::actions_screen) view: Entity<ScreenActionsView>,
        pub(in crate::actions_screen) store: Arc<MemActions>,
        pub(in crate::actions_screen) alpha: ActionId,
        pub(in crate::actions_screen) bravo: ActionId,
        pub(in crate::actions_screen) rt: tokio::runtime::Runtime,
    }

    impl Screen {
        pub(in crate::actions_screen) fn open(cx: &mut TestAppContext) -> Self {
            crate::i18n::install_language(forge_storage::Language::En);
            cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
            let rt = runtime();
            let store = Arc::new(MemActions::default());
            let alpha = action_running(ALPHA, chain());
            let bravo = action_running(BRAVO, chain());
            let ids = (alpha.id, bravo.id);
            for action in [alpha, bravo] {
                store.actions.lock().unwrap().insert(action.id, action);
            }
            let view = view_with(
                cx,
                &rt,
                Vec::new(),
                MockMediaRepo::new(),
                SubActionRegistry::new(),
                Arc::clone(&store) as Arc<dyn ActionRepo>,
            );
            let screen = Self {
                view,
                store,
                alpha: ids.0,
                bravo: ids.1,
                rt,
            };
            screen.settle(cx);
            let alpha = screen.alpha;
            screen.view.update(cx, |view, cx| view.select(alpha, cx));
            screen.settle(cx);
            screen
        }

        pub(in crate::actions_screen) fn settle(&self, cx: &mut TestAppContext) {
            for _ in 0..SETTLE_ROUNDS {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }

        pub(in crate::actions_screen) fn write(
            &self,
            cx: &mut TestAppContext,
            id: ActionId,
            write: ActionWrite,
        ) {
            self.view
                .update(cx, |view, cx| view.write_action(id, write, cx));
        }

        pub(in crate::actions_screen) fn reorder_shown(&self, cx: &mut TestAppContext) {
            self.view.update(cx, |view, cx| {
                view.persist_chain_mutation(|chain| chain.rotate_left(1), cx)
            });
        }

        pub(in crate::actions_screen) fn shown(&self, cx: &mut TestAppContext) -> Action {
            self.view
                .read_with(cx, |view, _| view.detail.as_ref().unwrap().action.clone())
        }

        pub(in crate::actions_screen) fn hold_next_list(&self) -> tokio::sync::oneshot::Sender<()> {
            let (release, gate) = tokio::sync::oneshot::channel();
            *self.store.next_list_held.lock().unwrap() = Some(gate);
            release
        }

        fn idle(&self, cx: &mut TestAppContext) -> bool {
            self.view
                .read_with(cx, |view, _| view.action_writes.is_empty())
        }
    }

    #[gpui::test]
    fn edits_on_two_actions_in_flight_together_each_keep_their_own_change(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        screen.reorder_shown(cx);
        screen.write(cx, screen.bravo, ActionWrite::Rename(RENAMED.to_owned()));

        screen.settle(cx);

        let alpha = screen.store.stored(screen.alpha);
        let bravo = screen.store.stored(screen.bravo);
        assert_eq!(
            (
                kinds(&alpha.sub_actions),
                alpha.name,
                kinds(&bravo.sub_actions),
                bravo.name
            ),
            (rotated(), ALPHA.to_owned(), original(), RENAMED.to_owned())
        );
    }

    #[gpui::test]
    fn an_enable_toggle_queued_behind_a_reorder_keeps_both(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        screen.reorder_shown(cx);
        let alpha = screen.alpha;
        screen
            .view
            .update(cx, |view, cx| view.set_enabled(alpha, false, cx));

        screen.settle(cx);

        let stored = screen.store.stored(screen.alpha);
        assert_eq!(
            (kinds(&stored.sub_actions), stored.enabled),
            (rotated(), false)
        );
    }

    #[gpui::test]
    fn a_rename_queued_behind_a_reorder_keeps_both(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        screen.reorder_shown(cx);
        screen.write(cx, screen.alpha, ActionWrite::Rename(RENAMED.to_owned()));

        screen.settle(cx);

        let stored = screen.store.stored(screen.alpha);
        assert_eq!(
            (kinds(&stored.sub_actions), stored.name),
            (rotated(), RENAMED.to_owned())
        );
    }

    #[gpui::test]
    fn a_settings_edit_queued_behind_a_reorder_keeps_the_new_order(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        let settings = settings_from(&screen.shown(cx));
        screen.reorder_shown(cx);
        screen.write(cx, screen.alpha, ActionWrite::Settings(settings));

        screen.settle(cx);

        let stored = screen.store.stored(screen.alpha);
        assert_eq!(
            (kinds(&stored.sub_actions), stored.name),
            (rotated(), RENAMED.to_owned())
        );
    }

    #[gpui::test]
    fn an_archive_queued_behind_a_reorder_saves_the_order_first(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        screen.write(cx, screen.alpha, ActionWrite::Rename(RENAMED.to_owned()));
        screen.reorder_shown(cx);
        screen.write(
            cx,
            screen.alpha,
            ActionWrite::Archive {
                name: ALPHA.to_owned(),
            },
        );

        screen.settle(cx);

        let ops = screen.store.ops.lock().unwrap().clone();
        assert_eq!(
            (ops, kinds(&screen.store.stored(screen.alpha).sub_actions)),
            (
                vec![
                    (SAVE_OP, screen.alpha),
                    (SAVE_OP, screen.alpha),
                    (ARCHIVE_OP, screen.alpha),
                ],
                rotated()
            )
        );
    }

    #[gpui::test]
    fn an_edit_right_after_create_lands_on_the_new_action(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        let created = action_running(THIRD, Vec::new());
        let id = created.id;
        screen.write(cx, id, ActionWrite::Create(created));
        screen.write(cx, id, ActionWrite::Rename(RENAMED.to_owned()));

        screen.settle(cx);

        assert_eq!(screen.store.stored(id).name, RENAMED);
    }

    #[gpui::test]
    fn a_duplicate_is_named_after_the_source_with_the_passed_suffix(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        let new_id = ActionId::new();
        screen.write(
            cx,
            screen.alpha,
            ActionWrite::Duplicate {
                new_id,
                copy_suffix: "(копія)".to_owned(),
            },
        );

        screen.settle(cx);

        assert_eq!(screen.store.stored(new_id).name, "Alpha (копія)");
    }

    #[gpui::test]
    fn a_created_action_ends_up_selected(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        let created = action_running(THIRD, Vec::new());
        let id = created.id;

        screen.write(cx, id, ActionWrite::Create(created));
        screen.settle(cx);

        assert_eq!(screen.shown(cx).id, id);
    }

    #[gpui::test]
    fn a_failed_save_on_one_action_leaves_the_other_actions_save_intact(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        screen.store.fail_next_save(screen.alpha);
        screen.reorder_shown(cx);
        screen.write(cx, screen.bravo, ActionWrite::Rename(RENAMED.to_owned()));

        screen.settle(cx);

        assert_eq!(
            (
                kinds(&screen.store.stored(screen.alpha).sub_actions),
                screen.store.stored(screen.bravo).name
            ),
            (original(), RENAMED.to_owned())
        );
    }

    #[gpui::test]
    fn a_failed_save_shows_the_stored_action_and_one_error_toast(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        screen.store.fail_next_save(screen.alpha);
        screen.reorder_shown(cx);

        screen.settle(cx);

        assert_eq!(
            (
                kinds(&screen.shown(cx).sub_actions),
                cx.update(|cx| error_toasts(cx)),
                screen.idle(cx)
            ),
            (original(), 1, true)
        );
    }

    #[gpui::test]
    fn an_edit_queued_behind_a_failed_save_is_still_stored(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        screen.store.fail_next_save(screen.alpha);
        screen.reorder_shown(cx);
        screen.write(cx, screen.alpha, ActionWrite::Rename(RENAMED.to_owned()));

        screen.settle(cx);

        let stored = screen.store.stored(screen.alpha);
        let shown = screen.shown(cx);
        assert_eq!(
            (stored.name, shown.name, kinds(&shown.sub_actions)),
            (
                RENAMED.to_owned(),
                RENAMED.to_owned(),
                kinds(&stored.sub_actions)
            )
        );
    }

    #[gpui::test]
    fn a_chain_edit_under_a_missing_nested_path_writes_nothing(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        screen.view.update(cx, |view, _| {
            view.nav_path = vec![nav::NavFrame {
                step_index: chain().len(),
                chain_key: FIRST.to_owned(),
                case_index: None,
            }];
        });

        screen.view.update(cx, |view, cx| {
            view.persist_chain_mutation(
                |chain| chain.push(nav::ChainItem::fresh(step(FIRST, true))),
                cx,
            )
        });
        screen.settle(cx);

        assert_eq!(
            (
                screen.store.ops.lock().unwrap().len(),
                cx.update(|cx| error_toasts(cx)),
                screen.idle(cx)
            ),
            (0, 0, true)
        );
    }

    impl Screen {
        fn detail_reads(&self) -> usize {
            self.store
                .detail_reads
                .load(std::sync::atomic::Ordering::SeqCst)
        }

        fn listing_named(&self, name: &str) -> Vec<Action> {
            let mut action = self.store.stored(self.alpha);
            action.name = name.to_owned();
            vec![action]
        }

        fn listed_name(&self, cx: &mut TestAppContext, id: ActionId) -> Option<String> {
            self.view.read_with(cx, |view, _| {
                view.find(id).map(|action| action.name.clone())
            })
        }

        fn wait_for_name(&self, cx: &mut TestAppContext, id: ActionId, name: &str) {
            pump_until(&self.rt, || {
                cx.run_until_parked();
                self.listed_name(cx, id).as_deref() == Some(name)
            });
        }
    }

    type Landed = Arc<std::sync::atomic::AtomicBool>;

    fn landed() -> Landed {
        Arc::default()
    }

    fn has_landed(flag: &Landed) -> bool {
        flag.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn mark_landed(flag: &Landed) {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    #[gpui::test]
    fn a_slow_reload_never_overwrites_a_newer_one_that_landed_first(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        let (older, newer) = (screen.listing_named("older"), screen.listing_named("newer"));
        let (release_older, older_gate) = tokio::sync::oneshot::channel::<()>();
        let older_landed = landed();
        let flag = Arc::clone(&older_landed);

        screen.view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    older_gate.await.map_err(RepoFailure::operation)?;
                    mark_landed(&flag);
                    Ok(older)
                },
                cx,
            );
            view.spawn_reload(async move { Ok(newer) }, cx);
        });
        screen.wait_for_name(cx, screen.alpha, "newer");
        release_older.send(()).unwrap();
        pump_until(&screen.rt, || has_landed(&older_landed));
        cx.run_until_parked();

        assert_eq!(
            screen.listed_name(cx, screen.alpha).as_deref(),
            Some("newer")
        );
    }

    #[gpui::test]
    fn a_stale_reload_that_failed_is_dropped_without_a_toast(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        let newer = screen.listing_named("newer");
        let (release_older, older_gate) = tokio::sync::oneshot::channel::<()>();
        let older_landed = landed();
        let flag = Arc::clone(&older_landed);

        screen.view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    older_gate.await.map_err(RepoFailure::operation)?;
                    mark_landed(&flag);
                    Err(RepoFailure::operation("disk gone"))
                },
                cx,
            );
            view.spawn_reload(async move { Ok(newer) }, cx);
        });
        screen.wait_for_name(cx, screen.alpha, "newer");
        release_older.send(()).unwrap();
        pump_until(&screen.rt, || has_landed(&older_landed));
        cx.run_until_parked();

        assert_eq!(cx.update(|cx| error_toasts(cx)), 0);
    }

    #[gpui::test]
    fn a_stale_failed_reload_leaves_the_open_detail_unread(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        let newer = screen.listing_named("newer");
        let (release_older, older_gate) = tokio::sync::oneshot::channel::<()>();
        let older_landed = landed();
        let flag = Arc::clone(&older_landed);

        screen.view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    older_gate.await.map_err(RepoFailure::operation)?;
                    mark_landed(&flag);
                    Err(RepoFailure::operation("disk gone"))
                },
                cx,
            );
            view.spawn_reload(async move { Ok(newer) }, cx);
        });
        screen.wait_for_name(cx, screen.alpha, "newer");
        screen.settle(cx);
        let reads_before = screen.detail_reads();
        release_older.send(()).unwrap();
        pump_until(&screen.rt, || has_landed(&older_landed));
        screen.settle(cx);

        assert_eq!(screen.detail_reads(), reads_before);
    }

    #[gpui::test]
    fn a_current_failed_reload_rereads_the_open_detail(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        let reads_before = screen.detail_reads();

        screen.view.update(cx, |view, cx| {
            view.spawn_reload(async { Err(RepoFailure::operation("disk gone")) }, cx);
        });
        screen.settle(cx);

        assert!(screen.detail_reads() > reads_before);
    }

    #[gpui::test]
    fn an_older_listing_never_lands_after_a_newer_read_failed(cx: &mut TestAppContext) {
        let screen = Screen::open(cx);
        let older = screen.listing_named("older");
        let (release_older, older_gate) = tokio::sync::oneshot::channel::<()>();
        let older_landed = landed();
        let flag = Arc::clone(&older_landed);

        screen.view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    older_gate.await.map_err(RepoFailure::operation)?;
                    mark_landed(&flag);
                    Ok(older)
                },
                cx,
            );
            view.spawn_reload(async { Err(RepoFailure::operation("disk gone")) }, cx);
        });
        pump_until(&screen.rt, || {
            cx.run_until_parked();
            cx.update(|cx| error_toasts(cx)) == 1
        });
        release_older.send(()).unwrap();
        pump_until(&screen.rt, || has_landed(&older_landed));
        screen.settle(cx);

        assert_eq!(screen.listed_name(cx, screen.alpha).as_deref(), Some(ALPHA));
    }

    #[gpui::test]
    fn a_reload_started_before_a_write_cannot_undo_the_list_that_write_drained(
        cx: &mut TestAppContext,
    ) {
        let screen = Screen::open(cx);
        let before_the_write = screen.listing_named(ALPHA);
        let (release_stale, stale_gate) = tokio::sync::oneshot::channel::<()>();
        let stale_landed = landed();
        let flag = Arc::clone(&stale_landed);

        screen.view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    stale_gate.await.map_err(RepoFailure::operation)?;
                    mark_landed(&flag);
                    Ok(before_the_write)
                },
                cx,
            );
        });
        screen.write(cx, screen.alpha, ActionWrite::Rename(RENAMED.to_owned()));
        screen.wait_for_name(cx, screen.alpha, RENAMED);
        release_stale.send(()).unwrap();
        pump_until(&screen.rt, || has_landed(&stale_landed));
        cx.run_until_parked();

        assert_eq!(
            screen.listed_name(cx, screen.alpha).as_deref(),
            Some(RENAMED)
        );
    }

    #[gpui::test]
    fn a_write_drain_holding_an_old_list_keeps_a_newer_drain_on_another_action(
        cx: &mut TestAppContext,
    ) {
        let screen = Screen::open(cx);
        let (release_alpha_list, alpha_list_gate) = tokio::sync::oneshot::channel::<()>();
        *screen.store.next_list_held.lock().unwrap() = Some(alpha_list_gate);

        screen.write(
            cx,
            screen.alpha,
            ActionWrite::Rename("Alpha new".to_owned()),
        );
        pump_until(&screen.rt, || {
            screen.store.next_list_held.lock().unwrap().is_none()
        });
        screen.write(
            cx,
            screen.bravo,
            ActionWrite::Rename("Bravo new".to_owned()),
        );
        screen.wait_for_name(cx, screen.bravo, "Bravo new");
        release_alpha_list.send(()).unwrap();
        pump_until(&screen.rt, || {
            cx.run_until_parked();
            screen.idle(cx)
        });
        screen.settle(cx);

        assert_eq!(
            screen.listed_name(cx, screen.bravo).as_deref(),
            Some("Bravo new")
        );
    }
}
