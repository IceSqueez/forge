use gpui::{App, KeyBinding, actions};

/// Key-dispatch context contributed by the shell root element. The shell root is
/// an ancestor of every focused element, so bindings scoped to this context fire
/// from anywhere in the app — the app-global navigation tier.
pub const SHELL_CONTEXT: &str = "ForgeShell";

actions!(
    forge_shell,
    [GoHome, GoChat, GoActions, GoTriggers, GoTwitch, GoSettings]
);

/// Installs the shell's global navigation key bindings. The binary MUST call this
/// once at boot, alongside the kit's input registrars. Both the platform accel
/// (`cmd-`) and its Linux/Windows twin (`ctrl-`) are bound so the shortcut is
/// live on every target.
pub fn register_shell_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-1", GoHome, Some(SHELL_CONTEXT)),
        KeyBinding::new("ctrl-1", GoHome, Some(SHELL_CONTEXT)),
        KeyBinding::new("cmd-2", GoChat, Some(SHELL_CONTEXT)),
        KeyBinding::new("ctrl-2", GoChat, Some(SHELL_CONTEXT)),
        KeyBinding::new("cmd-3", GoActions, Some(SHELL_CONTEXT)),
        KeyBinding::new("ctrl-3", GoActions, Some(SHELL_CONTEXT)),
        KeyBinding::new("cmd-4", GoTriggers, Some(SHELL_CONTEXT)),
        KeyBinding::new("ctrl-4", GoTriggers, Some(SHELL_CONTEXT)),
        KeyBinding::new("cmd-5", GoTwitch, Some(SHELL_CONTEXT)),
        KeyBinding::new("ctrl-5", GoTwitch, Some(SHELL_CONTEXT)),
        KeyBinding::new("cmd-6", GoSettings, Some(SHELL_CONTEXT)),
        KeyBinding::new("ctrl-6", GoSettings, Some(SHELL_CONTEXT)),
    ]);
}
