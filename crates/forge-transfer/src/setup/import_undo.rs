use forge_speech_types::{AliasId, AssignmentStrategy, IgnoreProfile, VoiceAlias};
use forge_storage::{
    DataProvider, FilterRule, OverlayId, SettingsRepo, StorageError, TtsPipelineSettings,
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
    Setting {
        key: String,
        previous: Option<String>,
    },
    TtsPipeline(TtsPipelineSettings),
    FilterRules(Vec<FilterRule>),
    VoiceAlias {
        id: AliasId,
        previous: Option<VoiceAlias>,
    },
    VoiceStrategy(AssignmentStrategy),
    IgnoreProfile(IgnoreProfile),
    RemovedOverlay(OverlayId),
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
        SetupUndo::Setting { key, previous } => match previous {
            Some(value) => SettingsRepo::set_string(provider, &key, &value)
                .await
                .map_err(storage),
            None => SettingsRepo::delete(provider, &key)
                .await
                .map(drop)
                .map_err(storage),
        },
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
        SetupUndo::RemovedOverlay(id) => Err(format!(
            "overlay '{}' was removed and survives only in the safety backup",
            id.as_str()
        )),
    }
}
