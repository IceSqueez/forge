use std::path::PathBuf;
use std::sync::Arc;

use forge_storage::DataProvider;
use forge_transfer::{SafetyCopyReport, TransferError, TransferExclusivity, prune_safety_copies};

pub(crate) fn spawn_boot_prune(
    backend: Arc<dyn DataProvider>,
    data_dir: PathBuf,
    exclusivity: TransferExclusivity,
) {
    let mut held = exclusivity.watch_held();
    tokio::spawn(async move {
        loop {
            match prune_safety_copies(backend.as_ref(), &data_dir, &exclusivity).await {
                Ok(report) if report.deferred_by_transfer => {
                    tracing::info!("safety copy pruning waits for the running transfer");
                    if held.changed().await.is_none() {
                        return;
                    }
                }
                outcome => {
                    log_outcome(outcome);
                    return;
                }
            }
        }
    });
}

fn log_outcome(outcome: Result<SafetyCopyReport, TransferError>) {
    match outcome {
        Ok(report) => {
            for failure in &report.failures {
                tracing::warn!(
                    path = %failure.path.display(),
                    reason = %failure.reason,
                    "could not delete a safety copy"
                );
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not prune safety copies"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use forge_storage::settings::core_settings::{
        SAFETY_COPIES_DELETE_AFTER_DAYS, SAFETY_COPIES_KEEP_NEWEST,
    };
    use forge_transfer::{backup_file_stem, backups_dir};
    use tempfile::TempDir;
    use time::OffsetDateTime;

    use super::*;
    use crate::test_support::{TestBackend, alive_tasks, pump_until, runtime, test_backend};

    const STEM_PREFIX: &str = "forge-backup";
    const PRE_PACK: &str = "forge-pre-pack";

    struct Boot {
        rt: tokio::runtime::Runtime,
        settings: Arc<TestBackend>,
        data_dir: TempDir,
        exclusivity: TransferExclusivity,
    }

    fn boot_keeping_one(copies: usize) -> Boot {
        let (settings, _writes) = test_backend();
        settings.seed(&SAFETY_COPIES_KEEP_NEWEST, &1);
        let boot = Boot {
            rt: runtime(),
            settings,
            data_dir: tempfile::tempdir().unwrap(),
            exclusivity: TransferExclusivity::new(),
        };
        boot.add_copies(copies);
        boot
    }

    impl Boot {
        fn backups(&self) -> PathBuf {
            backups_dir(self.data_dir.path())
        }

        fn add_copies(&self, count: usize) {
            std::fs::create_dir_all(self.backups()).unwrap();
            for n in 0..count {
                let age = time::Duration::hours(i64::try_from(n).unwrap() + 1);
                let stem = backup_file_stem(OffsetDateTime::now_utc() - age, &format!("a{n:05}"));
                let name = format!(
                    "{PRE_PACK}{}.tar.gz",
                    stem.strip_prefix(STEM_PREFIX).unwrap()
                );
                std::fs::write(self.backups().join(name), b"copy").unwrap();
            }
        }

        fn copies(&self) -> usize {
            copies_in(&self.backups())
        }

        fn spawn(&self) {
            let _entered = self.rt.enter();
            spawn_boot_prune(
                Arc::clone(&self.settings) as Arc<dyn DataProvider>,
                self.data_dir.path().to_path_buf(),
                self.exclusivity.clone(),
            );
        }

        fn prune_attempts(&self) -> usize {
            self.settings.reads_of(&SAFETY_COPIES_DELETE_AFTER_DAYS)
        }

        fn await_prune_attempt(&self, attempt: usize) {
            pump_until(&self.rt, || self.prune_attempts() == attempt);
        }

        fn await_copies(&self, expected: usize) {
            pump_until(&self.rt, || self.copies() == expected);
        }
    }

    fn copies_in(backups: &Path) -> usize {
        std::fs::read_dir(backups)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(PRE_PACK)
            })
            .count()
    }

    #[test]
    fn a_free_lock_trims_at_boot_at_once() {
        let boot = boot_keeping_one(3);

        boot.spawn();

        boot.await_copies(1);
    }

    #[test]
    fn a_boot_prune_blocked_by_a_transfer_trims_once_the_transfer_ends() {
        let boot = boot_keeping_one(3);
        let running = boot.exclusivity.try_acquire().unwrap();
        boot.spawn();
        boot.await_prune_attempt(1);
        let untouched_while_held = boot.copies();

        drop(running);
        boot.await_prune_attempt(2);

        assert_eq!(untouched_while_held, 3);
        boot.await_copies(1);
    }

    #[test]
    fn a_boot_prune_waits_out_several_transfers_in_a_row() {
        let boot = boot_keeping_one(3);
        let first = boot.exclusivity.try_acquire().unwrap();
        boot.spawn();
        boot.await_prune_attempt(1);
        drop(first);
        let second = boot.exclusivity.try_acquire().unwrap();
        boot.await_prune_attempt(2);
        let untouched_through_the_handover = boot.copies();

        drop(second);
        boot.await_prune_attempt(3);

        assert_eq!(untouched_through_the_handover, 3);
        boot.await_copies(1);
    }

    #[test]
    fn the_boot_prune_stops_after_its_one_real_run() {
        let boot = boot_keeping_one(3);
        let background_tasks = alive_tasks(&boot.rt);

        boot.spawn();
        pump_until(&boot.rt, || alive_tasks(&boot.rt) == background_tasks);

        assert_eq!((boot.copies(), boot.prune_attempts()), (1, 1));
    }
}
