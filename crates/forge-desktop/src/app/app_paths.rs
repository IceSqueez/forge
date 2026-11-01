use std::sync::OnceLock;

use forge_paths::AppPaths;

static APP_PATHS: OnceLock<AppPaths> = OnceLock::new();

pub(crate) fn app_paths() -> &'static AppPaths {
    APP_PATHS.get_or_init(AppPaths::resolve)
}
