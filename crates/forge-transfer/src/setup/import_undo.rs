use forge_speech_types::{AliasId, AssignmentStrategy, IgnoreProfile, VoiceAlias};
use forge_storage::{
    DataProvider, FilterRule, SettingRow, StorageError, TtsPipelineSettings, remove_row,
    restore_row,
};
use forge_types::{ActionId, TriggerInstanceId};

use crate::documents::undo::{Undo, revert};

pub(crate) enum SetupUndo {
    Configuration(Undo),
    Relink {
        action: ActionId,
        trigger: TriggerInstanceId,
        position: i64,
    },
    SettingRestore(SettingRow),
    SettingRemove(SettingRow),
    TtsPipeline(TtsPipelineSettings),
    FilterRules(Vec<FilterRule>),
    VoiceAlias {
        id: AliasId,
        previous: Option<VoiceAlias>,
    },
    VoiceStrategy(AssignmentStrategy),
    IgnoreProfile(IgnoreProfile),
}

#[derive(Default)]
pub(crate) struct SetupJournal {
    entries: Vec<SetupUndo>,
}

impl SetupJournal {
    pub(crate) fn push(&mut self, undo: SetupUndo) {
        self.entries.push(undo);
    }

    pub(crate) fn push_configuration(&mut self, undo: Undo) {
        self.entries.push(SetupUndo::Configuration(undo));
    }

    pub(crate) async fn roll_back(self, provider: &dyn DataProvider) -> Result<(), String> {
        let mut failures = Vec::new();
        for undo in self.entries.into_iter().rev() {
            if let Err(e) = revert_setup(provider, undo).await {
                failures.push(e);
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

async fn revert_setup(provider: &dyn DataProvider, undo: SetupUndo) -> Result<(), String> {
    let storage = |e: StorageError| e.to_string();
    match undo {
        SetupUndo::Configuration(undo) => revert(provider, undo).await,
        SetupUndo::Relink {
            action,
            trigger,
            position,
        } => provider
            .trigger_instance_repo()
            .link_action_at(action, trigger, position)
            .await
            .map_err(storage),
        SetupUndo::SettingRestore(row) => restore_row(provider, &row).await.map_err(storage),
        SetupUndo::SettingRemove(row) => {
            remove_row(provider, &row).await.map(drop).map_err(storage)
        }
        SetupUndo::TtsPipeline(previous) => provider
            .tts_filters_repo()
            .set_pipeline_settings(&previous)
            .await
            .map_err(storage),
        SetupUndo::FilterRules(previous) => provider
            .tts_filters_repo()
            .replace_rules(&previous)
            .await
            .map_err(storage),
        SetupUndo::VoiceAlias { id, previous } => {
            let repo = provider.voice_alias_repo();
            match previous {
                Some(alias) => repo.upsert(&alias).await.map(drop).map_err(storage),
                None => repo.delete(&id).await.map_err(storage),
            }
        }
        SetupUndo::VoiceStrategy(previous) => provider
            .voice_alias_repo()
            .set_strategy(&previous)
            .await
            .map_err(storage),
        SetupUndo::IgnoreProfile(previous) => provider
            .voice_alias_repo()
            .set_ignore_profile(&previous)
            .await
            .map_err(storage),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_storage::DATABASE_FILE_NAME;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{Action, ExecutionMode, PermissionRung, TriggerInstance};

    use super::*;

    async fn backend(dir: &tempfile::TempDir) -> SqliteBackend {
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        SqliteBackend::open_for_test(&url, [7; 32], dir.path().join("media"), None)
            .await
            .unwrap()
    }

    async fn saved_action(provider: &dyn DataProvider) -> ActionId {
        let queue_id = provider
            .queue_repo()
            .get_by_name("Default")
            .await
            .unwrap()
            .unwrap()
            .id;
        let action = Action {
            id: ActionId::new(),
            name: "Relinked".to_owned(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![],
        };
        provider.action_repo().save(&action).await.unwrap();
        action.id
    }

    async fn saved_trigger(provider: &dyn DataProvider) -> TriggerInstanceId {
        let instance = TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: "core.manual".to_owned(),
            name: "Linked".to_owned(),
            overrides: BTreeMap::new(),
            enabled: true,
            user_defined: true,
            platform_scope: Default::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        };
        provider
            .trigger_instance_repo()
            .save(&instance)
            .await
            .unwrap();
        instance.id
    }

    #[tokio::test]
    async fn roll_back_relinks_unlinked_triggers_in_their_original_order() {
        let dir = tempfile::tempdir().unwrap();
        let backend = backend(&dir).await;
        let provider: &dyn DataProvider = &backend;
        let repo = provider.trigger_instance_repo();
        let action = saved_action(provider).await;
        let mut original = Vec::new();
        for _ in 0..3 {
            let trigger = saved_trigger(provider).await;
            repo.link_action(action, trigger).await.unwrap();
            original.push(trigger);
        }
        let mut journal = SetupJournal::default();
        for (position, trigger) in original.iter().enumerate() {
            repo.unlink_action(action, *trigger).await.unwrap();
            journal.push(SetupUndo::Relink {
                action,
                trigger: *trigger,
                position: position as i64,
            });
        }

        journal.roll_back(provider).await.unwrap();

        let restored: Vec<TriggerInstanceId> = repo
            .list_for_action(action)
            .await
            .unwrap()
            .into_iter()
            .map(|trigger| trigger.id)
            .collect();
        assert_eq!(restored, original);
    }
}
