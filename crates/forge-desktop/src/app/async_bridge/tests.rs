use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use forge_components::{ToastKind, Toasts};
use gpui::{AppContext, Entity, TestAppContext};

use super::*;
use crate::shared::repo_failure::RepoFailure;
use crate::test_support::{pump, pump_until, runtime};

struct Probe;

type Reported = Vec<(ToastKind, String)>;

const PERSIST_FAILED: &str = "could not save";
const DEBOUNCE: Duration = Duration::from_millis(1);
const DEBOUNCE_SETTLE: Duration = Duration::from_millis(15);

fn reported(cx: &mut TestAppContext) -> Vec<(ToastKind, String)> {
    cx.update(|cx| {
        cx.global::<Toasts>()
            .items()
            .iter()
            .map(|t| (t.kind, t.message.to_string()))
            .collect()
    })
}

fn paused_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .expect("paused current-thread runtime")
}

fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
    for _ in 0..2 {
        rt.block_on(async { tokio::time::sleep(DEBOUNCE_SETTLE).await });
        pump(rt);
        cx.run_until_parked();
    }
}

#[gpui::test]
fn a_failed_debounced_write_toasts_its_context_and_a_successful_one_stays_silent(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| cx.set_global(Toasts::new()));
    let rt = paused_runtime();
    let probe = cx.new(|_| Probe);
    let outcomes: [(Result<(), RepoFailure>, Reported); 2] = [
        (Ok(()), Vec::new()),
        (
            Err(RepoFailure::operation("disk gone")),
            vec![(ToastKind::Error, PERSIST_FAILED.to_owned())],
        ),
    ];
    for (write_result, expected) in outcomes {
        cx.update(|cx| cx.set_global(Toasts::new()));
        let debounce = Debounced::new(DEBOUNCE);

        probe.update(cx, |_, cx| {
            debounce.schedule_reporting(
                rt.handle(),
                PERSIST_FAILED,
                async move { write_result },
                ErrorSink::Toast,
                cx,
            );
        });
        settle(cx, &rt);

        assert_eq!(reported(cx), expected);
    }
}

#[gpui::test]
fn a_debounced_write_superseded_by_a_newer_one_never_runs_or_reports(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Toasts::new()));
    let rt = paused_runtime();
    let probe = cx.new(|_| Probe);
    let debounce = Debounced::new(DEBOUNCE);
    let ran = Arc::new(AtomicU64::new(0));

    probe.update(cx, |_, cx| {
        let first_ran = Arc::clone(&ran);
        debounce.schedule_reporting(
            rt.handle(),
            PERSIST_FAILED,
            async move {
                first_ran.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>("first".to_owned())
            },
            ErrorSink::Toast,
            cx,
        );
        let second_ran = Arc::clone(&ran);
        debounce.schedule_reporting(
            rt.handle(),
            PERSIST_FAILED,
            async move {
                second_ran.fetch_add(10, Ordering::SeqCst);
                Ok::<(), RepoFailure>(())
            },
            ErrorSink::Toast,
            cx,
        );
    });
    settle(cx, &rt);

    assert_eq!((ran.load(Ordering::SeqCst), reported(cx)), (10, Vec::new()));
}

#[gpui::test]
fn report_failure_toasts_only_when_the_write_failed(cx: &mut TestAppContext) {
    let rt = paused_runtime();
    let probe = cx.new(|_| Probe);
    for (write_result, expected) in [
        (Ok(()), Vec::new()),
        (
            Err(RepoFailure::operation("disk gone")),
            vec![(ToastKind::Error, PERSIST_FAILED.to_owned())],
        ),
    ] {
        cx.update(|cx| cx.set_global(Toasts::new()));

        probe.update(cx, |_, cx| {
            report_failure(
                rt.handle(),
                async move { write_result },
                ErrorSink::Toast,
                PERSIST_FAILED,
                cx,
            );
        });
        settle(cx, &rt);

        assert_eq!(reported(cx), expected);
    }
}

#[derive(Default)]
struct SettledWrites(Vec<(usize, WriteOutcome)>);

const WRITE_FAILED: &str = "disk gone";

fn write_through(
    writer: &SnapshotWriter,
    writes: &Entity<SettledWrites>,
    index: usize,
    write: impl Future<Output = Result<(), RepoFailure>> + Send + 'static,
    cx: &mut TestAppContext,
) {
    writes.update(cx, |_, cx| {
        writer.write(
            write,
            move |settled: &mut SettledWrites, outcome, _| settled.0.push((index, outcome)),
            cx,
        );
    });
}

fn settled_writes(
    writes: &Entity<SettledWrites>,
    cx: &mut TestAppContext,
) -> Vec<(usize, WriteOutcome)> {
    writes.read_with(cx, |settled, _| {
        let mut outcomes = settled.0.clone();
        outcomes.sort_by_key(|(index, _)| *index);
        outcomes
    })
}

fn counted(
    ran: &Arc<AtomicU64>,
    mark: u64,
    result: Result<(), RepoFailure>,
) -> impl Future<Output = Result<(), RepoFailure>> + Send + 'static {
    let ran = Arc::clone(ran);
    async move {
        ran.fetch_add(mark, Ordering::SeqCst);
        result
    }
}

#[gpui::test]
fn a_snapshot_write_settles_written_or_failed_with_the_write_error(cx: &mut TestAppContext) {
    let rt = runtime();
    for (result, expected) in [
        (Ok(()), WriteOutcome::Written),
        (
            Err(RepoFailure::operation(WRITE_FAILED)),
            WriteOutcome::Failed(RepoFailure::operation(WRITE_FAILED)),
        ),
    ] {
        let writer = SnapshotWriter::new(rt.handle());
        let writes = cx.new(|_| SettledWrites::default());

        write_through(&writer, &writes, 0, async move { result }, cx);
        pump(&rt);
        cx.run_until_parked();

        assert_eq!(settled_writes(&writes, cx), vec![(0, expected)]);
    }
}

#[gpui::test]
fn rapid_snapshot_writes_run_only_the_newest_and_settle_the_rest_superseded(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let writer = SnapshotWriter::new(rt.handle());
    let writes = cx.new(|_| SettledWrites::default());
    let ran = Arc::new(AtomicU64::new(0));

    for (index, mark) in [(0, 1), (1, 10), (2, 100)] {
        write_through(&writer, &writes, index, counted(&ran, mark, Ok(())), cx);
    }
    pump(&rt);
    cx.run_until_parked();

    assert_eq!(
        (ran.load(Ordering::SeqCst), settled_writes(&writes, cx)),
        (
            100,
            vec![
                (0, WriteOutcome::Superseded),
                (1, WriteOutcome::Superseded),
                (2, WriteOutcome::Written),
            ]
        )
    );
}

#[gpui::test]
fn a_running_write_that_fails_after_a_newer_one_was_queued_settles_superseded(
    cx: &mut TestAppContext,
) {
    let rt = runtime();
    let writer = SnapshotWriter::new(rt.handle());
    let writes = cx.new(|_| SettledWrites::default());
    let gate = Arc::new(tokio::sync::Notify::new());
    let ran = Arc::new(AtomicU64::new(0));

    let held = Arc::clone(&gate);
    write_through(
        &writer,
        &writes,
        0,
        async move {
            held.notified().await;
            Err(RepoFailure::operation(WRITE_FAILED))
        },
        cx,
    );
    pump(&rt);
    write_through(&writer, &writes, 1, counted(&ran, 1, Ok(())), cx);
    gate.notify_one();
    pump(&rt);
    cx.run_until_parked();

    assert_eq!(
        (ran.load(Ordering::SeqCst), settled_writes(&writes, cx)),
        (
            1,
            vec![(0, WriteOutcome::Superseded), (1, WriteOutcome::Written)]
        )
    );
}

#[gpui::test]
fn a_write_sent_after_the_queue_closed_never_runs_or_settles(cx: &mut TestAppContext) {
    let rt = runtime();
    let writer = SnapshotWriter::new(rt.handle());
    drop(rt);
    let writes = cx.new(|_| SettledWrites::default());
    let ran = Arc::new(AtomicU64::new(0));

    write_through(&writer, &writes, 0, counted(&ran, 1, Ok(())), cx);
    cx.run_until_parked();

    assert_eq!(
        (ran.load(Ordering::SeqCst), settled_writes(&writes, cx)),
        (0, Vec::new())
    );
}

type Marks = Arc<std::sync::Mutex<Vec<&'static str>>>;

fn mark(marks: &Marks, step: &'static str) {
    marks.lock().unwrap().push(step);
}

fn marked(marks: &Marks) -> Vec<&'static str> {
    marks.lock().unwrap().clone()
}

#[gpui::test]
fn one_key_finishes_the_running_write_before_the_newest_queued_one_starts(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Toasts::new()));
    let rt = runtime();
    let probe = cx.new(|_| Probe);
    let mut writers = KeyedSnapshotWriters::new(rt.handle());
    let marks = Marks::default();
    let (release, held) = tokio::sync::oneshot::channel::<()>();
    let enqueue =
        |writers: &mut KeyedSnapshotWriters<&str>,
         work: std::pin::Pin<Box<dyn Future<Output = Result<(), RepoFailure>> + Send>>,
         cx: &mut TestAppContext| {
            probe.update(cx, |_, cx| {
                writers.write_reporting("alpha", work, PERSIST_FAILED, cx);
            });
        };

    let first = Arc::clone(&marks);
    enqueue(
        &mut writers,
        Box::pin(async move {
            mark(&first, "first started");
            held.await.map_err(RepoFailure::operation)?;
            mark(&first, "first finished");
            Ok(())
        }),
        cx,
    );
    pump_until(&rt, || marked(&marks).len() == 1);
    for step in ["middle started", "newest started"] {
        let marks = Arc::clone(&marks);
        enqueue(
            &mut writers,
            Box::pin(async move {
                mark(&marks, step);
                Ok(())
            }),
            cx,
        );
    }
    release.send(()).unwrap();
    pump_until(&rt, || marked(&marks).len() == 3);

    assert_eq!(
        marked(&marks),
        ["first started", "first finished", "newest started"]
    );
}

#[gpui::test]
fn writes_on_different_keys_never_supersede_each_other(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Toasts::new()));
    let rt = runtime();
    let probe = cx.new(|_| Probe);
    let mut writers = KeyedSnapshotWriters::new(rt.handle());
    let ran = Arc::new(AtomicU64::new(0));

    probe.update(cx, |_, cx| {
        for (key, weight) in [("alpha", 1), ("bravo", 10)] {
            writers.write_reporting(key, counted(&ran, weight, Ok(())), PERSIST_FAILED, cx);
        }
    });
    pump_until(&rt, || ran.load(Ordering::SeqCst) == 11);
    cx.run_until_parked();

    assert_eq!(reported(cx), Vec::new());
}

#[gpui::test]
fn a_failed_keyed_write_toasts_its_own_context_while_the_other_key_stays_silent(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| cx.set_global(Toasts::new()));
    let rt = runtime();
    let probe = cx.new(|_| Probe);
    let mut writers = KeyedSnapshotWriters::new(rt.handle());
    let ran = Arc::new(AtomicU64::new(0));

    probe.update(cx, |_, cx| {
        writers.write_reporting("alpha", counted(&ran, 1, Ok(())), "alpha failed", cx);
        writers.write_reporting(
            "bravo",
            counted(&ran, 10, Err(RepoFailure::operation(WRITE_FAILED))),
            "bravo failed",
            cx,
        );
    });
    pump_until(&rt, || ran.load(Ordering::SeqCst) == 11);
    cx.run_until_parked();

    assert_eq!(
        reported(cx),
        vec![(ToastKind::Error, "bravo failed".to_owned())]
    );
}

#[gpui::test]
fn a_write_on_release_toasts_only_when_it_failed(cx: &mut TestAppContext) {
    let rt = runtime();
    for (result, expected) in [
        (Ok(()), Vec::new()),
        (
            Err(RepoFailure::operation(WRITE_FAILED)),
            vec![(ToastKind::Error, PERSIST_FAILED.to_owned())],
        ),
    ] {
        cx.update(|cx| cx.set_global(Toasts::new()));
        let writer = SnapshotWriter::new(rt.handle());
        let ran = Arc::new(AtomicU64::new(0));

        cx.update(|cx| writer.write_on_release(counted(&ran, 1, result), PERSIST_FAILED, cx));
        pump_until(&rt, || ran.load(Ordering::SeqCst) == 1);
        cx.run_until_parked();

        assert_eq!(reported(cx), expected);
    }
}

#[gpui::test]
fn a_release_write_superseded_by_a_newer_one_stays_silent(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Toasts::new()));
    let rt = runtime();
    let writer = SnapshotWriter::new(rt.handle());
    let ran = Arc::new(AtomicU64::new(0));

    cx.update(|cx| {
        writer.write_on_release(
            counted(&ran, 1, Err(RepoFailure::operation(WRITE_FAILED))),
            PERSIST_FAILED,
            cx,
        );
        writer.write_on_release(counted(&ran, 10, Ok(())), PERSIST_FAILED, cx);
    });
    pump_until(&rt, || ran.load(Ordering::SeqCst) == 10);
    cx.run_until_parked();

    assert_eq!(reported(cx), Vec::new());
}

fn attach_count(cx: &mut TestAppContext, probe: &Entity<Probe>) -> Rc<Cell<usize>> {
    let attached = Rc::new(Cell::new(0));
    let counter = Rc::clone(&attached);
    probe.update(cx, |_, cx| {
        attach_to_window(
            move |_: &mut Probe, _, _| counter.set(counter.get() + 1),
            cx,
        );
    });
    cx.run_until_parked();
    attached
}

#[gpui::test]
fn attaching_with_no_window_open_does_nothing(cx: &mut TestAppContext) {
    let probe = cx.new(|_| Probe);

    assert_eq!(attach_count(cx, &probe).get(), 0);
}

#[gpui::test]
fn attaching_with_a_window_open_runs_once(cx: &mut TestAppContext) {
    let probe = cx.new(|_| Probe);
    crate::test_support::blank_window(cx);

    assert_eq!(attach_count(cx, &probe).get(), 1);
}

#[gpui::test]
fn a_view_gone_before_the_attach_lands_is_left_alone(cx: &mut TestAppContext) {
    let probe = cx.new(|_| Probe);
    crate::test_support::blank_window(cx);
    let attached = Rc::new(Cell::new(0));
    let counter = Rc::clone(&attached);
    probe.update(cx, |_, cx| {
        attach_to_window(
            move |_: &mut Probe, _, _| counter.set(counter.get() + 1),
            cx,
        );
    });

    drop(probe);
    cx.run_until_parked();

    assert_eq!(attached.get(), 0);
}
