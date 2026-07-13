use forge_platform_core::BuiltinId;

/// Top-level router discriminant: the shell renders exactly one screen at a time
/// behind fixed chrome. Every sidebar destination is a variant here; navigation
/// swaps the active-screen child entity. Platforms and Stream Apps each get their
/// own overview variant, but the per-integration detail is a single parameterized
/// [`Screen::BuiltinDetail`], which the one generic integration-detail view renders
/// from the target's four `Builtin*` traits. The enum is `Clone` (not `Copy`)
/// because `BuiltinDetail` carries an owned [`BuiltinId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    Chat,
    Actions,
    Triggers,
    Queues,
    EventFeed,
    Globals,
    Scripts,
    Platforms,
    StreamApps,
    BuiltinDetail(BuiltinId),
    Tts,
    Soundboard,
    Server,
    Settings,
}
