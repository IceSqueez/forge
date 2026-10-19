use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use forge_update::{
    InstallEnvironment, InstallKind, InstallLayout, ManualReason, UpdatableInstall,
    classify_install,
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};

pub const PREVIOUS_INSTANCE_FLAG: &str = "--forge-restore-after";

const PREVIOUS_EXIT_BUDGET: Duration = Duration::from_secs(10);
const PREVIOUS_EXIT_POLL: Duration = Duration::from_millis(100);

const MAC_OPEN_PROGRAM: &str = "open";
const MAC_OPEN_NEW_INSTANCE: &str = "-n";
const MAC_OPEN_FORWARD_ARGS: &str = "--args";

enum RelaunchTarget {
    AppBundle(PathBuf),
    Executable(PathBuf),
}

pub fn relaunch_after_exit() {
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

pub fn previous_instance(args: impl IntoIterator<Item = String>) -> Option<u32> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == PREVIOUS_INSTANCE_FLAG {
            return args.next()?.parse().ok();
        }
    }
    None
}

pub fn await_previous_exit(previous: u32) -> bool {
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
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), ProcessRefreshKind::new());
    system
        .process(pid)
        .is_some_and(|process| process.status() != ProcessStatus::Zombie)
}
