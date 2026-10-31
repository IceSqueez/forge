use tokio::runtime::Handle;
use tracing::dispatcher::{self, Dispatch};

use crate::ScriptError;

const SCRIPT_STACK_BYTES: usize = 256 << 20;
const SCRIPT_THREAD_NAME: &str = "forge-script";
const SCRIPT_THREAD_PANICKED: &str = "script thread panicked";

pub(crate) fn on_script_stack<T: Send>(
    work: impl FnOnce() -> Result<T, ScriptError> + Send,
) -> Result<T, ScriptError> {
    let runtime = Handle::try_current().ok();
    let log_dispatch = dispatcher::get_default(Dispatch::clone);
    std::thread::scope(|threads| {
        let thread = std::thread::Builder::new()
            .name(SCRIPT_THREAD_NAME.to_owned())
            .stack_size(SCRIPT_STACK_BYTES)
            .spawn_scoped(threads, move || {
                let _runtime = runtime.as_ref().map(Handle::enter);
                dispatcher::with_default(&log_dispatch, work)
            })
            .map_err(|error| ScriptError::Runtime {
                reason: error.to_string(),
            })?;
        thread.join().unwrap_or_else(|_| {
            Err(ScriptError::Runtime {
                reason: SCRIPT_THREAD_PANICKED.to_owned(),
            })
        })
    })
}
