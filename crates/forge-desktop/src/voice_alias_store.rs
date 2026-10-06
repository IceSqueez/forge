use forge_speak_queue::{SpeakCommand, SpeakQueueHandle};
use forge_storage::{AliasId, StorageError, VoiceAlias, VoiceAliasRepo};
use forge_voice::{AliasState, EngineId, VoiceId};

pub(crate) async fn save_alias(
    repo: &dyn VoiceAliasRepo,
    speak: Option<&SpeakQueueHandle>,
    alias: &VoiceAlias,
    replaces_existing: bool,
) -> Result<VoiceAlias, StorageError> {
    let stored = repo.upsert(alias).await?;
    if let Some(handle) = speak {
        if replaces_existing
            && let Err(e) = handle
                .send(SpeakCommand::RemoveAlias(stored.id.clone()))
                .await
        {
            tracing::warn!(error = %e, "voice alias hot-reload (replace) failed");
        }
        if let Err(e) = handle.send(SpeakCommand::SetAlias(stored.clone())).await {
            tracing::warn!(error = %e, "voice alias hot-reload failed");
        }
    }
    Ok(stored)
}

pub(crate) async fn block_viewer(
    repo: &dyn VoiceAliasRepo,
    speak: Option<&SpeakQueueHandle>,
    key: String,
    viewer_name: String,
) -> Result<VoiceAlias, StorageError> {
    let alias = match repo.find_by_viewer(&key).await? {
        Some(existing) => VoiceAlias {
            viewer_name,
            state: AliasState::Blocked,
            ..existing
        },
        None => VoiceAlias {
            id: AliasId::new(),
            viewer_id: key,
            viewer_name,
            engine_id: EngineId(String::new()),
            voice_id: VoiceId(String::new()),
            pitch_semitones: None,
            rate_multiplier: None,
            state: AliasState::Blocked,
        },
    };
    save_alias(repo, speak, &alias, false).await
}
