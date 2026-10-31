const RUNTIME_PUMPS: usize = 8;

pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
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
