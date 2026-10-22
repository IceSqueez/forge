use super::*;
use forge_types::ExecutionMode;

const MISSING_ACTION: &str = "action not found";
const MISSING_CHAIN: &str = "step list not found";

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
        chain: Vec<SubActionStep>,
    },
    Duplicate {
        new_id: ActionId,
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
            Self::Settings(_) | Self::SetEnabled(_) | Self::Rename(_) | Self::SetChain { .. }
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

struct WriteBatchOutcome {
    effects: Vec<WriteEffect>,
    errors: Vec<String>,
    saved: Option<Action>,
    listed: Result<Vec<Action>, String>,
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
        ActionWrite::Create(_)
        | ActionWrite::Duplicate { .. }
        | ActionWrite::Archive { .. }
        | ActionWrite::Restore => false,
    }
}

async fn stored_action(repo: &dyn ActionRepo, id: ActionId) -> Result<Action, String> {
    repo.get(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| MISSING_ACTION.to_owned())
}

async fn perform_operation(
    repo: &dyn ActionRepo,
    id: ActionId,
    write: ActionWrite,
) -> Result<Option<WriteEffect>, String> {
    match write {
        ActionWrite::Create(action) => {
            repo.save(&action).await.map_err(|e| e.to_string())?;
            Ok(Some(WriteEffect::Select(action.id)))
        }
        ActionWrite::Duplicate { new_id } => {
            let source = stored_action(repo, id).await?;
            let new_name = format!("{} (copy)", source.name);
            repo.duplicate(id, new_id, &new_name)
                .await
                .map_err(|e| e.to_string())?;
            Ok(None)
        }
        ActionWrite::Archive { name } => {
            repo.archive(id).await.map_err(|e| e.to_string())?;
            Ok(Some(WriteEffect::ArchiveUndo { name }))
        }
        ActionWrite::Restore => {
            repo.restore(id).await.map_err(|e| e.to_string())?;
            Ok(None)
        }
        ActionWrite::Settings(_)
        | ActionWrite::SetEnabled(_)
        | ActionWrite::Rename(_)
        | ActionWrite::SetChain { .. } => Ok(None),
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
    ) -> Result<(), String> {
        if self.working.is_none() {
            self.working = Some(stored_action(repo, id).await?);
        }
        let Some(action) = self.working.as_mut() else {
            return Err(MISSING_ACTION.to_owned());
        };
        if !apply_action_edit(action, write) {
            return Err(MISSING_CHAIN.to_owned());
        }
        self.dirty = true;
        Ok(())
    }

    async fn flush(&mut self, repo: &dyn ActionRepo) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        self.dirty = false;
        let Some(action) = self.working.as_ref() else {
            return Ok(());
        };
        let saved = repo.save(action).await.map_err(|e| e.to_string());
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
    let listed = repo.list().await.map_err(|e| e.to_string());
    WriteBatchOutcome {
        effects,
        errors,
        saved: batch.working.filter(|_| !batch.save_failed),
        listed,
    }
}

impl ScreenActionsView {
    pub(super) fn write_action(
        &mut self,
        id: ActionId,
        write: ActionWrite,
        cx: &mut Context<Self>,
    ) {
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
        async_bridge::run_async(
            &self.rt_handle,
            perform_writes(repo, id, latest, writes),
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
        for message in &outcome.errors {
            self.on_repo_error(message, cx);
        }
        for effect in outcome.effects {
            match effect {
                WriteEffect::Select(new_id) => self.select(new_id, cx),
                WriteEffect::ArchiveUndo { name } => self.raise_undo_toast(id, name, cx),
            }
        }
        if drained {
            match outcome.listed {
                Ok(actions) => self.apply_actions(actions, cx),
                Err(message) => {
                    self.on_repo_error(&message, cx);
                    self.reload_detail(cx);
                }
            }
        } else {
            self.run_action_writes(id, cx);
        }
        cx.notify();
    }
}
