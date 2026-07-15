use gpui::{App, KeyBinding, actions};

pub const SHELL_CONTEXT: &str = "ForgeShell";

actions!(
    forge_shell,
    [GoHome, GoChat, GoActions, GoTriggers, GoTwitch, GoSettings]
);

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
