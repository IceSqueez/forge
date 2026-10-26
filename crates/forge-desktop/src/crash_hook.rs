use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::crash_record::{CrashRecord, crash_file_name, without_panic_machinery, write_crash};
use crate::system_info::{BUILD_COMMIT, HostFacts, IdentityScrubber, SystemInfo, WindowFacts};

#[cfg(debug_assertions)]
const DEBUG_PANIC_ENV: &str = "FORGE_DEBUG_PANIC";
#[cfg(debug_assertions)]
const DEBUG_PANIC_OFF_MAIN_THREAD: &str = "background";

static CRASH_RECORDED: AtomicBool = AtomicBool::new(false);
static SYSTEM_INFO_SNAPSHOT: Mutex<Option<String>> = Mutex::new(None);

pub fn install(crash_dir: PathBuf) {
    let scrubber = IdentityScrubber::current();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_first_crash(&crash_dir, &scrubber, info);
        previous(info);
    }));
}

pub fn remember_system_info(report_text: String) {
    if let Ok(mut snapshot) = SYSTEM_INFO_SNAPSHOT.lock() {
        *snapshot = Some(report_text);
    }
}

pub async fn snapshot_system_info(window: WindowFacts, rt_handle: &tokio::runtime::Handle) {
    let collected = rt_handle.spawn_blocking(move || {
        SystemInfo::assemble(window, HostFacts::collect_static()).to_report_text()
    });
    if let Ok(report_text) = collected.await {
        remember_system_info(report_text);
    }
}

fn system_info_snapshot() -> Option<String> {
    match SYSTEM_INFO_SNAPSHOT.try_lock() {
        Ok(snapshot) => snapshot.clone(),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner().clone(),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}

fn record_first_crash(
    crash_dir: &std::path::Path,
    scrubber: &IdentityScrubber,
    info: &PanicHookInfo<'_>,
) {
    if CRASH_RECORDED.swap(true, Ordering::SeqCst) {
        return;
    }
    let process_id = std::process::id();
    let unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    let record = CrashRecord {
        message: info
            .payload_as_str()
            .unwrap_or("non-text panic payload")
            .to_owned(),
        location: info.location().map(ToString::to_string),
        thread: std::thread::current().name().map(str::to_owned),
        backtrace: without_panic_machinery(&Backtrace::force_capture().to_string()),
        forge_version: env!("CARGO_PKG_VERSION").to_owned(),
        build_commit: BUILD_COMMIT.to_owned(),
        occurred_at_utc: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default(),
        process_id,
        system_info: system_info_snapshot(),
    }
    .scrubbed(scrubber);
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a failed crash write has nowhere left to be reported"
    )]
    let _ = write_crash(
        crash_dir,
        &crash_file_name(unix_millis, process_id),
        &record,
    );
}

#[cfg(debug_assertions)]
#[expect(
    clippy::panic,
    reason = "debug hook crashes on request to exercise crash reporting"
)]
pub fn panic_if_requested() {
    let Some(request) = std::env::var_os(DEBUG_PANIC_ENV).filter(|value| !value.is_empty()) else {
        return;
    };
    if request == DEBUG_PANIC_OFF_MAIN_THREAD {
        std::thread::spawn(|| panic!("forced by {DEBUG_PANIC_ENV} off the main thread"));
        return;
    }
    panic!("forced by {DEBUG_PANIC_ENV} to exercise the crash report");
}
