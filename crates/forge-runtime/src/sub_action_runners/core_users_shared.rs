use forge_storage::GlobalsRepo;
use forge_types::ArgStack;

const SINGLE_BROADCASTER_NAMESPACE: &str = "local";

/// Per-user variables are keyed by `(broadcaster_id, user_id, name)`, but a sub-action
/// runner has no channel identity in its `RunContext`. Triggers that carry one set a
/// `broadcaster_id` arg (chat triggers do not); when it is absent every user variable
/// shares the single-broadcaster `"local"` namespace.
pub(super) async fn resolve_broadcaster_id(
    arg_stack: &ArgStack,
    globals: &dyn GlobalsRepo,
) -> String {
    let resolved =
        super::interpolate::interpolate_with_globals("%broadcaster_id%", arg_stack, globals).await;
    if resolved.is_empty() || resolved == "%broadcaster_id%" {
        SINGLE_BROADCASTER_NAMESPACE.to_owned()
    } else {
        resolved
    }
}
