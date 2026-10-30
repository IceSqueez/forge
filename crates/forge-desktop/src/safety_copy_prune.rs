use std::path::PathBuf;
use std::sync::Arc;

use forge_storage::DataProvider;
use forge_transfer::{SafetyCopyReport, TransferError, TransferExclusivity, prune_safety_copies};

pub fn spawn_boot_prune(
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
    use std::time::{Duration, Instant};

    use forge_storage::settings::core_settings::SAFETY_COPIES_KEEP_NEWEST;
    use forge_storage::write_setting;
    use forge_transfer::{backup_file_stem, backups_dir};
    use time::OffsetDateTime;

    use super::*;
    use crate::test_support::pack_flow::{PackWorld, pack_world};
    use crate::test_support::runtime;

    const SETTLE_GUARD: Duration = Duration::from_secs(30);
    const RUNTIME_TURN: Duration = Duration::from_millis(1);
    const QUIET_TURNS: usize = 50;
    const STEM_PREFIX: &str = "forge-backup";
    const PRE_PACK: &str = "forge-pre-pack";

    struct Boot {
        rt: tokio::runtime::Runtime,
        world: PackWorld,
        exclusivity: TransferExclusivity,
    }

    fn boot_keeping_one(copies: usize) -> Boot {
        let rt = runtime();
        let world = pack_world(&rt);
        rt.block_on(write_setting(
            world.provider().as_ref(),
            &SAFETY_COPIES_KEEP_NEWEST,
            &1,
        ))
        .unwrap();
        let boot = Boot {
            rt,
            world,
            exclusivity: TransferExclusivity::new(),
        };
        boot.add_copies(copies, "a");
        boot
    }

    impl Boot {
        fn backups(&self) -> PathBuf {
            backups_dir(&self.world.data_dir())
        }

        fn add_copies(&self, count: usize, tag: &str) {
            std::fs::create_dir_all(self.backups()).unwrap();
            for n in 0..count {
                let age = time::Duration::hours(i64::try_from(n).unwrap() + 1);
                let stem =
                    backup_file_stem(OffsetDateTime::now_utc() - age, &format!("{tag}{n:05}"));
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
                Arc::clone(self.world.provider()),
                self.world.data_dir(),
                self.exclusivity.clone(),
            );
        }

        fn turn(&self) {
            self.rt
                .block_on(async { tokio::time::sleep(RUNTIME_TURN).await });
        }

        fn settles_at(&self, expected: usize) -> bool {
            let started = Instant::now();
            while self.copies() != expected {
                if started.elapsed() > SETTLE_GUARD {
                    return false;
                }
                self.turn();
            }
            true
        }

        fn stays_at(&self, expected: usize) -> bool {
            (0..QUIET_TURNS).all(|_| {
                self.turn();
                self.copies() == expected
            })
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

        assert!(boot.settles_at(1));
    }

    #[test]
    fn a_boot_prune_blocked_by_a_transfer_trims_once_the_transfer_ends() {
        let boot = boot_keeping_one(3);
        let running = boot.exclusivity.try_acquire().unwrap();
        boot.spawn();
        let untouched_while_held = boot.stays_at(3);

        drop(running);

        assert!(untouched_while_held);
        assert!(boot.settles_at(1));
    }

    #[test]
    fn a_boot_prune_waits_out_several_transfers_in_a_row() {
        let boot = boot_keeping_one(3);
        let first = boot.exclusivity.try_acquire().unwrap();
        boot.spawn();
        assert!(boot.stays_at(3));
        drop(first);
        let second = boot.exclusivity.try_acquire().unwrap();
        let untouched_through_the_handover = boot.stays_at(3);

        drop(second);

        assert!(untouched_through_the_handover);
        assert!(boot.settles_at(1));
    }

    #[test]
    fn the_boot_prune_stops_after_its_one_real_run() {
        let boot = boot_keeping_one(3);
        boot.spawn();
        assert!(boot.settles_at(1));
        boot.add_copies(3, "b");
        let after_new_copies = boot.copies();

        let after_the_run = boot.rt.block_on(boot.exclusivity.acquire_when_free());
        drop(after_the_run);

        assert!(boot.stays_at(after_new_copies));
    }
}
