use std::ffi::OsStr;
use std::path::Path;

use super::{InstallEnvironment, InstallKind, InstallLayout, ManualReason, updatable};

const BUNDLE_EXTENSION: &str = "app";
const TRANSLOCATION_DIR: &str = "AppTranslocation";

pub(super) fn classify(env: &InstallEnvironment) -> InstallKind {
    let Some(bundle) = enclosing_bundle(&env.executable) else {
        return InstallKind::ManualOnly(ManualReason::UnknownExecutable);
    };
    if is_translocated(bundle) {
        return InstallKind::ManualOnly(ManualReason::MacTranslocated);
    }
    updatable(InstallLayout::MacAppBundle, bundle.to_path_buf())
}

fn enclosing_bundle(executable: &Path) -> Option<&Path> {
    executable
        .ancestors()
        .skip(1)
        .find(|dir| dir.extension() == Some(OsStr::new(BUNDLE_EXTENSION)))
}

fn is_translocated(bundle: &Path) -> bool {
    bundle
        .components()
        .any(|component| component.as_os_str() == OsStr::new(TRANSLOCATION_DIR))
}
