use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use forge_update::launch_supervisor;
use gpui::{App, Global};

use crate::instance_lock::InstanceLock;
use crate::restore_relaunch::relaunch_after_exit;

enum ExitRelaunch {
    UpdateSupervisor(PathBuf),
    RestoredInstance,
}

struct ExitState {
    instance_lock: Option<InstanceLock>,
    armed: Option<ExitRelaunch>,
}

#[derive(Clone)]
pub struct ExitHandoff(Rc<RefCell<ExitState>>);

impl Global for ExitHandoff {}

impl ExitHandoff {
    pub fn new(instance_lock: Option<InstanceLock>) -> Self {
        Self(Rc::new(RefCell::new(ExitState {
            instance_lock,
            armed: None,
        })))
    }

    pub fn adopt_lock(&self, instance_lock: Option<InstanceLock>) {
        self.0.borrow_mut().instance_lock = instance_lock;
    }

    pub fn arm_update(&self, data_dir: PathBuf) {
        self.0.borrow_mut().armed = Some(ExitRelaunch::UpdateSupervisor(data_dir));
    }

    pub fn arm_restore(&self) {
        let mut state = self.0.borrow_mut();
        if state.armed.is_none() {
            state.armed = Some(ExitRelaunch::RestoredInstance);
        }
    }

    pub fn disarm_restore(&self) {
        let mut state = self.0.borrow_mut();
        if matches!(state.armed, Some(ExitRelaunch::RestoredInstance)) {
            state.armed = None;
        }
    }

    pub fn claim_at_quit(cx: &App) -> Option<Self> {
        if cfg!(target_os = "macos") {
            cx.try_global::<Self>().cloned()
        } else {
            None
        }
    }

    pub fn finish(&self) {
        let (instance_lock, armed) = {
            let mut state = self.0.borrow_mut();
            (state.instance_lock.take(), state.armed.take())
        };
        drop(instance_lock);
        match armed {
            Some(ExitRelaunch::UpdateSupervisor(data_dir)) => match launch_supervisor(&data_dir) {
                Ok(pid) => tracing::info!(pid, "update supervisor launched"),
                Err(error) => {
                    tracing::warn!(error = %error, "update supervisor could not be launched")
                }
            },
            Some(ExitRelaunch::RestoredInstance) => relaunch_after_exit(),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::{ExitHandoff, ExitRelaunch};
    use crate::instance_lock::{LockOutcome, acquire};

    impl ExitHandoff {
        pub(crate) fn armed_update_dir(&self) -> Option<PathBuf> {
            match &self.0.borrow().armed {
                Some(ExitRelaunch::UpdateSupervisor(dir)) => Some(dir.clone()),
                _ => None,
            }
        }

        pub(crate) fn restore_is_armed(&self) -> bool {
            matches!(self.0.borrow().armed, Some(ExitRelaunch::RestoredInstance))
        }

        pub(crate) fn is_disarmed(&self) -> bool {
            self.0.borrow().armed.is_none()
        }
    }

    #[test]
    fn an_update_armed_after_a_restore_takes_the_slot() {
        let handoff = ExitHandoff::new(None);
        handoff.arm_restore();

        handoff.arm_update(PathBuf::from("/data/forge"));

        assert_eq!(
            handoff.armed_update_dir(),
            Some(PathBuf::from("/data/forge"))
        );
    }

    #[test]
    fn arming_a_restore_never_displaces_an_armed_update() {
        let handoff = ExitHandoff::new(None);
        handoff.arm_update(PathBuf::from("/data/forge"));

        handoff.arm_restore();

        assert_eq!(
            handoff.armed_update_dir(),
            Some(PathBuf::from("/data/forge"))
        );
    }

    #[test]
    fn disarming_a_restore_leaves_an_armed_update_in_place() {
        let handoff = ExitHandoff::new(None);
        handoff.arm_update(PathBuf::from("/data/forge"));

        handoff.disarm_restore();

        assert_eq!(
            handoff.armed_update_dir(),
            Some(PathBuf::from("/data/forge"))
        );
    }

    #[test]
    fn disarming_an_armed_restore_empties_the_slot() {
        let handoff = ExitHandoff::new(None);
        handoff.arm_restore();

        handoff.disarm_restore();

        assert!(handoff.is_disarmed());
    }

    #[test]
    fn finishing_releases_the_instance_lock_for_the_next_instance() {
        let data = TempDir::new().unwrap();
        let LockOutcome::Acquired(lock) = acquire(data.path()) else {
            panic!("the first lock was not acquired");
        };
        let handoff = ExitHandoff::new(Some(lock));
        let held = matches!(acquire(data.path()), LockOutcome::AlreadyRunning);

        handoff.finish();

        assert_eq!(
            (
                held,
                matches!(acquire(data.path()), LockOutcome::Acquired(_))
            ),
            (true, true)
        );
    }
}
