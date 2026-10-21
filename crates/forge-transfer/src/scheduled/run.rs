use forge_storage::SettingsRepo;

use crate::cancel::CancelFlag;
use crate::full_writer::{
    ScheduledBackupRequest, ScheduledWrite, write_scheduled_backup_cancellable,
};
use crate::scheduled::destination::resolve_folder;
use crate::scheduled::prune::prune;
use crate::scheduled::service::{Service, Trigger};
use crate::scheduled::status::{FailureCategory, FailureStreak};

struct Completed {
    prune_failures: u32,
}

impl Service {
    pub(crate) async fn attempt(&mut self, trigger: Trigger) {
        let Some(guard) = self.parts.exclusivity.try_acquire() else {
            if matches!(trigger, Trigger::Due) {
                self.defer();
            }
            return;
        };
        self.running = true;
        self.publish();
        let outcome = self.execute(trigger).await;
        drop(guard);
        self.running = false;
        match outcome {
            Ok(completed) => self.record_success(completed),
            Err(category) => self.record_failure(category),
        }
        self.refresh_storage().await;
        self.publish();
    }

    async fn execute(&mut self, trigger: Trigger) -> Result<Completed, FailureCategory> {
        if self.settings.key_unreadable {
            return Err(FailureCategory::KeyUnreadable);
        }
        let key = self
            .settings
            .key
            .clone()
            .ok_or(FailureCategory::KeyMissing)?;
        let folder =
            resolve_folder(&self.parts.data_dir, self.settings.destination.as_deref()).await?;
        let unchanged_since = match trigger {
            Trigger::Due => self.progress.fingerprint.clone(),
            Trigger::Manual => None,
        };
        let key_fingerprint = key.fingerprint();
        let request = ScheduledBackupRequest {
            provider: self.parts.provider.clone(),
            data_dir: self.parts.data_dir.clone(),
            folder: folder.clone(),
            key,
            categories: self.settings.categories.clone(),
        };
        let signals = std::sync::Arc::clone(&self.signals);
        let cancel = CancelFlag::new();
        let write = write_scheduled_backup_cancellable(request, unchanged_since, cancel.clone());
        tokio::pin!(write);
        let written = tokio::select! {
            result = &mut write => result,
            () = signals.shutdown.cancelled() => {
                cancel.raise();
                write.await
            }
            () = signals.cancel_in_flight.notified() => {
                cancel.raise();
                write.await
            }
        }
        .map_err(|error| FailureCategory::of(&error))?;
        let (fingerprint, wrote) = match written {
            ScheduledWrite::Written { fingerprint, .. } => (fingerprint, true),
            ScheduledWrite::Unchanged { fingerprint } => (fingerprint, false),
        };
        self.progress.last_success_at = Some(self.parts.clock.now());
        self.progress.fingerprint = Some(fingerprint);
        let repo: &dyn SettingsRepo = self.parts.provider.as_ref();
        self.progress
            .save(repo)
            .await
            .map_err(|_| FailureCategory::StateNotSaved)?;
        let prune_failures = if wrote {
            let now = self.parts.clock.now();
            prune(
                &folder,
                &key_fingerprint,
                self.settings.retention,
                now,
                self.parts.clock.offset_at(now),
            )
            .await
            .failed
        } else {
            0
        };
        Ok(Completed { prune_failures })
    }

    fn record_success(&mut self, completed: Completed) {
        self.streak = None;
        self.retry_after = None;
        self.prune_failures = completed.prune_failures;
    }

    fn record_failure(&mut self, category: FailureCategory) {
        let now = self.parts.clock.now();
        self.retry_after = Some(now + self.settings.interval());
        if category == FailureCategory::Interrupted {
            return;
        }
        self.streak = Some(match self.streak {
            Some(streak) => FailureStreak {
                category,
                since: streak.since,
                attempts: streak.attempts + 1,
            },
            None => FailureStreak {
                category,
                since: now,
                attempts: 1,
            },
        });
    }
}
