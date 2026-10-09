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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::time::Duration;

    use forge_speak_queue::SpeakCommand;
    use forge_storage::voice_aliases::MockVoiceAliasRepo;
    use forge_storage::{DataProvider, StorageError, VoiceAlias};
    use forge_voice::{AliasId, AliasState, EngineId, VoiceId};

    use super::{block_viewer, save_alias};
    use crate::test_support::{
        AliasShape, LiveAliases, alias_shape, live_alias, sandboxed_backend, spawn_speak_queue,
    };

    const TEST_KEY: [u8; 32] = [0x3c; 32];
    const QUEUE_DEADLINE: Duration = Duration::from_secs(30);

    fn alias(id: &str, viewer_id: &str, viewer_name: &str) -> VoiceAlias {
        VoiceAlias {
            id: AliasId(id.to_owned()),
            viewer_id: viewer_id.to_owned(),
            viewer_name: viewer_name.to_owned(),
            engine_id: EngineId("piper".to_owned()),
            voice_id: VoiceId("amy".to_owned()),
            pitch_semitones: Some(2.0),
            rate_multiplier: Some(0.8),
            state: AliasState::Active,
        }
    }

    fn shapes(aliases: Vec<VoiceAlias>) -> Vec<AliasShape> {
        aliases.iter().map(alias_shape).collect()
    }

    async fn applied(resolver: &LiveAliases, done: impl Fn(&LiveAliases) -> bool) {
        tokio::time::timeout(QUEUE_DEADLINE, async {
            while !done(resolver) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the speak queue never applied the alias");
    }

    #[tokio::test]
    async fn blocking_a_viewer_without_an_alias_stores_and_hot_reloads_one_blocked_alias() {
        let backend = sandboxed_backend("sqlite::memory:", TEST_KEY).await;
        let repo = backend.voice_alias_repo();
        let (speak, _events, resolver) = spawn_speak_queue(Vec::new());

        let stored = block_viewer(
            repo.as_ref(),
            Some(&speak),
            "twitch:111".to_owned(),
            "alice".to_owned(),
        )
        .await
        .expect("block");
        applied(&resolver, |r| live_alias(r, "twitch:111").is_some()).await;

        let stored = alias_shape(&stored);
        assert_eq!(
            (
                (stored.1.as_str(), stored.2.as_str(), stored.7.clone()),
                shapes(repo.list().await.expect("list")),
                live_alias(&resolver, "twitch:111").map(|live| alias_shape(&live)),
            ),
            (
                ("twitch:111", "alice", AliasState::Blocked),
                vec![stored.clone()],
                Some(stored.clone()),
            )
        );
    }

    #[tokio::test]
    async fn blocking_a_viewer_with_an_alias_keeps_its_id_and_voice_and_takes_the_new_name() {
        let backend = sandboxed_backend("sqlite::memory:", TEST_KEY).await;
        let repo = backend.voice_alias_repo();
        let existing = alias("a1", "twitch:111", "old_name");
        repo.upsert(&existing).await.expect("seed");

        for _ in 0..2 {
            block_viewer(
                repo.as_ref(),
                None,
                "twitch:111".to_owned(),
                "alice".to_owned(),
            )
            .await
            .expect("block");
        }

        assert_eq!(
            shapes(repo.list().await.expect("list")),
            vec![alias_shape(&VoiceAlias {
                viewer_name: "alice".to_owned(),
                state: AliasState::Blocked,
                ..existing
            })]
        );
    }

    #[tokio::test]
    async fn saving_drops_the_previous_live_entry_only_when_it_replaces_an_edited_alias() {
        for (replaces_existing, old_entry_still_live) in [(true, false), (false, true)] {
            let backend = sandboxed_backend("sqlite::memory:", TEST_KEY).await;
            let repo = backend.voice_alias_repo();
            let existing = alias("a1", "twitch:alice", "alice");
            repo.upsert(&existing).await.expect("seed");
            let (speak, _events, resolver) = spawn_speak_queue(vec![existing]);

            save_alias(
                repo.as_ref(),
                Some(&speak),
                &alias("a1", "twitch:bob", "bob"),
                replaces_existing,
            )
            .await
            .expect("save");
            applied(&resolver, |r| live_alias(r, "twitch:bob").is_some()).await;

            assert_eq!(
                live_alias(&resolver, "twitch:alice").is_some(),
                old_entry_still_live,
                "replaces_existing = {replaces_existing}"
            );
        }
    }

    #[tokio::test]
    async fn a_rejected_save_returns_the_storage_error_and_reloads_nothing() {
        let mut repo = MockVoiceAliasRepo::new();
        repo.expect_upsert()
            .returning(|_| Err(StorageError::AliasViewerTaken));
        let (speak, _events, resolver) = spawn_speak_queue(Vec::new());

        let outcome =
            save_alias(&repo, Some(&speak), &alias("a1", "twitch:bob", "bob"), true).await;
        speak
            .send(SpeakCommand::SetAlias(alias(
                "barrier", "barrier", "barrier",
            )))
            .await
            .expect("queue alive");
        applied(&resolver, |r| live_alias(r, "barrier").is_some()).await;

        assert!(matches!(outcome, Err(StorageError::AliasViewerTaken)));
        assert!(live_alias(&resolver, "twitch:bob").is_none());
    }

    #[tokio::test]
    async fn blocking_stops_at_a_failed_lookup_without_writing() {
        let mut repo = MockVoiceAliasRepo::new();
        repo.expect_find_by_viewer().returning(|_| {
            Err(StorageError::Connection {
                reason: "closed".to_owned(),
            })
        });
        repo.expect_upsert().never();

        let outcome = block_viewer(&repo, None, "twitch:111".to_owned(), "alice".to_owned()).await;

        assert!(matches!(outcome, Err(StorageError::Connection { .. })));
    }
}
