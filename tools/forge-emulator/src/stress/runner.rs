use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use forge_events::EventSource;
use serde::Serialize;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::load::{Delivery, FloodMix, delivery, due};
use super::probe::{ProcessProbe, ProcessReading, database_bytes, load_average};
use super::profile::StressProfile;
use super::tracker::Tracker;
use crate::EmulatorError;
use crate::control::{EventFilter, Observation};
use crate::fixture::TwitchAccount;
use crate::launch::{
    ForgeCommand, GameGuard, LaunchOptions, LaunchedForge, LivePaths, launch_forge,
};
use crate::overlay::{OverlayPage, OverlayPages};
use crate::run::LogTail;
use crate::twitch::FakeTwitch;

const READY_TIMEOUT: Duration = Duration::from_secs(120);
const SUBSCRIBED_TIMEOUT: Duration = Duration::from_secs(60);
const PAGE_TIMEOUT: Duration = Duration::from_secs(30);
const PACING_TICK: Duration = Duration::from_millis(1);
const FRAME_POLL: Duration = Duration::from_millis(20);
const MAX_INJECTS_PER_TICK: u64 = 2_000;
const SETTLE_MARGIN_MS: u64 = 200;
const OBSERVED_KINDS: [&str; 5] = [
    "action.start",
    "action.done",
    "action.skipped",
    "command.matched",
    "chat.send.failed",
];

pub struct StressOptions {
    pub emulator: PathBuf,
    pub forge: ForgeCommand,
    pub run_root: PathBuf,
    pub log_directives: String,
    pub guard: GameGuard,
    pub live: LivePaths,
    pub max_attempts: u32,
    pub shutdown_grace: Duration,
}

#[derive(Debug, Clone, Serialize)]
pub struct Phase {
    pub label: String,
    pub flood_per_sec: f64,
    pub secs: u64,
    /// Steady stimuli run only in loaded phases; a recovery phase sends nothing.
    pub loaded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhaseResult {
    pub phase: Phase,
    pub started_ms: u64,
    pub ended_ms: u64,
    pub sent: u64,
    pub owed: u64,
    pub degraded: Option<String>,
    pub tables: BTreeMap<String, u64>,
    /// Events forge's server dropped for the observer during the phase; when non-zero the
    /// observer's counts for the phase undercount and only frames and replies are complete.
    pub observer_dropped: u64,
}

/// One row of `samples.jsonl`.
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub at_ms: u64,
    pub phase: usize,
    pub injected: u64,
    pub process: ProcessReading,
    pub cpu_percent: Option<f64>,
    pub main_thread_percent: Option<f64>,
    pub database_bytes: Option<u64>,
    pub load_average: Option<f64>,
    pub twitch_events: u64,
    pub observer_dropped: u64,
    pub uncorrelated: u64,
    pub frames: Vec<u64>,
    pub page_dropped: Vec<u64>,
    pub replies: u64,
    pub other_kinds: BTreeMap<String, u64>,
    /// Per action: started, done, skipped, unstarted, in flight.
    pub actions: BTreeMap<String, [u64; 5]>,
    /// WARN and ERROR log lines since the previous sample, by target and message.
    pub warnings: BTreeMap<String, u64>,
}

pub struct StressOutcome {
    pub phases: Vec<PhaseResult>,
    pub samples: Vec<Sample>,
    pub tracker: Tracker,
    pub actions: Vec<String>,
    pub pages: Vec<String>,
    pub forge_exited: bool,
    pub shutdown_ms: Option<u64>,
    pub shutdown_forced: Option<bool>,
    pub warnings_total: BTreeMap<String, u64>,
    pub data_dir: PathBuf,
    pub version: Option<String>,
}

type Shared<T> = Arc<Mutex<T>>;

fn lock<T>(shared: &Mutex<T>) -> MutexGuard<'_, T> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

pub async fn run_stress(
    profile: &StressProfile,
    options: StressOptions,
) -> Result<StressOutcome, EmulatorError> {
    let account = profile
        .fixture
        .twitch
        .clone()
        .ok_or_else(|| EmulatorError::InvalidLaunch {
            reason: "the stress profile seeds no Twitch account".to_owned(),
        })?;
    let fake = FakeTwitch::start(profile.fakes.config_for(&account)).await?;
    let launch = LaunchOptions {
        emulator: options.emulator,
        forge: options.forge,
        run_root: options.run_root.clone(),
        fixture: profile.fixture.clone(),
        endpoint_overrides: fake.endpoint_overrides().to_vec(),
        log_directives: options.log_directives,
        guard: options.guard,
        live: options.live,
        ready_timeout: READY_TIMEOUT,
        max_attempts: options.max_attempts,
        shutdown_grace: options.shutdown_grace,
    };
    let launched = match launch_forge(&launch).await {
        Ok(launched) => launched,
        Err(e) => {
            fake.shutdown().await;
            return Err(e);
        }
    };
    let result = drive(profile, &account, &fake, launched, options.shutdown_grace).await;
    fake.shutdown().await;
    result
}

async fn drive(
    profile: &StressProfile,
    account: &TwitchAccount,
    fake: &FakeTwitch,
    launched: LaunchedForge,
    shutdown_grace: Duration,
) -> Result<StressOutcome, EmulatorError> {
    let LaunchedForge {
        process,
        client,
        mut events,
        seed,
        ..
    } = launched;
    let clock = Instant::now();
    let data_dir = process.data_dir().to_owned();
    let log_dir = process.log_dir();
    let pid = process.pid();
    let version = client.forge_version().await.ok();

    let mut filters = vec![EventFilter {
        source: Some(EventSource::Twitch),
        kind: None,
    }];
    filters.extend(OBSERVED_KINDS.iter().map(|kind| EventFilter {
        source: None,
        kind: Some((*kind).to_owned()),
    }));
    client.subscribe(&filters).await?;

    let actions = measured_actions(profile);
    let runs = profile
        .stimuli
        .iter()
        .map(|stimulus| {
            stimulus
                .runs
                .iter()
                .filter_map(|name| actions.iter().position(|action| action == name))
                .collect()
        })
        .collect();
    let mut tracker = Tracker::new(actions.clone(), runs, profile.pages.len());
    for command in &seed.chat_commands {
        tracker.learn_action_id(command.action_id.to_string(), &command.action_name);
    }
    for trigger in &seed.event_triggers {
        tracker.learn_action_id(trigger.action_id.to_string(), &trigger.action_name);
    }
    let tracker: Shared<Tracker> = Arc::new(Mutex::new(tracker));

    for (name, value) in &profile.globals {
        client.set_global(name, value, true).await?;
    }
    await_subscriptions(fake, &profile.subscription_types()).await?;
    let pages = OverlayPages::for_seed(&seed)?;
    let mut opened: Vec<Arc<OverlayPage>> = Vec::new();
    for name in &profile.pages {
        pages.open_page(name, PAGE_TIMEOUT).await?;
        if let Some(page) = pages.page(name) {
            page.take_frames();
            opened.push(page);
        }
    }

    let mut background: Vec<JoinHandle<()>> = Vec::new();
    let pump_tracker = Arc::clone(&tracker);
    background.push(tokio::spawn(async move {
        while let Some(observation) = events.next().await {
            let arrived = Instant::now();
            let mut tracker = lock(&pump_tracker);
            match observation {
                Observation::Event(event) => tracker.on_event(&event, arrived),
                Observation::Dropped(count) => tracker.observer_dropped += count,
                Observation::Undecodable { .. } => tracker.uncorrelated += 1,
            }
        }
    }));
    let frame_tracker = Arc::clone(&tracker);
    let frame_pages = opened.clone();
    background.push(tokio::spawn(async move {
        loop {
            tokio::time::sleep(FRAME_POLL).await;
            for (index, page) in frame_pages.iter().enumerate() {
                let frames = page.take_frames();
                if frames.is_empty() {
                    continue;
                }
                let mut tracker = lock(&frame_tracker);
                for frame in &frames {
                    tracker.on_frame(index, &frame.raw, frame.arrived);
                }
            }
        }
    }));
    let mut replies = fake.tap_requests();
    let reply_tracker = Arc::clone(&tracker);
    background.push(tokio::spawn(async move {
        while let Some(tapped) = replies.recv().await {
            if tapped.request.path.ends_with("/chat/messages") {
                lock(&reply_tracker).on_reply(tapped.request.body.as_ref(), tapped.arrived);
            }
        }
    }));

    let phase_now: Shared<usize> = Arc::new(Mutex::new(0));
    let samples: Shared<Vec<Sample>> = Arc::new(Mutex::new(Vec::new()));
    let warnings_total: Shared<BTreeMap<String, u64>> = Arc::new(Mutex::new(BTreeMap::new()));
    background.push(tokio::spawn(sample_loop(SampleLoop {
        every: Duration::from_millis(profile.sample_ms),
        probe: ProcessProbe::new(pid),
        data_dir: data_dir.clone(),
        log: LogTail::from_start(&log_dir),
        clock,
        tracker: Arc::clone(&tracker),
        phase: Arc::clone(&phase_now),
        samples: Arc::clone(&samples),
        warnings_total: Arc::clone(&warnings_total),
        actions: actions.clone(),
    })));

    let generator = Generator {
        profile,
        account,
        fake,
        tracker: &tracker,
        samples: &samples,
        mix: FloodMix::new(&profile.stimuli),
        clock,
    };
    let phases = generator.run_all(&phase_now, &data_dir).await?;

    for task in &background {
        task.abort();
    }
    pages.close();
    drop(opened);
    let mut process = process;
    let forge_exited = !process.is_running();
    drop(client);
    let shutdown_started = Instant::now();
    let (shutdown_ms, shutdown_forced) = match process.shutdown(shutdown_grace).await {
        Ok(exit) => (
            Some(u64::try_from(shutdown_started.elapsed().as_millis()).unwrap_or(u64::MAX)),
            Some(exit.forced),
        ),
        Err(_) => (None, None),
    };

    let tracker = Arc::try_unwrap(tracker)
        .map(|mutex| mutex.into_inner().unwrap_or_else(PoisonError::into_inner))
        .map_err(|_| EmulatorError::InvalidLaunch {
            reason: "a measurement task outlived the run".to_owned(),
        })?;
    let samples = std::mem::take(&mut *lock(&samples));
    let warnings_total = std::mem::take(&mut *lock(&warnings_total));
    Ok(StressOutcome {
        phases,
        samples,
        tracker,
        actions,
        pages: profile.pages.clone(),
        forge_exited,
        shutdown_ms,
        shutdown_forced,
        warnings_total,
        data_dir,
        version,
    })
}

/// Every action some stimulus runs, in the order the profile first names them.
fn measured_actions(profile: &StressProfile) -> Vec<String> {
    let mut actions: Vec<String> = Vec::new();
    for stimulus in &profile.stimuli {
        for action in &stimulus.runs {
            if !actions.contains(action) {
                actions.push(action.clone());
            }
        }
    }
    actions
}

async fn await_subscriptions(fake: &FakeTwitch, types: &[&str]) -> Result<(), EmulatorError> {
    let what = format!("live subscriptions of type {}", types.join(", "));
    fake.wait_for(&what, SUBSCRIBED_TIMEOUT, |ledger| {
        types
            .iter()
            .all(|kind| {
                ledger.subscriptions.iter().any(|subscription| {
                    subscription.subscription_type == *kind
                        && ledger
                            .live_sessions()
                            .any(|session| session.id == subscription.session_id)
                })
            })
            .then_some(())
    })
    .await
}

struct Generator<'a> {
    profile: &'a StressProfile,
    account: &'a TwitchAccount,
    fake: &'a FakeTwitch,
    tracker: &'a Shared<Tracker>,
    samples: &'a Shared<Vec<Sample>>,
    mix: FloodMix,
    clock: Instant,
}

impl Generator<'_> {
    async fn run_all(
        mut self,
        phase_now: &Shared<usize>,
        data_dir: &std::path::Path,
    ) -> Result<Vec<PhaseResult>, EmulatorError> {
        let profile = self.profile;
        let mut results: Vec<PhaseResult> = Vec::new();
        let mut last_stable: Option<f64> = None;
        let mut knee_found = false;
        for rate in &profile.ramp.rates {
            let phase = Phase {
                label: format!("ramp {rate}/s"),
                flood_per_sec: f64::from(*rate),
                secs: profile.ramp.hold_secs,
                loaded: true,
            };
            let index = results.len();
            let mut result = self.run_phase(index, phase, phase_now).await?;
            self.settle().await;
            result.degraded = self.judge(index, &result);
            result.tables = table_rows(data_dir).await;
            eprintln!(
                "forge-emulator: {} sent {} of {} owed{}",
                result.phase.label,
                result.sent,
                result.owed,
                result
                    .degraded
                    .as_deref()
                    .map(|why| format!(" - DEGRADED: {why}"))
                    .unwrap_or_default()
            );
            let degraded = result.degraded.is_some();
            results.push(result);
            if degraded {
                knee_found = true;
                break;
            }
            last_stable = Some(f64::from(*rate));
        }
        let recovery = |label: &str| Phase {
            label: label.to_owned(),
            flood_per_sec: 0.0,
            secs: profile.recovery_secs,
            loaded: false,
        };
        let mut result = self
            .run_phase(results.len(), recovery("recovery after ramp"), phase_now)
            .await?;
        result.tables = table_rows(data_dir).await;
        results.push(result);

        let base = last_stable
            .or_else(|| profile.ramp.rates.first().map(|rate| f64::from(*rate)))
            .unwrap_or(1.0);
        let burst = Phase {
            label: format!(
                "burst {:.0}/s ({}x last stable)",
                base * profile.burst.multiplier,
                profile.burst.multiplier
            ),
            flood_per_sec: base * profile.burst.multiplier,
            secs: profile.burst.secs,
            loaded: true,
        };
        let index = results.len();
        let mut result = self.run_phase(index, burst, phase_now).await?;
        self.settle().await;
        result.degraded = self.judge(index, &result);
        result.tables = table_rows(data_dir).await;
        results.push(result);
        let mut result = self
            .run_phase(results.len(), recovery("recovery after burst"), phase_now)
            .await?;
        result.tables = table_rows(data_dir).await;
        results.push(result);
        if !knee_found {
            eprintln!("forge-emulator: the ramp ended without reaching a knee");
        }
        Ok(results)
    }

    async fn run_phase(
        &mut self,
        index: usize,
        phase: Phase,
        phase_now: &Shared<usize>,
    ) -> Result<PhaseResult, EmulatorError> {
        *lock(phase_now) = index;
        let dropped_before = lock(self.tracker).observer_dropped;
        let started = Instant::now();
        let length_ms = phase.secs * 1_000;
        let steady: Vec<(usize, f64)> = self
            .profile
            .stimuli
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                s.per_minute
                    .map(|per_minute| (i, f64::from(per_minute) / 60.0))
            })
            .collect();
        let mut steady_sent = vec![0u64; steady.len()];
        let mut sent = 0u64;
        let mut owed = 0u64;
        loop {
            let elapsed = elapsed_ms(started);
            if elapsed >= length_ms {
                break;
            }
            owed = due(phase.flood_per_sec, elapsed);
            let mut budget = MAX_INJECTS_PER_TICK;
            while sent < owed && budget > 0 {
                let Some(stimulus) = self.mix.next_pick() else {
                    break;
                };
                self.inject(stimulus, index).await?;
                sent += 1;
                budget -= 1;
            }
            if phase.loaded {
                for (slot, (stimulus, per_sec)) in steady.iter().enumerate() {
                    while steady_sent[slot] < due(*per_sec, elapsed) {
                        self.inject(*stimulus, index).await?;
                        steady_sent[slot] += 1;
                    }
                }
            }
            tokio::time::sleep(PACING_TICK).await;
        }
        Ok(PhaseResult {
            phase,
            started_ms: millis_since(self.clock, started),
            ended_ms: elapsed_ms(self.clock),
            sent,
            owed: owed.max(sent),
            degraded: None,
            tables: BTreeMap::new(),
            observer_dropped: lock(self.tracker).observer_dropped - dropped_before,
        })
    }

    /// Lets the sampler pick up the log lines written in the phase's last moments.
    async fn settle(&self) {
        tokio::time::sleep(Duration::from_millis(
            self.profile.sample_ms + SETTLE_MARGIN_MS,
        ))
        .await;
    }

    async fn inject(&self, stimulus: usize, phase: usize) -> Result<(), EmulatorError> {
        let Some(definition) = self.profile.stimuli.get(stimulus) else {
            return Ok(());
        };
        let seq = {
            let mut tracker = lock(self.tracker);
            let seq = tracker.injected_total();
            tracker.injected(seq, stimulus, phase, Instant::now());
            seq
        };
        match delivery(definition, seq, self.profile.senders, self.account) {
            Delivery::Chat { viewer, text } => {
                self.fake.inject_chat_message(&viewer, &text).await?;
            }
            Delivery::Notification {
                subscription_type,
                event,
            } => {
                self.fake
                    .inject_notification(subscription_type, event)
                    .await?;
            }
        }
        Ok(())
    }

    fn judge(&self, index: usize, result: &PhaseResult) -> Option<String> {
        let knee = &self.profile.knee;
        let achieved = if result.owed == 0 {
            1.0
        } else {
            result.sent as f64 / result.owed as f64
        };
        if achieved < knee.min_achieved_share {
            return Some(format!(
                "the generator delivered only {:.0}% of the target rate: forge stopped reading its EventSub socket fast enough",
                achieved * 100.0
            ));
        }
        let lost: BTreeMap<String, u64> = lock(self.samples)
            .iter()
            .filter(|sample| sample.phase == index)
            .flat_map(|sample| sample.warnings.iter())
            .filter(|(key, _)| {
                knee.fatal_warnings
                    .iter()
                    .any(|fatal| key.contains(fatal.as_str()))
            })
            .fold(BTreeMap::new(), |mut acc, (key, count)| {
                *acc.entry(key.clone()).or_default() += count;
                acc
            });
        if let Some((key, count)) = lost.iter().next() {
            return Some(format!("forge logged `{key}` {count} times"));
        }
        if result.observer_dropped > 0 {
            return None;
        }
        let tracker = lock(self.tracker);
        let allowed = result.phase.flood_per_sec * knee.max_backlog_secs;
        for name in &knee.watch {
            let Some(action) = tracker.action_position(name) else {
                continue;
            };
            let backlog = tracker.unstarted(action) + tracker.in_flight(action);
            if backlog as f64 > allowed {
                return Some(format!(
                    "`{name}` has {backlog} events unfinished at step end, more than {:.0} ({}s of this rate)",
                    allowed, knee.max_backlog_secs
                ));
            }
            let skipped = tracker.counts(action).skipped_total();
            if skipped > 0 {
                return Some(format!("`{name}` had {skipped} executions skipped"));
            }
        }
        None
    }
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn millis_since(origin: Instant, at: Instant) -> u64 {
    u64::try_from(at.saturating_duration_since(origin).as_millis()).unwrap_or(u64::MAX)
}

/// Row counts of every table, read through the sqlite3 CLI in read-only mode so forge's own
/// connections are never contended for; empty when the CLI is missing.
async fn table_rows(data_dir: &std::path::Path) -> BTreeMap<String, u64> {
    let database = data_dir.join("forge.db");
    let run = |sql: String| {
        let database = database.clone();
        async move {
            tokio::process::Command::new("sqlite3")
                .arg("-readonly")
                .arg(&database)
                .arg(sql)
                .output()
                .await
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        }
    };
    let Some(names) = run(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx%'"
            .to_owned(),
    )
    .await
    else {
        return BTreeMap::new();
    };
    let names: Vec<&str> = names.lines().filter(|name| !name.is_empty()).collect();
    let query = names
        .iter()
        .map(|name| format!("SELECT '{name}', count(*) FROM \"{name}\""))
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    let Some(counts) = run(query).await else {
        return BTreeMap::new();
    };
    counts
        .lines()
        .filter_map(|line| {
            let (name, count) = line.split_once('|')?;
            Some((name.to_owned(), count.trim().parse().ok()?))
        })
        .collect()
}

struct SampleLoop {
    every: Duration,
    probe: ProcessProbe,
    data_dir: PathBuf,
    log: LogTail,
    clock: Instant,
    tracker: Shared<Tracker>,
    phase: Shared<usize>,
    samples: Shared<Vec<Sample>>,
    warnings_total: Shared<BTreeMap<String, u64>>,
    actions: Vec<String>,
}

async fn sample_loop(mut state: SampleLoop) {
    let mut previous: Option<(Instant, ProcessReading)> = None;
    loop {
        tokio::time::sleep(state.every).await;
        let now = Instant::now();
        let reading = state.probe.read();
        let (cpu_percent, main_thread_percent) = match previous {
            Some((at, before)) => {
                let elapsed = millis_since(at, now);
                (
                    super::probe::cpu_percent(before.cpu_ticks, reading.cpu_ticks, elapsed),
                    super::probe::cpu_percent(
                        before.main_thread_ticks,
                        reading.main_thread_ticks,
                        elapsed,
                    ),
                )
            }
            None => (None, None),
        };
        previous = Some((now, reading));
        let mut warnings: BTreeMap<String, u64> = BTreeMap::new();
        if let Ok(records) = state.log.poll() {
            for record in records {
                if record.level == "WARN" || record.level == "ERROR" {
                    *warnings
                        .entry(warning_key(&record.target, &record.line))
                        .or_default() += 1;
                }
            }
        }
        {
            let mut total = lock(&state.warnings_total);
            for (key, count) in &warnings {
                *total.entry(key.clone()).or_default() += count;
            }
        }
        let sample = {
            let tracker = lock(&state.tracker);
            Sample {
                at_ms: millis_since(state.clock, now),
                phase: *lock(&state.phase),
                injected: tracker.injected_total(),
                process: reading,
                cpu_percent,
                main_thread_percent,
                database_bytes: database_bytes(&state.data_dir),
                load_average: load_average(),
                twitch_events: tracker.twitch_events,
                observer_dropped: tracker.observer_dropped,
                uncorrelated: tracker.uncorrelated,
                frames: tracker.frames.clone(),
                page_dropped: tracker.page_dropped.clone(),
                replies: tracker.replies,
                other_kinds: tracker.other_kinds.clone(),
                actions: state
                    .actions
                    .iter()
                    .enumerate()
                    .map(|(index, name)| {
                        let counts = tracker.counts(index);
                        (
                            name.clone(),
                            [
                                counts.started,
                                counts.done,
                                counts.skipped_total(),
                                tracker.unstarted(index),
                                tracker.in_flight(index),
                            ],
                        )
                    })
                    .collect(),
                warnings,
            }
        };
        lock(&state.samples).push(sample);
    }
}

/// `target: message` with the trailing `key=value` fields cut off, so repeats group together.
pub fn warning_key(target: &str, line: &str) -> String {
    let message = line
        .split_once(&format!("{target}: "))
        .map_or(line, |(_, rest)| rest);
    let mut cut = message.len();
    for (index, _) in message.match_indices(' ') {
        let word = &message[index + 1..];
        let key_len = word.find('=').filter(|at| {
            word[..*at]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        if key_len.is_some_and(|at| at > 0) {
            cut = index;
            break;
        }
    }
    format!("{target}: {}", message[..cut].trim())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn log_lines_group_by_target_and_message_without_their_fields() {
        for (line, expected) in [
            (
                "2026-09-25T10:00:00Z  WARN forge::trigger: evaluation fell behind the bus; those events fired no trigger missed=512",
                "forge::trigger: evaluation fell behind the bus; those events fired no trigger",
            ),
            (
                "2026-09-25T10:00:00Z  WARN forge_runtime::bus: event_log flush task lagged; events may not be persisted skipped=3 extra=\"a b\"",
                "forge_runtime::bus: event_log flush task lagged; events may not be persisted",
            ),
            (
                "2026-09-25T10:00:00Z ERROR forge::x: a = b is not a field",
                "forge::x: a = b is not a field",
            ),
        ] {
            let target = expected.split(": ").next().unwrap();
            assert_eq!(warning_key(target, line), expected);
        }
    }
}
