use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::apply_state::{ApplyPhase, ApplyState};
use crate::journal::UpdateJournal;
use crate::{
    ApplyMode, ChildExit, ChildProcess, Clock, DatabaseUnit, HandoffPlan, HandoffRequest,
    HealthMarker, InstallLayout, ProcessHost, SchemaProbe, StagedArtifact, SupervisorContext,
    SupervisorOutcome, SupervisorSettings, UpdatableInstall, UpdateError, run_supervisor,
};
use tempfile::TempDir;

pub const FROM: &str = "0.6.1";
pub const TO: &str = "0.6.2";
pub const SCHEMA: u32 = 7;
pub const OLD_BUILD: &[u8] = b"old build bytes";
pub const NEW_BUILD: &[u8] = b"new build bytes";
pub const LIVE_DB: &[u8] = b"migrated database";
pub const SNAPSHOT_DB: &[u8] = b"pre-update snapshot";
pub const DB_MAIN: &str = "forge.db";
pub const DB_WAL: &str = "forge.db-wal";
pub const DB_SHM: &str = "forge.db-shm";

pub type Behavior = Box<dyn FnMut(u32) -> Option<ChildExit> + Send>;

pub enum Spawn {
    Child {
        behavior: Behavior,
        honours_kill: bool,
    },
    Fails,
}

impl Spawn {
    pub fn child(behavior: impl FnMut(u32) -> Option<ChildExit> + Send + 'static) -> Self {
        Self::Child {
            behavior: Box::new(behavior),
            honours_kill: true,
        }
    }

    pub fn exits(exit: ChildExit) -> Self {
        Self::child(move |_| Some(exit))
    }

    pub fn hangs() -> Self {
        Self::child(|_| None)
    }

    pub fn ignores_kill() -> Self {
        Self::Child {
            behavior: Box::new(|_| None),
            honours_kill: false,
        }
    }

    pub fn reports_healthy_at(data_dir: PathBuf, poll: u32) -> Self {
        Self::child(move |current| {
            if current == poll {
                crate::write_health_marker(&data_dir, TO).unwrap();
            }
            None
        })
    }
}

#[derive(Debug, Clone)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub watched: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct HostLog {
    pub launches: Vec<Launch>,
    pub kills: u32,
    pub parent_waits: u32,
}

pub struct FakeHost {
    parent_exits: bool,
    queue: VecDeque<Spawn>,
    watched: Option<PathBuf>,
    pub log: Arc<Mutex<HostLog>>,
}

impl FakeHost {
    pub fn new(spawns: impl IntoIterator<Item = Spawn>) -> Self {
        Self {
            parent_exits: true,
            queue: spawns.into_iter().collect(),
            watched: None,
            log: Arc::default(),
        }
    }

    pub fn parent_never_exits(mut self) -> Self {
        self.parent_exits = false;
        self
    }

    pub fn watching(mut self, path: &Path) -> Self {
        self.watched = Some(path.to_path_buf());
        self
    }

    pub fn launches(&self) -> Vec<Launch> {
        self.log.lock().unwrap().launches.clone()
    }

    pub fn kills(&self) -> u32 {
        self.log.lock().unwrap().kills
    }

    pub fn parent_waits(&self) -> u32 {
        self.log.lock().unwrap().parent_waits
    }
}

impl ProcessHost for FakeHost {
    fn wait_for_parent_exit(&mut self, _timeout: Duration) -> Result<bool, UpdateError> {
        self.log.lock().unwrap().parent_waits += 1;
        Ok(self.parent_exits)
    }

    fn spawn(
        &mut self,
        program: &Path,
        args: &[OsString],
    ) -> Result<Box<dyn ChildProcess>, UpdateError> {
        let watched = self
            .watched
            .as_ref()
            .and_then(|path| std::fs::read(path).ok());
        let pid = {
            let mut log = self.log.lock().unwrap();
            log.launches.push(Launch {
                program: program.to_path_buf(),
                args: args.to_vec(),
                watched,
            });
            u32::try_from(log.launches.len()).unwrap() + 1000
        };
        let spawn = self
            .queue
            .pop_front()
            .unwrap_or_else(|| Spawn::exits(ChildExit::Code(0)));
        match spawn {
            Spawn::Fails => Err(UpdateError::Io(std::io::Error::from(
                std::io::ErrorKind::PermissionDenied,
            ))),
            Spawn::Child {
                behavior,
                honours_kill,
            } => Ok(Box::new(FakeChild {
                pid,
                behavior,
                honours_kill,
                polls: 0,
                killed: false,
                exited: None,
                log: Arc::clone(&self.log),
            })),
        }
    }
}

struct FakeChild {
    pid: u32,
    behavior: Behavior,
    honours_kill: bool,
    polls: u32,
    killed: bool,
    exited: Option<ChildExit>,
    log: Arc<Mutex<HostLog>>,
}

impl ChildProcess for FakeChild {
    fn id(&self) -> u32 {
        self.pid
    }

    fn try_exit(&mut self) -> Result<Option<ChildExit>, UpdateError> {
        if self.exited.is_some() {
            return Ok(self.exited);
        }
        if self.killed && self.honours_kill {
            self.exited = Some(ChildExit::Terminated);
            return Ok(self.exited);
        }
        let poll = self.polls;
        self.polls += 1;
        self.exited = (self.behavior)(poll);
        Ok(self.exited)
    }

    fn kill(&mut self) -> Result<(), UpdateError> {
        if self.exited.is_none() {
            self.killed = true;
            self.log.lock().unwrap().kills += 1;
        }
        Ok(())
    }
}

pub struct FakeClock {
    start: Instant,
    offset: Cell<Duration>,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            offset: Cell::new(Duration::ZERO),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.offset.get()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.start + self.offset.get()
    }

    fn sleep(&self, duration: Duration) {
        self.offset.set(self.offset.get() + duration);
    }
}

pub struct FixedSchema(pub Option<u32>);

impl SchemaProbe for FixedSchema {
    fn current_schema_version(&self, _data_dir: &Path) -> Option<u32> {
        self.0
    }
}

pub fn settings() -> SupervisorSettings {
    SupervisorSettings {
        health_timeout: Duration::from_secs(10),
        parent_exit_timeout: Duration::from_secs(5),
        installer_timeout: Duration::from_secs(30),
        child_exit_grace: Duration::from_secs(3),
        poll_interval: Duration::from_secs(1),
        file_retry_attempts: 3,
        file_retry_delay: Duration::from_secs(1),
    }
}

pub fn database_unit() -> DatabaseUnit {
    DatabaseUnit::from_file_names([DB_MAIN, DB_WAL, DB_SHM].map(String::from)).unwrap()
}

pub struct Fixture {
    pub root: TempDir,
    pub data_dir: PathBuf,
    pub install_dir: PathBuf,
    pub target: PathBuf,
    pub staged: PathBuf,
    pub snapshot: PathBuf,
    pub state: ApplyState,
    pub journal: UpdateJournal,
}

impl Fixture {
    pub fn file_layout(layout: InstallLayout, mode: ApplyMode) -> Self {
        let name = match layout {
            InstallLayout::LinuxAppImage => "Forge.AppImage",
            InstallLayout::MacAppBundle => "Forge.app",
            InstallLayout::WindowsPortable | InstallLayout::WindowsPerUserMsi => "forge.exe",
        };
        let staged_name = match layout {
            InstallLayout::WindowsPerUserMsi => ".forge-update-forge.msi",
            _ => ".forge-update-artifact",
        };
        Self::build(layout, mode, name, staged_name, |target, staged| {
            std::fs::write(target, OLD_BUILD).unwrap();
            std::fs::write(staged, NEW_BUILD).unwrap();
            target.to_path_buf()
        })
    }

    pub fn bundle(mode: ApplyMode, archive: &[u8]) -> Self {
        Self::build(
            InstallLayout::MacAppBundle,
            mode,
            "Forge.app",
            ".forge-update-Forge.app.tar.gz",
            |target, staged| {
                let binary = bundle_binary(target);
                std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
                std::fs::write(&binary, OLD_BUILD).unwrap();
                std::fs::write(staged, archive).unwrap();
                binary
            },
        )
    }

    fn build(
        layout: InstallLayout,
        mode: ApplyMode,
        name: &str,
        staged_name: &str,
        populate: impl FnOnce(&Path, &Path) -> PathBuf,
    ) -> Self {
        let root = tempfile::tempdir().unwrap();
        let data_dir = root.path().join("data ü");
        let install_dir = root.path().join("Forge ü install");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&install_dir).unwrap();
        let target = install_dir.join(name);
        let staged = install_dir.join(staged_name);
        let running = populate(&target, &staged);
        std::fs::write(data_dir.join(DB_MAIN), LIVE_DB).unwrap();
        std::fs::write(data_dir.join(DB_WAL), b"wal").unwrap();
        std::fs::write(data_dir.join(DB_SHM), b"shm").unwrap();
        let snapshot = data_dir.join("backups").join("pre-update.db");
        std::fs::create_dir_all(snapshot.parent().unwrap()).unwrap();
        std::fs::write(&snapshot, SNAPSHOT_DB).unwrap();
        let install = UpdatableInstall {
            layout,
            target: target.clone(),
        };
        let staged_artifact = StagedArtifact {
            path: staged.clone(),
            size: 0,
        };
        let plan = HandoffPlan::new(HandoffRequest {
            install: &install,
            staged: &staged_artifact,
            from_version: FROM,
            to_version: TO,
            database_snapshot: snapshot.clone(),
            schema_version: SCHEMA,
            running_executable: &running,
            mode,
        })
        .unwrap();
        plan.write(&data_dir).unwrap();
        let state = ApplyState::read(&data_dir).unwrap().unwrap();
        let journal = UpdateJournal::read_blocking(&data_dir).unwrap().unwrap();
        Self {
            root,
            data_dir,
            install_dir,
            target,
            staged,
            snapshot,
            state,
            journal,
        }
    }

    pub fn launch_executable(&self) -> PathBuf {
        self.state.launch_executable.clone()
    }

    pub fn previous(&self) -> PathBuf {
        self.journal.previous_build.clone()
    }

    pub fn host(&self, spawns: impl IntoIterator<Item = Spawn>) -> FakeHost {
        FakeHost::new(spawns).watching(&self.launch_executable())
    }

    pub fn supervise(
        &self,
        host: &mut FakeHost,
        schema: Option<u32>,
    ) -> Result<SupervisorOutcome, UpdateError> {
        let clock = FakeClock::new();
        self.supervise_with(host, &clock, schema)
    }

    pub fn supervise_with(
        &self,
        host: &mut FakeHost,
        clock: &FakeClock,
        schema: Option<u32>,
    ) -> Result<SupervisorOutcome, UpdateError> {
        let unit = database_unit();
        run_supervisor(SupervisorContext {
            data_dir: &self.data_dir,
            database: &unit,
            settings: settings(),
            host,
            clock,
            schema: &FixedSchema(schema),
        })
    }

    pub fn running_build(&self) -> Vec<u8> {
        std::fs::read(self.launch_executable()).unwrap()
    }

    pub fn records_cleared(&self) -> bool {
        [
            UpdateJournal::path_in(&self.data_dir),
            ApplyState::path_in(&self.data_dir),
            HealthMarker::path_in(&self.data_dir),
        ]
        .iter()
        .all(|path| !path.exists())
    }

    pub fn phase(&self) -> Option<ApplyPhase> {
        ApplyState::read(&self.data_dir)
            .unwrap()
            .map(|state| state.phase)
    }
}

pub fn bundle_binary(bundle: &Path) -> PathBuf {
    bundle.join("Contents").join("MacOS").join("forge")
}

pub fn candidate_args() -> Vec<OsString> {
    vec![OsString::from(crate::CANDIDATE_FLAG)]
}
