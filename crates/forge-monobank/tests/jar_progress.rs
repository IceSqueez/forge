mod support;

use std::sync::Arc;

use forge_events::TypedEvent;
use forge_events::test_support::CapturingPublisher;
use forge_monobank::{JAR_PROGRESS_KIND, JarProgressed, MonobankProvider};
use forge_platform_core::DonationProvider;
use support::{
    Bank, JAR, Limits, STATEMENT_PREFIX, creds_with, jar_id, mount, now_unix, provider_on,
    ticking_timing, top_up,
};
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use wiremock::{MockServer, ResponseTemplate};

const TOP_UP_BALANCE_MICROS: u64 = 100_000_000;
const LISTED_JAR_BALANCE: i64 = 4_200;
const LISTED_JAR_BALANCE_MICROS: u64 = 42_000_000;

struct Progressing {
    _server: MockServer,
    bank: Bank,
    provider: MonobankProvider,
    events: Arc<CapturingPublisher>,
    drain: JoinHandle<()>,
}

impl Progressing {
    async fn start(bank: Bank) -> Self {
        let server = MockServer::start().await;
        mount(&server, &bank).await;
        let events = CapturingPublisher::shared();
        let provider = provider_on(
            &server.uri(),
            creds_with(support::TOKEN, Some(JAR)),
            Limits::open().shared(),
        )
        .with_timing(ticking_timing())
        .with_event_publisher(events.clone());
        let mut stream = provider.donations();
        let drain = tokio::spawn(async move { while stream.next().await.is_some() {} });
        Self {
            _server: server,
            bank,
            provider,
            events,
            drain,
        }
    }

    fn progress(&self) -> Vec<(bool, u64)> {
        self.events
            .of_kind(JAR_PROGRESS_KIND)
            .iter()
            .filter_map(JarProgressed::from_event)
            .map(|progress| (progress.is_synced, progress.balance_micros))
            .collect()
    }

    async fn wait_for_progress(&self, count: usize) {
        forge_test_support::settle_until("the awaited jar progress", || {
            self.progress().len() >= count
        })
        .await;
    }

    async fn wait_for_statement_calls(&self, count: usize) {
        forge_test_support::settle_until("the awaited statement calls", || {
            self.bank.statement_calls().len() >= count
        })
        .await;
    }

    async fn let_the_current_poll_finish(&self) {
        let made = self.bank.statement_calls().len();
        self.wait_for_statement_calls(made + 1).await;
    }
}

impl Drop for Progressing {
    fn drop(&mut self) {
        self.drain.abort();
    }
}

fn bank_with_a_recent_top_up() -> Bank {
    let bank = Bank::new(&[JAR]);
    bank.set_jar_balance(JAR, LISTED_JAR_BALANCE);
    bank.add(JAR, top_up("RECENT", now_unix() - 60));
    bank
}

#[tokio::test]
async fn connect_syncs_the_balance_of_the_newest_top_up_in_the_window() {
    let p = Progressing::start(bank_with_a_recent_top_up()).await;

    p.wait_for_progress(1).await;

    assert_eq!(p.progress()[0], (true, TOP_UP_BALANCE_MICROS));
}

#[tokio::test]
async fn connect_without_top_ups_in_the_window_syncs_the_listed_jar_balance() {
    let bank = Bank::new(&[JAR]);
    bank.set_jar_balance(JAR, LISTED_JAR_BALANCE);
    let p = Progressing::start(bank).await;

    p.wait_for_progress(1).await;

    assert_eq!(p.progress()[0], (true, LISTED_JAR_BALANCE_MICROS));
}

#[tokio::test]
async fn steady_polls_of_an_unchanged_balance_publish_nothing_after_the_sync() {
    let p = Progressing::start(bank_with_a_recent_top_up()).await;
    p.wait_for_progress(1).await;

    let made = p.bank.statement_calls().len();
    p.wait_for_statement_calls(made + 3).await;

    assert_eq!(p.progress(), [(true, TOP_UP_BALANCE_MICROS)]);
}

#[tokio::test]
async fn a_new_top_up_publishes_live_progress_with_the_new_balance() {
    let p = Progressing::start(bank_with_a_recent_top_up()).await;
    p.wait_for_progress(1).await;

    let mut newer = top_up("NEWER", now_unix());
    newer["balance"] = serde_json::json!(30_000);
    p.bank.add(JAR, newer);
    p.wait_for_progress(2).await;

    assert_eq!(p.progress()[1], (false, 300_000_000));
}

#[tokio::test]
async fn selecting_the_jar_again_resyncs_an_unchanged_balance() {
    let p = Progressing::start(bank_with_a_recent_top_up()).await;
    p.wait_for_progress(1).await;

    assert!(p.provider.select_jar(jar_id(JAR)).await.is_ok());
    p.wait_for_progress(2).await;

    assert_eq!(p.progress()[1], (true, TOP_UP_BALANCE_MICROS));
}

#[tokio::test]
async fn older_page_backfill_never_publishes_the_balances_it_reaches_back_to() {
    let bank = Bank::new(&[JAR]);
    let start = now_unix() - 3_000;
    for offset in 0..=forge_monobank::STATEMENT_PAGE_LIMIT as i64 {
        let mut item = top_up(&format!("T{offset}"), start + offset);
        item["balance"] = serde_json::json!(1_000 + offset);
        bank.add(JAR, item);
    }
    let p = Progressing::start(bank).await;

    p.wait_for_statement_calls(3).await;
    p.let_the_current_poll_finish().await;
    let calls = p.bank.statement_calls();
    assert!(calls[1].to < calls[0].to, "no older page: {calls:?}");

    assert_eq!(p.progress(), [(true, 15_000_000)]);
}

#[tokio::test]
async fn a_failed_statement_call_publishes_no_progress() {
    let bank = bank_with_a_recent_top_up();
    bank.fail_next(
        STATEMENT_PREFIX,
        ResponseTemplate::new(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
    );
    let p = Progressing::start(bank).await;

    p.wait_for_progress(1).await;
    p.let_the_current_poll_finish().await;

    assert_eq!(p.progress(), [(true, TOP_UP_BALANCE_MICROS)]);
}
