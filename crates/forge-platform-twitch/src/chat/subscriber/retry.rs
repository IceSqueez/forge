use super::{
    SubscribeCtx, TOPICS, TopicOutcome, TopicSpec, classify_failure, mark_active, record_created,
    set_tracker_status,
};
use crate::helix::HelixError;
use crate::subscriptions::{SubStatus, SubscriptionTracker};
use forge_net::Backoff;
use futures_util::stream::{self, StreamExt};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, info, warn};

const RETRY_BASE_DELAY: Duration = Duration::from_secs(2);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(60);
const RETRY_ATTEMPT_LIMIT: u32 = 6;
const RETRY_CONCURRENCY: usize = 3;

pub(crate) struct TopicRetryRun {
    run: Pin<Box<dyn Future<Output = ()> + Send>>,
    session_tx: watch::Sender<String>,
}

impl TopicRetryRun {
    pub(crate) fn move_to_session(&self, session_id: &str) {
        self.session_tx.send_replace(session_id.to_owned());
    }

    pub(crate) async fn finished(&mut self) {
        self.run.as_mut().await;
    }
}

pub(crate) struct TopicRetries {
    ctx: SubscribeCtx,
    settle: RetryingSettle,
}

impl TopicRetries {
    pub(super) fn new(ctx: SubscribeCtx, settle: RetryingSettle) -> Self {
        Self { ctx, settle }
    }

    pub(crate) fn into_run(self) -> Option<TopicRetryRun> {
        let pending = self.settle.registered().len();
        if pending == 0 {
            return None;
        }
        debug!(
            topics = pending,
            concurrency = RETRY_CONCURRENCY,
            "retrying failed eventsub subscriptions"
        );
        let (session_tx, session_rx) = watch::channel(self.ctx.session_id.clone());
        Some(TopicRetryRun {
            run: Box::pin(self.run(session_rx)),
            session_tx,
        })
    }

    async fn run(self, session: watch::Receiver<String>) {
        let Self { ctx, settle } = self;
        let tracker = Arc::clone(&settle.tracker);
        stream::iter(settle.registered())
            .for_each_concurrent(RETRY_CONCURRENCY, |index| {
                retry_topic(ctx.clone(), Arc::clone(&tracker), index, session.clone())
            })
            .await;
        drop(settle);
    }
}

pub(super) struct RetryingSettle {
    tracker: SubscriptionTracker,
    indexes: Mutex<Vec<usize>>,
}

impl RetryingSettle {
    pub(super) fn new(tracker: SubscriptionTracker) -> Self {
        Self {
            tracker,
            indexes: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn register(&self, index: usize) {
        let mut indexes = self.indexes.lock().unwrap_or_else(|p| p.into_inner());
        if !indexes.contains(&index) {
            indexes.push(index);
        }
    }

    fn registered(&self) -> Vec<usize> {
        self.indexes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl Drop for RetryingSettle {
    fn drop(&mut self) {
        let indexes = self.indexes.get_mut().unwrap_or_else(|p| p.into_inner());
        for index in indexes.iter() {
            settle_retrying(&self.tracker, *index);
        }
    }
}

async fn retry_topic(
    ctx: SubscribeCtx,
    tracker: SubscriptionTracker,
    index: usize,
    session: watch::Receiver<String>,
) {
    let Some(topic) = TOPICS.get(index) else {
        return;
    };
    let mut backoff = Backoff::new(RETRY_BASE_DELAY, RETRY_MAX_DELAY);
    while backoff.attempt() < RETRY_ATTEMPT_LIMIT {
        tokio::time::sleep(backoff.next_delay()).await;
        if retrying_reason(&tracker, index).is_none() {
            return;
        }
        let session_id = session.borrow().clone();
        let attempt = ctx.on_session(session_id.clone());
        let outcome = match attempt.create(topic).await {
            Ok(response) => record_created(&attempt, &tracker, index, topic, response),
            Err(error) if error.is_already_exists() => mark_active(
                &attempt,
                &tracker,
                index,
                topic.kind,
                None,
                attempt.condition(topic),
            ),
            Err(error) => {
                let session_moved = *session.borrow() != session_id;
                record_retry_failure(&tracker, index, topic, &error, session_moved)
            }
        };
        match outcome {
            TopicOutcome::Active => {
                info!(
                    kind = topic.kind,
                    attempts = backoff.attempt(),
                    "eventsub subscription recovered after retry"
                );
                return;
            }
            TopicOutcome::Retryable => {}
            TopicOutcome::AlreadyExists
            | TopicOutcome::Failed
            | TopicOutcome::ScopeRejected
            | TopicOutcome::Unauthorized => return,
        }
    }
    let reason = retrying_reason(&tracker, index).unwrap_or_default();
    settle_retrying(&tracker, index);
    warn!(
        kind = topic.kind,
        attempts = backoff.attempt(),
        error = %reason,
        "eventsub subscription retries exhausted; giving up for this session"
    );
}

fn record_retry_failure(
    tracker: &SubscriptionTracker,
    index: usize,
    topic: &TopicSpec,
    error: &HelixError,
    session_moved: bool,
) -> TopicOutcome {
    let (reason, outcome) = classify_failure(error);
    let outcome = match outcome {
        TopicOutcome::Failed if session_moved => TopicOutcome::Retryable,
        other => other,
    };
    if matches!(outcome, TopicOutcome::Retryable) {
        debug!(kind = topic.kind, error = %error, "eventsub subscription retry failed; will retry");
        set_tracker_status(tracker, index, SubStatus::Retrying(reason));
    } else {
        warn!(kind = topic.kind, error = %error, "eventsub subscription retry rejected; giving up for this session");
        set_tracker_status(tracker, index, SubStatus::Failed(reason));
    }
    outcome
}

fn retrying_reason(tracker: &SubscriptionTracker, index: usize) -> Option<String> {
    let records = tracker.read().unwrap_or_else(|p| p.into_inner());
    match records.get(index).map(|rec| &rec.status) {
        Some(SubStatus::Retrying(reason)) => Some(reason.clone()),
        _ => None,
    }
}

fn settle_retrying(tracker: &SubscriptionTracker, index: usize) {
    let mut records = tracker.write().unwrap_or_else(|p| p.into_inner());
    if let Some(rec) = records.get_mut(index)
        && let SubStatus::Retrying(reason) = &mut rec.status
    {
        rec.status = SubStatus::Failed(std::mem::take(reason));
    }
}
