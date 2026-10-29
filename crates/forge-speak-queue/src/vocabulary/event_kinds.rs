pub const ENQUEUED: &str = "speak.enqueued";
pub const STARTED: &str = "speak.started";
pub const FINISHED: &str = "speak.finished";
pub const SKIPPED: &str = "speak.skipped";
pub const FAILED: &str = "speak.failed";
pub const REJECTED: &str = "speak.rejected";
pub const REMOVED: &str = "speak.removed";
pub const REORDERED: &str = "speak.reordered";
pub const CLEARED: &str = "speak.cleared";
pub const PAUSED: &str = "speak.paused";
pub const RESUMED: &str = "speak.resumed";

pub const ALL: [&str; 11] = [
    ENQUEUED, STARTED, FINISHED, SKIPPED, FAILED, REJECTED, REMOVED, REORDERED, CLEARED, PAUSED,
    RESUMED,
];
