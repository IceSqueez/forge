mod game_guard;
mod group;
mod launcher;
mod live_paths;
mod output;
mod process;
mod signals;
mod spec;

pub use game_guard::{GameGuard, HyprlandProbe, assess_hyprland_clients};
pub use launcher::{LaunchOptions, LaunchedForge, launch_forge};
pub use live_paths::LivePaths;
pub use output::{CapturedOutput, OutputLine, OutputStream};
pub use process::{ForgeExit, ForgeProcess};
pub use spec::{DEFAULT_LOG_DIRECTIVES, ForgeCommand, INHERITED_VARIABLES, LaunchSpec};
