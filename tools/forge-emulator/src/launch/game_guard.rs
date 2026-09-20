use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;

use crate::EmulatorError;

const HYPRCTL: &str = "hyprctl";
const CLIENTS_QUERY: [&str; 2] = ["clients", "-j"];
const GAME_CLASS_PREFIXES: [&str; 2] = ["steam_app_", "gamescope"];
const FULLSCREEN_MODE: f64 = 2.0;
const DEFAULT_PROBE_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct GameGuard {
    probe: HyprlandProbe,
    allow_over_game: bool,
}

impl GameGuard {
    pub fn system() -> Self {
        Self::probing(HyprlandProbe::system())
    }

    pub fn probing(probe: HyprlandProbe) -> Self {
        Self {
            probe,
            allow_over_game: false,
        }
    }

    pub fn allow_over_game(mut self) -> Self {
        self.allow_over_game = true;
        self
    }

    pub async fn ensure_no_game(&self) -> Result<(), EmulatorError> {
        if self.allow_over_game {
            return Ok(());
        }
        self.probe.ensure_no_game().await
    }
}

/// Runs `<program> <args...> clients -j` and fails closed on anything but a readable, game-free answer.
#[derive(Debug, Clone)]
pub struct HyprlandProbe {
    program: PathBuf,
    args: Vec<OsString>,
    deadline: Duration,
}

impl HyprlandProbe {
    pub fn system() -> Self {
        Self::command(HYPRCTL, Vec::new())
    }

    pub fn command(program: impl Into<PathBuf>, args: Vec<OsString>) -> Self {
        Self {
            program: program.into(),
            args,
            deadline: DEFAULT_PROBE_DEADLINE,
        }
    }

    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    pub async fn ensure_no_game(&self) -> Result<(), EmulatorError> {
        let unreadable = |reason: String| EmulatorError::GameStateUnreadable { reason };
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .args(CLIENTS_QUERY)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = command
            .spawn()
            .map_err(|e| unreadable(format!("{}: {e}", self.program.display())))?;
        let output = tokio::time::timeout(self.deadline, child.wait_with_output())
            .await
            .map_err(|_| unreadable(format!("no answer within {:?}", self.deadline)))?
            .map_err(|e| unreadable(e.to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(unreadable(format!("{}: {}", output.status, stderr.trim())));
        }
        assess_hyprland_clients(&output.stdout)
    }
}

/// Same predicate as the no-gui-over-game hook (`fullscreen >= 2`, or a `steam_app_` / `gamescope`
/// class), plus boolean `fullscreen: true`; any shape it cannot evaluate is a refusal.
pub fn assess_hyprland_clients(json: &[u8]) -> Result<(), EmulatorError> {
    let unreadable = |reason: &str| EmulatorError::GameStateUnreadable {
        reason: reason.to_owned(),
    };
    let clients: Value =
        serde_json::from_slice(json).map_err(|_| unreadable("client list is not JSON"))?;
    let Value::Array(clients) = clients else {
        return Err(unreadable("client list is not a JSON array"));
    };
    let mut games = BTreeSet::new();
    for client in &clients {
        let Value::Object(client) = client else {
            return Err(unreadable("a client entry is not a JSON object"));
        };
        let class = match client.get("class") {
            None | Some(Value::Null) => "",
            Some(Value::String(class)) => class.as_str(),
            Some(_) => return Err(unreadable("a client class is not a string")),
        };
        let fullscreen = match client.get("fullscreen") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => false,
            Some(Value::Bool(true)) => true,
            Some(Value::Number(mode)) => mode.as_f64().is_some_and(|mode| mode >= FULLSCREEN_MODE),
            Some(_) => return Err(unreadable("a client fullscreen state is not a number")),
        };
        let game_class = GAME_CLASS_PREFIXES
            .iter()
            .any(|prefix| class.starts_with(prefix));
        if fullscreen || game_class {
            games.insert(class.to_owned());
        }
    }
    if games.is_empty() {
        return Ok(());
    }
    Err(EmulatorError::GameInProgress {
        windows: games.into_iter().collect::<Vec<_>>().join(", "),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn assess(json: &str) -> Result<(), EmulatorError> {
        assess_hyprland_clients(json.as_bytes())
    }

    #[test]
    fn desktop_without_a_game_is_cleared() {
        for json in [
            "[]",
            r#"[{"class": "com.mitchellh.ghostty", "fullscreen": 0}]"#,
            r#"[{"class": "firefox", "fullscreen": 1}]"#,
            r#"[{"class": null, "fullscreen": null}, {}]"#,
            r#"[{"class": "kitty", "fullscreen": false}]"#,
            r#"[{"class": "my_steam_app_notes", "fullscreen": 0}]"#,
            r#"[{"class": "Gamescope", "fullscreen": 0}]"#,
        ] {
            assert!(assess(json).is_ok(), "expected clearance for {json}");
        }
    }

    #[test]
    fn fullscreen_mode_two_is_the_lowest_refused_mode() {
        for (mode, refused) in [("1", false), ("1.9", false), ("2", true), ("3", true)] {
            let json = format!(r#"[{{"class": "mpv", "fullscreen": {mode}}}]"#);
            assert_eq!(
                matches!(assess(&json), Err(EmulatorError::GameInProgress { .. })),
                refused,
                "fullscreen {mode}"
            );
        }
    }

    #[test]
    fn game_classes_and_boolean_fullscreen_are_refused_naming_each_window_once() {
        let json = r#"[
            {"class": "steam_app_570", "fullscreen": 0},
            {"class": "gamescope", "fullscreen": 0},
            {"class": "steam_app_570", "fullscreen": 2},
            {"class": "legacy-player", "fullscreen": true},
            {"class": "kitty", "fullscreen": 0}
        ]"#;
        let refusal = assess(json);
        assert!(
            matches!(&refusal, Err(EmulatorError::GameInProgress { windows })
                if windows == "gamescope, legacy-player, steam_app_570"),
            "got {refusal:?}"
        );
    }

    #[test]
    fn shapes_the_predicate_cannot_evaluate_fail_closed() {
        for json in [
            "",
            "not json",
            r#"{"class": "kitty"}"#,
            r#"["kitty"]"#,
            r#"[{"class": 7, "fullscreen": 0}]"#,
            r#"[{"class": "kitty", "fullscreen": "2"}]"#,
        ] {
            let refusal = assess(json);
            assert!(
                matches!(refusal, Err(EmulatorError::GameStateUnreadable { .. })),
                "{json:?}: got {refusal:?}"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_guard_allowed_over_a_game_never_runs_the_probe_that_would_refuse() {
        let probe = HyprlandProbe::command(
            "/bin/sh",
            vec![
                "-c".into(),
                r#"printf '[{"class":"steam_app_570","fullscreen":2}]'"#.into(),
                "sh".into(),
            ],
        );

        let guarded = GameGuard::probing(probe.clone()).ensure_no_game().await;
        assert!(
            matches!(guarded, Err(EmulatorError::GameInProgress { .. })),
            "got {guarded:?}"
        );
        let allowed = GameGuard::probing(probe)
            .allow_over_game()
            .ensure_no_game()
            .await;
        assert!(allowed.is_ok(), "got {allowed:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn probe_refuses_when_hyprctl_fails_or_cannot_run() {
        let failing = HyprlandProbe::command(
            "/bin/sh",
            vec![
                "-c".into(),
                "echo no socket >&2; exit 3".into(),
                "sh".into(),
            ],
        );
        let missing = HyprlandProbe::command("/nonexistent/hyprctl", Vec::new());
        for probe in [failing, missing] {
            let refusal = probe.ensure_no_game().await;
            assert!(
                matches!(refusal, Err(EmulatorError::GameStateUnreadable { .. })),
                "{probe:?}: got {refusal:?}"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn probe_that_never_answers_is_refused_at_its_deadline() {
        let probe = HyprlandProbe::command(
            "/bin/sh",
            vec!["-c".into(), "exec sleep 30".into(), "sh".into()],
        )
        .with_deadline(Duration::from_millis(200));
        let refusal = tokio::time::timeout(Duration::from_secs(10), probe.ensure_no_game())
            .await
            .expect("probe deadline is enforced");
        assert!(matches!(
            refusal,
            Err(EmulatorError::GameStateUnreadable { .. })
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn probe_passes_the_clients_query_to_hyprctl() {
        let probe = HyprlandProbe::command(
            "/bin/sh",
            vec![
                "-c".into(),
                r#"[ "$1 $2" = "clients -j" ] && printf '[]' || exit 9"#.into(),
                "sh".into(),
            ],
        );
        assert!(probe.ensure_no_game().await.is_ok());
    }
}
