use forge_speech_types::{AliasId, IgnoreProfile, VoiceAlias};
use forge_types::{
    Action, ActionId, ClipId, Queue, QueueId, ScriptId, TriggerInstance, TriggerInstanceId, Variant,
};

use crate::{
    FilterRule, MediaBlobId, MediaReferrer, MediaReferrerKind, NewOverlay, OverlayDefinition,
    OverlayId, ScriptRecord, SettingRow, StoredClip, TtsPipelineSettings,
};

#[derive(Debug, Clone)]
pub enum StoreChange {
    SaveQueue(Queue),
    DeleteQueue(QueueId),
    SaveScript(Box<ScriptRecord>),
    RestoreScript(ScriptId),
    DeleteScript(ScriptId),
    SetGlobal {
        name: String,
        value: Variant,
        persisted: bool,
    },
    DeleteGlobal(String),
    SaveClip(Box<StoredClip>),
    DeleteClip(ClipId),
    RetainMedia {
        referrer: MediaReferrer,
        blob: MediaBlobId,
    },
    ReleaseMedia(MediaReferrer),
    ReleaseAllMedia {
        kind: MediaReferrerKind,
        referrer_id: String,
    },
    CreateOverlay(Box<NewOverlay>),
    SaveOverlay(Box<OverlayDefinition>),
    DeleteOverlay(OverlayId),
    SaveTrigger(Box<TriggerInstance>),
    RestoreTrigger(TriggerInstanceId),
    DeleteTrigger(TriggerInstanceId),
    SaveAction(Box<Action>),
    RestoreAction(ActionId),
    DeleteAction(ActionId),
    LinkTrigger {
        action: ActionId,
        trigger: TriggerInstanceId,
    },
    UnlinkAllTriggers(ActionId),
    SetPipelineSettings(Box<TtsPipelineSettings>),
    ReplaceFilterRules(Vec<FilterRule>),
    UpsertVoiceAlias(Box<VoiceAlias>),
    DeleteVoiceAlias(AliasId),
    SetIgnoreProfile(Box<IgnoreProfile>),
    PutSetting(SettingRow),
    DeleteSetting(SettingRow),
}

impl StoreChange {
    pub fn touches_catalog(&self) -> bool {
        matches!(
            self,
            Self::SaveQueue(_)
                | Self::DeleteQueue(_)
                | Self::SaveScript(_)
                | Self::RestoreScript(_)
                | Self::DeleteScript(_)
                | Self::SaveTrigger(_)
                | Self::RestoreTrigger(_)
                | Self::DeleteTrigger(_)
                | Self::SaveAction(_)
                | Self::RestoreAction(_)
                | Self::DeleteAction(_)
                | Self::LinkTrigger { .. }
                | Self::UnlinkAllTriggers(_)
        )
    }

    pub fn removes_action(&self) -> bool {
        matches!(self, Self::DeleteAction(_))
    }

    pub fn changed_setting(&self) -> Option<&str> {
        match self {
            Self::PutSetting(row) | Self::DeleteSetting(row) => Some(row.key()),
            _ => None,
        }
    }
}
