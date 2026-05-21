use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::Event;
use forge_storage::{
    ActionRepo, AliasId, AssignmentStrategy, CommandRepo, CredentialId, CredentialsRepo,
    DataProvider, EventLogRepo, GlobalEntry, GlobalsRepo, HistoryRepo, IgnoreProfile, QueueRepo,
    ScriptRecord, ScriptRepo, SettingsRepo, SoundboardClipsRepo, StorageError, StoredClip,
    TriggerRepo, UserGlobalEntry, UserGlobalsRepo, Viewer, ViewerPlatform, ViewerRepo, VoiceAlias,
    VoiceAliasRepo,
};
use forge_types::{
    Action, ActionId, ClipId, Command, CommandId, ExecutionContext, Queue, QueueId, ScriptId,
    Trigger, TriggerId, Variant,
};
use time::OffsetDateTime;

pub struct NullDp;

#[async_trait]
impl GlobalsRepo for NullDp {
    async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _name: &str,
        _value: Variant,
        _persisted: bool,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _name: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn incr(&self, _name: &str, _amount: i64) -> Result<Variant, StorageError> {
        Err(StorageError::NotReady)
    }
}

#[async_trait]
impl UserGlobalsRepo for NullDp {
    async fn get(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
        _value: Variant,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_for_user(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn list_for_broadcaster(
        &self,
        _broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl SettingsRepo for NullDp {
    async fn get_string(&self, _key: &str) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn set_string(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        Ok(HashMap::new())
    }
}

#[async_trait]
impl ScriptRepo for NullDp {
    async fn get(&self, _id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _record: ScriptRecord) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ScriptId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl CredentialsRepo for NullDp {
    async fn store(&self, _id: &CredentialId, _plaintext_bundle: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn load(&self, _id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn delete(&self, _id: &CredentialId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(vec![])
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

#[async_trait]
impl ActionRepo for NullDp {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _action: &Action) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_by_group(&self, _group: Option<&str>) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl TriggerRepo for NullDp {
    async fn list_for_action(&self, _action_id: ActionId) -> Result<Vec<Trigger>, StorageError> {
        Ok(vec![])
    }

    async fn save(&self, _trigger: &Trigger) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: TriggerId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl CommandRepo for NullDp {
    async fn list(&self) -> Result<Vec<Command>, StorageError> {
        Ok(vec![])
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<Command>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _command: &Command) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: CommandId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl QueueRepo for NullDp {
    async fn list(&self) -> Result<Vec<Queue>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: QueueId) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _queue: &Queue) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: QueueId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl HistoryRepo for NullDp {
    async fn save(&self, _ctx: &ExecutionContext) -> Result<(), StorageError> {
        Ok(())
    }

    async fn recent_for_action(
        &self,
        _action_id: ActionId,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(vec![])
    }

    async fn stats_summary(
        &self,
        _since: time::OffsetDateTime,
    ) -> Result<std::collections::HashMap<ActionId, forge_storage::ActionStats>, StorageError> {
        Ok(std::collections::HashMap::new())
    }
}

#[async_trait]
impl EventLogRepo for NullDp {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _id: forge_types::EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<forge_types::EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

#[async_trait]
impl DataProvider for NullDp {
    fn action_repo(&self) -> &dyn ActionRepo {
        self
    }

    fn trigger_repo(&self) -> &dyn TriggerRepo {
        self
    }

    fn command_repo(&self) -> &dyn CommandRepo {
        self
    }

    fn queue_repo(&self) -> &dyn QueueRepo {
        self
    }

    fn history_repo(&self) -> &dyn HistoryRepo {
        self
    }

    fn event_log_repo(&self) -> &dyn EventLogRepo {
        self
    }

    fn soundboard_clips_repo(&self) -> &dyn SoundboardClipsRepo {
        self
    }

    fn voice_alias_repo(&self) -> &dyn VoiceAliasRepo {
        self
    }

    fn viewer_repo(&self) -> &dyn ViewerRepo {
        self
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(0)
    }

    async fn export(&self, _path: &Path) -> Result<(), StorageError> {
        Ok(())
    }
}

pub fn null_dp() -> Arc<dyn DataProvider> {
    Arc::new(NullDp)
}

pub fn null_creds() -> Arc<dyn forge_storage::CredentialsRepo> {
    Arc::new(NullDp)
}

macro_rules! impl_null_soundboard {
    ($t:ty) => {
        #[async_trait]
        impl SoundboardClipsRepo for $t {
            async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
                Ok(vec![])
            }
            async fn get(&self, _id: ClipId) -> Result<Option<StoredClip>, StorageError> {
                Ok(None)
            }
            async fn save(&self, _clip: &StoredClip) -> Result<(), StorageError> {
                Ok(())
            }
            async fn delete(&self, _id: ClipId) -> Result<bool, StorageError> {
                Ok(false)
            }
        }
    };
}

macro_rules! impl_null_voice_alias {
    ($t:ty) => {
        #[async_trait]
        impl VoiceAliasRepo for $t {
            async fn list(&self) -> Result<Vec<VoiceAlias>, StorageError> {
                Ok(vec![])
            }
            async fn upsert(&self, _alias: &VoiceAlias) -> Result<(), StorageError> {
                Ok(())
            }
            async fn delete(&self, _id: &AliasId) -> Result<(), StorageError> {
                Ok(())
            }
            async fn find_by_viewer(
                &self,
                _viewer_id: &str,
            ) -> Result<Option<VoiceAlias>, StorageError> {
                Ok(None)
            }
            async fn get_strategy(&self) -> Result<AssignmentStrategy, StorageError> {
                Ok(AssignmentStrategy::default())
            }
            async fn set_strategy(
                &self,
                _strategy: &AssignmentStrategy,
            ) -> Result<(), StorageError> {
                Ok(())
            }
            async fn get_ignore_profile(&self) -> Result<IgnoreProfile, StorageError> {
                Ok(IgnoreProfile::default())
            }
            async fn set_ignore_profile(
                &self,
                _profile: &IgnoreProfile,
            ) -> Result<(), StorageError> {
                Ok(())
            }
        }
    };
}

macro_rules! impl_null_viewer {
    ($t:ty) => {
        #[async_trait]
        impl ViewerRepo for $t {
            async fn list(&self) -> Result<Vec<Viewer>, StorageError> {
                Ok(vec![])
            }
            async fn get(
                &self,
                _platform: ViewerPlatform,
                _viewer_id: &str,
            ) -> Result<Option<Viewer>, StorageError> {
                Ok(None)
            }
            async fn record_message(
                &self,
                _platform: ViewerPlatform,
                _viewer_id: &str,
                _username: &str,
            ) -> Result<(), StorageError> {
                Ok(())
            }
            async fn set_custom_greeting(
                &self,
                _platform: ViewerPlatform,
                _viewer_id: &str,
                _enabled: bool,
            ) -> Result<bool, StorageError> {
                Ok(false)
            }
        }
    };
}

impl_null_soundboard!(NullDp);
impl_null_voice_alias!(NullDp);
impl_null_viewer!(NullDp);

impl_null_soundboard!(VecCommandDp);
impl_null_voice_alias!(VecCommandDp);
impl_null_viewer!(VecCommandDp);

pub struct VecCommandDp {
    commands: Vec<forge_types::Command>,
}

impl VecCommandDp {
    pub fn with_commands(commands: Vec<forge_types::Command>) -> Arc<dyn DataProvider> {
        Arc::new(Self { commands })
    }
}

#[async_trait]
impl GlobalsRepo for VecCommandDp {
    async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _name: &str,
        _value: Variant,
        _persisted: bool,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _name: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn incr(&self, _name: &str, _amount: i64) -> Result<Variant, StorageError> {
        Err(StorageError::NotReady)
    }
}

#[async_trait]
impl UserGlobalsRepo for VecCommandDp {
    async fn get(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
        _value: Variant,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_for_user(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn list_for_broadcaster(
        &self,
        _broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl SettingsRepo for VecCommandDp {
    async fn get_string(&self, _key: &str) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn set_string(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        Ok(HashMap::new())
    }
}

#[async_trait]
impl ScriptRepo for VecCommandDp {
    async fn get(&self, _id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _record: ScriptRecord) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ScriptId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl CredentialsRepo for VecCommandDp {
    async fn store(&self, _id: &CredentialId, _plaintext_bundle: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn load(&self, _id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn delete(&self, _id: &CredentialId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(vec![])
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

#[async_trait]
impl ActionRepo for VecCommandDp {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _action: &Action) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_by_group(&self, _group: Option<&str>) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl TriggerRepo for VecCommandDp {
    async fn list_for_action(&self, _action_id: ActionId) -> Result<Vec<Trigger>, StorageError> {
        Ok(vec![])
    }

    async fn save(&self, _trigger: &Trigger) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: TriggerId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl CommandRepo for VecCommandDp {
    async fn list(&self) -> Result<Vec<forge_types::Command>, StorageError> {
        Ok(self.commands.clone())
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<forge_types::Command>, StorageError> {
        Ok(self.commands.iter().find(|c| c.name == name).cloned())
    }

    async fn save(&self, _command: &forge_types::Command) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: CommandId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl QueueRepo for VecCommandDp {
    async fn list(&self) -> Result<Vec<Queue>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: QueueId) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _queue: &Queue) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: QueueId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl HistoryRepo for VecCommandDp {
    async fn save(&self, _ctx: &ExecutionContext) -> Result<(), StorageError> {
        Ok(())
    }

    async fn recent_for_action(
        &self,
        _action_id: ActionId,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(vec![])
    }

    async fn stats_summary(
        &self,
        _since: time::OffsetDateTime,
    ) -> Result<std::collections::HashMap<ActionId, forge_storage::ActionStats>, StorageError> {
        Ok(std::collections::HashMap::new())
    }
}

#[async_trait]
impl EventLogRepo for VecCommandDp {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _id: forge_types::EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<forge_types::EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

#[async_trait]
impl DataProvider for VecCommandDp {
    fn action_repo(&self) -> &dyn ActionRepo {
        self
    }

    fn trigger_repo(&self) -> &dyn TriggerRepo {
        self
    }

    fn command_repo(&self) -> &dyn CommandRepo {
        self
    }

    fn queue_repo(&self) -> &dyn QueueRepo {
        self
    }

    fn history_repo(&self) -> &dyn HistoryRepo {
        self
    }

    fn event_log_repo(&self) -> &dyn EventLogRepo {
        self
    }

    fn soundboard_clips_repo(&self) -> &dyn SoundboardClipsRepo {
        self
    }

    fn voice_alias_repo(&self) -> &dyn VoiceAliasRepo {
        self
    }

    fn viewer_repo(&self) -> &dyn ViewerRepo {
        self
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(0)
    }

    async fn export(&self, _path: &Path) -> Result<(), StorageError> {
        Ok(())
    }
}

impl_null_soundboard!(VecGlobalsDp);
impl_null_voice_alias!(VecGlobalsDp);
impl_null_viewer!(VecGlobalsDp);

pub struct VecGlobalsDp {
    entries: Vec<GlobalEntry>,
}

impl VecGlobalsDp {
    pub fn with_globals(entries: Vec<GlobalEntry>) -> Arc<dyn DataProvider> {
        Arc::new(Self { entries })
    }
}

#[async_trait]
impl GlobalsRepo for VecGlobalsDp {
    async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
        Ok(self
            .entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.value.clone()))
    }

    async fn set(
        &self,
        _name: &str,
        _value: Variant,
        _persisted: bool,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _name: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(self.entries.clone())
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn incr(&self, _name: &str, _amount: i64) -> Result<Variant, StorageError> {
        Err(StorageError::NotReady)
    }
}

#[async_trait]
impl UserGlobalsRepo for VecGlobalsDp {
    async fn get(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
        _value: Variant,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_for_user(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn list_for_broadcaster(
        &self,
        _broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl SettingsRepo for VecGlobalsDp {
    async fn get_string(&self, _key: &str) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn set_string(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        Ok(HashMap::new())
    }
}

#[async_trait]
impl ScriptRepo for VecGlobalsDp {
    async fn get(&self, _id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _record: ScriptRecord) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ScriptId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl CredentialsRepo for VecGlobalsDp {
    async fn store(&self, _id: &CredentialId, _plaintext_bundle: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn load(&self, _id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn delete(&self, _id: &CredentialId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(vec![])
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

#[async_trait]
impl ActionRepo for VecGlobalsDp {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _action: &Action) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_by_group(&self, _group: Option<&str>) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl TriggerRepo for VecGlobalsDp {
    async fn list_for_action(&self, _action_id: ActionId) -> Result<Vec<Trigger>, StorageError> {
        Ok(vec![])
    }

    async fn save(&self, _trigger: &Trigger) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: TriggerId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl CommandRepo for VecGlobalsDp {
    async fn list(&self) -> Result<Vec<forge_types::Command>, StorageError> {
        Ok(vec![])
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<forge_types::Command>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _command: &forge_types::Command) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: CommandId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl QueueRepo for VecGlobalsDp {
    async fn list(&self) -> Result<Vec<Queue>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: QueueId) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _queue: &Queue) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: QueueId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl HistoryRepo for VecGlobalsDp {
    async fn save(&self, _ctx: &ExecutionContext) -> Result<(), StorageError> {
        Ok(())
    }

    async fn recent_for_action(
        &self,
        _action_id: ActionId,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(vec![])
    }

    async fn stats_summary(
        &self,
        _since: time::OffsetDateTime,
    ) -> Result<std::collections::HashMap<ActionId, forge_storage::ActionStats>, StorageError> {
        Ok(std::collections::HashMap::new())
    }
}

#[async_trait]
impl EventLogRepo for VecGlobalsDp {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _id: forge_types::EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<forge_types::EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

#[async_trait]
impl DataProvider for VecGlobalsDp {
    fn action_repo(&self) -> &dyn ActionRepo {
        self
    }

    fn trigger_repo(&self) -> &dyn TriggerRepo {
        self
    }

    fn command_repo(&self) -> &dyn CommandRepo {
        self
    }

    fn queue_repo(&self) -> &dyn QueueRepo {
        self
    }

    fn history_repo(&self) -> &dyn HistoryRepo {
        self
    }

    fn event_log_repo(&self) -> &dyn EventLogRepo {
        self
    }

    fn soundboard_clips_repo(&self) -> &dyn SoundboardClipsRepo {
        self
    }

    fn voice_alias_repo(&self) -> &dyn VoiceAliasRepo {
        self
    }

    fn viewer_repo(&self) -> &dyn ViewerRepo {
        self
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(0)
    }

    async fn export(&self, _path: &Path) -> Result<(), StorageError> {
        Ok(())
    }
}

impl_null_soundboard!(VecActionDp);
impl_null_voice_alias!(VecActionDp);
impl_null_viewer!(VecActionDp);

pub struct VecActionDp {
    actions: Vec<Action>,
}

impl VecActionDp {
    pub fn with_actions(actions: Vec<Action>) -> Arc<dyn DataProvider> {
        Arc::new(Self { actions })
    }
}

#[async_trait]
impl GlobalsRepo for VecActionDp {
    async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _name: &str,
        _value: Variant,
        _persisted: bool,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _name: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn incr(&self, _name: &str, _amount: i64) -> Result<Variant, StorageError> {
        Err(StorageError::NotReady)
    }
}

#[async_trait]
impl UserGlobalsRepo for VecActionDp {
    async fn get(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
        _value: Variant,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_for_user(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn list_for_broadcaster(
        &self,
        _broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl SettingsRepo for VecActionDp {
    async fn get_string(&self, _key: &str) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn set_string(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        Ok(HashMap::new())
    }
}

#[async_trait]
impl ScriptRepo for VecActionDp {
    async fn get(&self, _id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _record: ScriptRecord) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ScriptId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl CredentialsRepo for VecActionDp {
    async fn store(&self, _id: &CredentialId, _plaintext_bundle: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn load(&self, _id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn delete(&self, _id: &CredentialId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(vec![])
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

#[async_trait]
impl ActionRepo for VecActionDp {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(self.actions.clone())
    }

    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(self.actions.iter().find(|a| a.id == id).cloned())
    }

    async fn save(&self, _action: &Action) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_by_group(&self, _group: Option<&str>) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl TriggerRepo for VecActionDp {
    async fn list_for_action(&self, _action_id: ActionId) -> Result<Vec<Trigger>, StorageError> {
        Ok(vec![])
    }

    async fn save(&self, _trigger: &Trigger) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: TriggerId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl CommandRepo for VecActionDp {
    async fn list(&self) -> Result<Vec<Command>, StorageError> {
        Ok(vec![])
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<Command>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _command: &Command) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: CommandId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl QueueRepo for VecActionDp {
    async fn list(&self) -> Result<Vec<Queue>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: QueueId) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _queue: &Queue) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: QueueId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl HistoryRepo for VecActionDp {
    async fn save(&self, _ctx: &ExecutionContext) -> Result<(), StorageError> {
        Ok(())
    }

    async fn recent_for_action(
        &self,
        _action_id: ActionId,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(vec![])
    }

    async fn stats_summary(
        &self,
        _since: time::OffsetDateTime,
    ) -> Result<std::collections::HashMap<ActionId, forge_storage::ActionStats>, StorageError> {
        Ok(std::collections::HashMap::new())
    }
}

#[async_trait]
impl EventLogRepo for VecActionDp {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _id: forge_types::EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<forge_types::EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

#[async_trait]
impl DataProvider for VecActionDp {
    fn action_repo(&self) -> &dyn ActionRepo {
        self
    }

    fn trigger_repo(&self) -> &dyn TriggerRepo {
        self
    }

    fn command_repo(&self) -> &dyn CommandRepo {
        self
    }

    fn queue_repo(&self) -> &dyn QueueRepo {
        self
    }

    fn history_repo(&self) -> &dyn HistoryRepo {
        self
    }

    fn event_log_repo(&self) -> &dyn EventLogRepo {
        self
    }

    fn soundboard_clips_repo(&self) -> &dyn SoundboardClipsRepo {
        self
    }

    fn voice_alias_repo(&self) -> &dyn VoiceAliasRepo {
        self
    }

    fn viewer_repo(&self) -> &dyn ViewerRepo {
        self
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(0)
    }

    async fn export(&self, _path: &Path) -> Result<(), StorageError> {
        Ok(())
    }
}

impl_null_soundboard!(VecUserGlobalsDp);
impl_null_voice_alias!(VecUserGlobalsDp);
impl_null_viewer!(VecUserGlobalsDp);

pub struct VecUserGlobalsDp {
    entries: Vec<UserGlobalEntry>,
}

impl VecUserGlobalsDp {
    pub fn with_entries(entries: Vec<UserGlobalEntry>) -> Arc<dyn DataProvider> {
        Arc::new(Self { entries })
    }
}

#[async_trait]
impl GlobalsRepo for VecUserGlobalsDp {
    async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _name: &str,
        _value: Variant,
        _persisted: bool,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _name: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn incr(&self, _name: &str, _amount: i64) -> Result<Variant, StorageError> {
        Err(StorageError::NotReady)
    }
}

#[async_trait]
impl UserGlobalsRepo for VecUserGlobalsDp {
    async fn get(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        Ok(self
            .entries
            .iter()
            .find(|e| e.broadcaster_id == broadcaster_id && e.user_id == user_id && e.name == name)
            .map(|e| e.value.clone()))
    }

    async fn set(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
        _value: Variant,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_for_user(
        &self,
        broadcaster_id: &str,
        user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(self
            .entries
            .iter()
            .filter(|e| e.broadcaster_id == broadcaster_id && e.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn list_for_broadcaster(
        &self,
        broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(self
            .entries
            .iter()
            .filter(|e| e.broadcaster_id == broadcaster_id)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl SettingsRepo for VecUserGlobalsDp {
    async fn get_string(&self, _key: &str) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn set_string(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        Ok(HashMap::new())
    }
}

#[async_trait]
impl ScriptRepo for VecUserGlobalsDp {
    async fn get(&self, _id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _record: ScriptRecord) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ScriptId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl CredentialsRepo for VecUserGlobalsDp {
    async fn store(&self, _id: &CredentialId, _plaintext_bundle: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn load(&self, _id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn delete(&self, _id: &CredentialId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(vec![])
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

#[async_trait]
impl ActionRepo for VecUserGlobalsDp {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _action: &Action) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_by_group(&self, _group: Option<&str>) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl TriggerRepo for VecUserGlobalsDp {
    async fn list_for_action(&self, _action_id: ActionId) -> Result<Vec<Trigger>, StorageError> {
        Ok(vec![])
    }

    async fn save(&self, _trigger: &Trigger) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: TriggerId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl CommandRepo for VecUserGlobalsDp {
    async fn list(&self) -> Result<Vec<forge_types::Command>, StorageError> {
        Ok(vec![])
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<forge_types::Command>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _command: &forge_types::Command) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: CommandId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl QueueRepo for VecUserGlobalsDp {
    async fn list(&self) -> Result<Vec<Queue>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: QueueId) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _queue: &Queue) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: QueueId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl HistoryRepo for VecUserGlobalsDp {
    async fn save(&self, _ctx: &ExecutionContext) -> Result<(), StorageError> {
        Ok(())
    }

    async fn recent_for_action(
        &self,
        _action_id: ActionId,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(vec![])
    }

    async fn stats_summary(
        &self,
        _since: time::OffsetDateTime,
    ) -> Result<std::collections::HashMap<ActionId, forge_storage::ActionStats>, StorageError> {
        Ok(std::collections::HashMap::new())
    }
}

#[async_trait]
impl EventLogRepo for VecUserGlobalsDp {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _id: forge_types::EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<forge_types::EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

#[async_trait]
impl DataProvider for VecUserGlobalsDp {
    fn action_repo(&self) -> &dyn ActionRepo {
        self
    }

    fn trigger_repo(&self) -> &dyn TriggerRepo {
        self
    }

    fn command_repo(&self) -> &dyn CommandRepo {
        self
    }

    fn queue_repo(&self) -> &dyn QueueRepo {
        self
    }

    fn history_repo(&self) -> &dyn HistoryRepo {
        self
    }

    fn event_log_repo(&self) -> &dyn EventLogRepo {
        self
    }

    fn soundboard_clips_repo(&self) -> &dyn SoundboardClipsRepo {
        self
    }

    fn voice_alias_repo(&self) -> &dyn VoiceAliasRepo {
        self
    }

    fn viewer_repo(&self) -> &dyn ViewerRepo {
        self
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(0)
    }

    async fn export(&self, _path: &Path) -> Result<(), StorageError> {
        Ok(())
    }
}
