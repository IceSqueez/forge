use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::reload;

static HANDLE: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

/// Wraps the filter rather than a layer, so the layers below stay downcastable.
pub fn reloadable(filter: EnvFilter) -> reload::Layer<EnvFilter, Registry> {
    let (layer, handle) = reload::Layer::new(filter);
    let _ = HANDLE.set(handle);
    layer
}
