use std::sync::Arc;

use tokio::sync::Notify;

const RUNTIME_PUMPS: usize = 8;

thread_local! {
    static RUNTIME_PARKS: Arc<Notify> = Arc::new(Notify::new());
}

pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .on_thread_park(|| RUNTIME_PARKS.with(|parks| parks.notify_waiters()))
        .enable_all()
        .build()
        .expect("current-thread runtime")
}

pub fn alive_tasks(rt: &tokio::runtime::Runtime) -> usize {
    rt.metrics().num_alive_tasks()
}

pub fn pump(rt: &tokio::runtime::Runtime) {
    rt.block_on(async {
        for _ in 0..RUNTIME_PUMPS {
            tokio::task::yield_now().await;
        }
    });
}

pub fn run_until_tasks_park(rt: &tokio::runtime::Runtime) {
    let parks = RUNTIME_PARKS.with(Arc::clone);
    rt.block_on(super::within_hang_guard(parks.notified()));
}

pub fn pump_until(rt: &tokio::runtime::Runtime, mut reached: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + super::PUMP_HANG_GUARD;
    while !reached() {
        assert!(
            std::time::Instant::now() < deadline,
            "the awaited state never arrived"
        );
        pump(rt);
    }
}
