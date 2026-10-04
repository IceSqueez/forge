use std::time::Duration;

const MINUTE: Duration = Duration::from_secs(60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

pub const MIN_SCHEDULE_DELAY: Duration = MINUTE;
pub const MAX_SCHEDULE_DELAY: Duration = DAY.saturating_mul(365);

pub const MAX_PENDING_SCHEDULED_RUNS: u64 = 1_000;
pub const MAX_PENDING_SCHEDULED_RUNS_PER_ACTION: u64 = 100;

pub const MAX_SCHEDULED_ARGS_BYTES: usize = 64 * 1024;
pub const MAX_SCHEDULE_KEY_CHARS: usize = 200;

pub const WALL_CLOCK_RECHECK: Duration = Duration::from_secs(30);
pub const MIN_LATE_TOLERANCE: Duration = WALL_CLOCK_RECHECK.saturating_mul(4);
pub const MAX_LATE_TOLERANCE: Duration = MAX_SCHEDULE_DELAY;

pub const CATCH_UP_SETTLE_LIMIT: Duration = MINUTE;
