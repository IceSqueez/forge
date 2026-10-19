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
