use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use forge_update::launch_supervisor;
use gpui::{App, Global};

use crate::instance_lock::InstanceLock;

struct ExitState {
    instance_lock: Option<InstanceLock>,
    armed: Option<PathBuf>,
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

    pub fn arm(&self, data_dir: PathBuf) {
        self.0.borrow_mut().armed = Some(data_dir);
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
        let Some(data_dir) = armed else {
            return;
        };
        match launch_supervisor(&data_dir) {
            Ok(pid) => tracing::info!(pid, "update supervisor launched"),
            Err(error) => tracing::warn!(error = %error, "update supervisor could not be launched"),
        }
    }
}
