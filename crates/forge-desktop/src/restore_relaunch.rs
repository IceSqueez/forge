use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use forge_update::{
    InstallEnvironment, InstallKind, InstallLayout, ManualReason, UpdatableInstall,
    classify_install,
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};

pub(crate) const PREVIOUS_INSTANCE_FLAG: &str = "--forge-restore-after";

const PREVIOUS_EXIT_BUDGET: Duration = Duration::from_secs(10);
const PREVIOUS_EXIT_POLL: Duration = Duration::from_millis(100);

const MAC_OPEN_PROGRAM: &str = "open";
const MAC_OPEN_NEW_INSTANCE: &str = "-n";
const MAC_OPEN_FORWARD_ARGS: &str = "--args";

enum RelaunchTarget {
    AppBundle(PathBuf),
    Executable(PathBuf),
}

pub(crate) fn relaunch_after_exit() {
    let Some(target) = relaunch_target() else {
        tracing::warn!("forge could not find its own executable; restart it by hand");
        return;
    };
    let mut command = match target {
        RelaunchTarget::AppBundle(bundle) => {
            let mut open = Command::new(MAC_OPEN_PROGRAM);
            open.arg(MAC_OPEN_NEW_INSTANCE)
                .arg(bundle)
                .arg(MAC_OPEN_FORWARD_ARGS);
            open
        }
        RelaunchTarget::Executable(program) => Command::new(program),
    };
    let previous = std::process::id();
    command
        .arg(PREVIOUS_INSTANCE_FLAG)
        .arg(previous.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match command.spawn() {
        Ok(child) => tracing::info!(
            previous,
            relaunched = child.id(),
            "forge relaunched to finish the restore"
        ),
        Err(error) => tracing::warn!(
            error = %error,
            "forge could not relaunch itself; the restore finishes at the next start"
        ),
    }
}

fn relaunch_target() -> Option<RelaunchTarget> {
    let env = InstallEnvironment::current()?;
    Some(match classify_install(&env) {
        InstallKind::Updatable(UpdatableInstall {
            layout: InstallLayout::MacAppBundle,
            target,
        }) => RelaunchTarget::AppBundle(target),
        InstallKind::Updatable(UpdatableInstall { target, .. }) => {
            RelaunchTarget::Executable(target)
        }
        InstallKind::ManualOnly(ManualReason::ReadOnlyLocation) => {
            RelaunchTarget::Executable(env.appimage.unwrap_or(env.executable))
        }
        InstallKind::ManualOnly(_) => RelaunchTarget::Executable(env.executable),
    })
}

pub(crate) fn previous_instance(args: impl IntoIterator<Item = String>) -> Option<u32> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == PREVIOUS_INSTANCE_FLAG {
            return args.next()?.parse().ok();
        }
    }
    None
}

pub(crate) fn await_previous_exit(previous: u32) -> bool {
    let pid = Pid::from_u32(previous);
    let started = Instant::now();
    while is_running(pid) {
        if started.elapsed() >= PREVIOUS_EXIT_BUDGET {
            return false;
        }
        std::thread::sleep(PREVIOUS_EXIT_POLL);
    }
    true
}

fn is_running(pid: Pid) -> bool {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system
        .process(pid)
        .is_some_and(|process| process.status() != ProcessStatus::Zombie)
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};

    use super::{PREVIOUS_INSTANCE_FLAG, await_previous_exit, previous_instance};

    fn previous(args: &[&str]) -> Option<u32> {
        previous_instance(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn the_previous_instance_is_read_from_the_value_after_the_flag() {
        for (args, expected) in [
            (vec![PREVIOUS_INSTANCE_FLAG, "4242"], Some(4242)),
            (
                vec!["--screen", "home", PREVIOUS_INSTANCE_FLAG, "17"],
                Some(17),
            ),
            (
                vec![PREVIOUS_INSTANCE_FLAG, "17", "--screen", "home"],
                Some(17),
            ),
            (vec![PREVIOUS_INSTANCE_FLAG, "4294967295"], Some(u32::MAX)),
        ] {
            assert_eq!(previous(&args), expected, "{args:?}");
        }
    }

    #[test]
    fn a_missing_or_unusable_pid_yields_no_previous_instance() {
        for args in [
            vec![],
            vec!["--screen", "home"],
            vec![PREVIOUS_INSTANCE_FLAG],
            vec![PREVIOUS_INSTANCE_FLAG, "forge"],
            vec![PREVIOUS_INSTANCE_FLAG, ""],
            vec![PREVIOUS_INSTANCE_FLAG, "-1"],
            vec![PREVIOUS_INSTANCE_FLAG, "4294967296"],
            vec!["4242"],
        ] {
            assert_eq!(previous(&args), None, "{args:?}");
        }
    }

    #[test]
    fn an_already_exited_previous_instance_is_not_waited_for() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.wait().unwrap();

        assert!(await_previous_exit(child.id()));
    }
}
