use std::fmt::Display;

pub(crate) fn log_failed_cleanup<T, E: Display>(what: &'static str, result: Result<T, E>) {
    if let Err(error) = result {
        tracing::warn!(%error, what, "update cleanup failed");
    }
}
