#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "helpers outside #[test] functions"
)]

use std::process::Command;

const CHILD_MARKER: &str = "FORGE_TIME_ZONE_TEST_CHILD";

pub(crate) fn in_child_zone(test_name: &str, zone: &str) -> bool {
    if std::env::var_os(CHILD_MARKER).is_some() {
        return true;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([test_name, "--exact", "--test-threads=1", "--nocapture"])
        .env(CHILD_MARKER, "1")
        .env("TZ", zone)
        .output()
        .expect("re-run the test binary in a child process");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "child run of {test_name} in TZ={zone} failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    false
}
