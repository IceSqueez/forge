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

#[cfg(test)]
mod tests {
    use super::super::{TOPICS, mark_revoked, subscribe_all};
    use super::TopicRetryRun;
    use crate::bot_account::test_support::RecordingBus;
    use crate::helix::{HelixError, HelixRequest, HelixTransport};
    use crate::subscriptions::{SubStatus, SubscriptionTracker};
    use crate::vocabulary::event_kinds::EVENTSUB_SUBSCRIPTION_CREATED;
    use async_trait::async_trait;
    use forge_events::EventPublisher;
    use reqwest::StatusCode;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::sync::watch;
    use tokio::time::Instant;

    const PROBED: &str = "stream.online";
    const PASS_SESSION: &str = "sess-1";
    const SUCCESSOR_SESSION: &str = "sess-2";
    const RECOVERED_ID: &str = "sub-recovered";
    const ATTEMPT_CEILINGS: [Duration; 6] = [
        Duration::from_secs(2),
        Duration::from_secs(4),
        Duration::from_secs(8),
        Duration::from_secs(16),
        Duration::from_secs(32),
        Duration::from_secs(60),
    ];
    const TIMER_GRANULARITY: Duration = Duration::from_millis(1);
    const SLOW_REPLY: Duration = Duration::from_secs(1000);
    const CONCURRENT_RETRY_CEILING: usize = 3;
    const DELAY_SAMPLES: usize = 128;

    #[derive(Clone)]
    enum Answer {
        Created(&'static str),
        Status(StatusCode),
        RateLimited,
        Transport,
        Undecodable,
        NoData,
        Reauth,
        Slow(Box<Answer>),
        Gated(Box<Answer>),
    }

    struct Post {
        topic: String,
        session_id: String,
        condition: serde_json::Value,
        at: Instant,
    }

    #[derive(Default)]
    struct Load {
        now: usize,
        peak: usize,
    }

    struct ScriptedHelix {
        scripts: Mutex<HashMap<&'static str, Vec<Answer>>>,
        posts: watch::Sender<Vec<Post>>,
        gate: watch::Sender<bool>,
        load: Mutex<Load>,
    }

    impl ScriptedHelix {
        fn new(scripts: impl IntoIterator<Item = (&'static str, Vec<Answer>)>) -> Arc<Self> {
            Arc::new(Self {
                scripts: Mutex::new(scripts.into_iter().collect()),
                posts: watch::Sender::new(Vec::new()),
                gate: watch::Sender::new(false),
                load: Mutex::new(Load::default()),
            })
        }

        fn next_answer(&self, topic: &str) -> Answer {
            let mut scripts = self.scripts.lock().unwrap();
            match scripts.get_mut(topic) {
                Some(answers) if answers.len() > 1 => answers.remove(0),
                Some(answers) => answers[0].clone(),
                None => Answer::Created("sub-default"),
            }
        }

        fn sessions_posted(&self, topic: &str) -> Vec<String> {
            self.posts
                .borrow()
                .iter()
                .filter(|post| post.topic == topic)
                .map(|post| post.session_id.clone())
                .collect()
        }

        fn post_times(&self, topic: &str) -> Vec<Instant> {
            self.posts
                .borrow()
                .iter()
                .filter(|post| post.topic == topic)
                .map(|post| post.at)
                .collect()
        }

        fn condition_posted(&self, topic: &str) -> serde_json::Value {
            self.posts
                .borrow()
                .iter()
                .find(|post| post.topic == topic)
                .map(|post| post.condition.clone())
                .unwrap()
        }

        fn peak_load(&self) -> usize {
            self.load.lock().unwrap().peak
        }

        fn open_gate(&self) {
            self.gate.send_replace(true);
        }

        async fn slow_reply(&self) {
            {
                let mut load = self.load.lock().unwrap();
                load.now += 1;
                load.peak = load.peak.max(load.now);
            }
            tokio::time::sleep(SLOW_REPLY).await;
            self.load.lock().unwrap().now -= 1;
        }
    }

    #[async_trait]
    impl HelixTransport for ScriptedHelix {
        async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, HelixError> {
            let body = request.body.unwrap_or_default();
            let topic = body["type"].as_str().unwrap_or_default().to_owned();
            let mut answer = self.next_answer(&topic);
            self.posts.send_modify(|posts| {
                posts.push(Post {
                    topic: topic.clone(),
                    session_id: body["transport"]["session_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    condition: body["condition"].clone(),
                    at: Instant::now(),
                });
            });
            loop {
                answer = match answer {
                    Answer::Slow(next) => {
                        self.slow_reply().await;
                        *next
                    }
                    Answer::Gated(next) => {
                        let mut gate = self.gate.subscribe();
                        gate.wait_for(|open| *open).await.unwrap();
                        *next
                    }
                    Answer::Created(id) => {
                        return Ok(serde_json::json!({
                            "data": [{ "id": id, "type": topic, "condition": {} }]
                        }));
                    }
                    Answer::NoData => return Ok(serde_json::json!({ "data": [] })),
                    Answer::Status(status) => {
                        return Err(HelixError::Http {
                            status: status.as_u16(),
                            body: "rejected".to_owned(),
                        });
                    }
                    Answer::RateLimited => return Err(HelixError::RateLimited),
                    Answer::Transport => {
                        return Err(HelixError::Transport("connection refused".to_owned()));
                    }
                    Answer::Undecodable => {
                        return Err(HelixError::UndecodableResponse(
                            "expected value at line 1 column 1".to_owned(),
                        ));
                    }
                    Answer::Reauth => return Err(HelixError::ReauthRequired),
                };
            }
        }
    }

    struct Retrying {
        stub: Arc<ScriptedHelix>,
        tracker: SubscriptionTracker,
        bus: Arc<RecordingBus>,
        run: Option<TopicRetryRun>,
    }

    impl Retrying {
        async fn after_pass(stub: &Arc<ScriptedHelix>) -> Self {
            Self::after_pass_on(stub, SubscriptionTracker::default()).await
        }

        async fn after_pass_on(stub: &Arc<ScriptedHelix>, tracker: SubscriptionTracker) -> Self {
            let bus = Arc::new(RecordingBus::default());
            let publisher: Arc<dyn EventPublisher> = bus.clone();
            let helix: Arc<dyn HelixTransport> = stub.clone();
            let pass = subscribe_all(
                &helix,
                PASS_SESSION,
                "broadcaster",
                "user",
                &publisher,
                &tracker,
            )
            .await
            .expect("no topic rejects the token, so the pass must stand");
            Self {
                stub: Arc::clone(stub),
                tracker,
                bus,
                run: pass.into_retries().into_run(),
            }
        }

        fn run(&mut self) -> &mut TopicRetryRun {
            self.run
                .as_mut()
                .expect("a transient first-attempt failure must start a retry run")
        }

        async fn finish(&mut self) {
            self.run().finished().await;
        }

        async fn drive_until_posted(&mut self, topic: &str, times: usize) {
            let mut posts = self.stub.posts.subscribe();
            let run = self
                .run
                .as_mut()
                .expect("a transient first-attempt failure must start a retry run");
            let reached = tokio::select! {
                () = run.finished() => false,
                reached = posts.wait_for(|posts| posts.iter().filter(|post| post.topic == topic).count() >= times) => reached.is_ok(),
            };
            assert!(
                reached,
                "the retry run ended before {topic} was posted {times} times"
            );
        }

        fn record(&self, kind: &str) -> (SubStatus, Option<String>) {
            let records = self.tracker.read().unwrap();
            let rec = records.iter().find(|rec| rec.kind == kind).unwrap();
            (rec.status.clone(), rec.subscription_id.clone())
        }

        fn status_of(&self, kind: &str) -> SubStatus {
            self.record(kind).0
        }

        fn created_events_for(&self, kind: &str) -> Vec<serde_json::Value> {
            self.bus
                .of_kind(EVENTSUB_SUBSCRIPTION_CREATED)
                .into_iter()
                .map(|event| event.payload)
                .filter(|payload| payload["type"] == kind)
                .collect()
        }
    }

    fn retrying(reason: &str) -> SubStatus {
        SubStatus::Retrying(reason.to_owned())
    }

    fn failed(reason: &str) -> SubStatus {
        SubStatus::Failed(reason.to_owned())
    }

    fn then(answer: Answer) -> Box<Answer> {
        Box::new(answer)
    }

    #[tokio::test(start_paused = true)]
    async fn every_transient_failure_class_recovers_into_an_active_row_announced_once() {
        let cases = [
            (
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                "HTTP 500",
            ),
            (Answer::Status(StatusCode::SERVICE_UNAVAILABLE), "HTTP 503"),
            (Answer::RateLimited, "rate limited"),
            (Answer::Transport, "connection refused"),
            (Answer::Undecodable, "unreadable response"),
            (Answer::NoData, "unreadable response"),
        ];

        for (first, reason) in cases {
            let stub = ScriptedHelix::new([(PROBED, vec![first, Answer::Created(RECOVERED_ID)])]);
            let mut retry = Retrying::after_pass(&stub).await;
            assert_eq!(retry.status_of(PROBED), retrying(reason));

            retry.finish().await;

            let created = retry.created_events_for(PROBED);
            assert_eq!(
                (
                    retry.record(PROBED),
                    created.len(),
                    created[0]["subscription_id"].clone()
                ),
                (
                    (SubStatus::Active, Some(RECOVERED_ID.to_owned())),
                    1,
                    serde_json::json!(RECOVERED_ID)
                ),
                "{reason}: a retried topic must become Active with the id Twitch returned and \
                 announce itself exactly once"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_topic_failing_every_attempt_gives_up_after_six_retries_with_its_last_reason() {
        let stub = ScriptedHelix::new([(
            PROBED,
            vec![Answer::Status(StatusCode::SERVICE_UNAVAILABLE)],
        )]);
        let mut retry = Retrying::after_pass(&stub).await;

        retry.finish().await;

        assert_eq!(
            (stub.sessions_posted(PROBED).len(), retry.status_of(PROBED)),
            (1 + ATTEMPT_CEILINGS.len(), failed("HTTP 503"))
        );
    }

    #[test]
    fn giving_up_on_a_topic_warns_exactly_once_naming_the_kind_and_the_attempts() {
        let (_, lines) = crate::log_capture::capture_blocking(tracing::Level::DEBUG, async {
            tokio::time::pause();
            let stub = ScriptedHelix::new([(
                PROBED,
                vec![Answer::Status(StatusCode::SERVICE_UNAVAILABLE)],
            )]);
            let mut retry = Retrying::after_pass(&stub).await;
            retry.finish().await;
        });

        let warns: Vec<_> = crate::log_capture::forge_lines(&lines)
            .into_iter()
            .filter(|line| line.level == tracing::Level::WARN)
            .map(|line| {
                (
                    line.message().to_owned(),
                    line.field("kind").unwrap_or_default().to_owned(),
                    line.field("attempts").unwrap_or_default().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            warns,
            vec![(
                "eventsub subscription retries exhausted; giving up for this session".to_owned(),
                PROBED.to_owned(),
                ATTEMPT_CEILINGS.len().to_string(),
            )]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn every_retry_delay_stays_under_its_doubling_ceiling_capped_at_sixty_seconds() {
        for _ in 0..DELAY_SAMPLES {
            let stub = ScriptedHelix::new([(
                PROBED,
                vec![Answer::Status(StatusCode::SERVICE_UNAVAILABLE)],
            )]);
            let mut retry = Retrying::after_pass(&stub).await;

            retry.finish().await;

            let times = stub.post_times(PROBED);
            let gaps: Vec<Duration> = times.windows(2).map(|pair| pair[1] - pair[0]).collect();
            assert_eq!(gaps.len(), ATTEMPT_CEILINGS.len());
            for (attempt, (gap, ceiling)) in gaps.iter().zip(ATTEMPT_CEILINGS).enumerate() {
                assert!(
                    *gap <= ceiling + TIMER_GRANULARITY,
                    "retry {}: waited {gap:?}, over its {ceiling:?} ceiling",
                    attempt + 1
                );
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn no_more_than_three_topics_have_a_retry_in_flight_at_once() {
        let slow_failure = || {
            vec![
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                Answer::Slow(then(Answer::Status(StatusCode::INTERNAL_SERVER_ERROR))),
            ]
        };
        let stub = ScriptedHelix::new([
            ("channel.follow", slow_failure()),
            ("channel.cheer", slow_failure()),
            ("channel.ban", slow_failure()),
            ("stream.offline", slow_failure()),
            (PROBED, slow_failure()),
        ]);
        let mut retry = Retrying::after_pass(&stub).await;

        retry.finish().await;

        assert_eq!(stub.peak_load(), CONCURRENT_RETRY_CEILING);
    }

    #[tokio::test(start_paused = true)]
    async fn a_permanent_rejection_on_a_retry_ends_the_topic_without_another_attempt() {
        let cases = [
            (Answer::Reauth, "unauthorized"),
            (Answer::Status(StatusCode::FORBIDDEN), "missing scope"),
            (Answer::Status(StatusCode::BAD_REQUEST), "HTTP 400"),
        ];

        for (rejection, reason) in cases {
            let stub = ScriptedHelix::new([(
                PROBED,
                vec![
                    Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                    rejection,
                    Answer::Created(RECOVERED_ID),
                ],
            )]);
            let mut retry = Retrying::after_pass(&stub).await;

            retry.finish().await;

            assert_eq!(
                (stub.sessions_posted(PROBED).len(), retry.status_of(PROBED)),
                (2, failed(reason))
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn attempts_after_a_successor_swap_carry_the_successor_session_id() {
        let stub = ScriptedHelix::new([(
            PROBED,
            vec![
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                Answer::Created(RECOVERED_ID),
            ],
        )]);
        let mut retry = Retrying::after_pass(&stub).await;

        retry.run().move_to_session(SUCCESSOR_SESSION);
        retry.finish().await;

        assert_eq!(
            stub.sessions_posted(PROBED),
            vec![PASS_SESSION, SUCCESSOR_SESSION, SUCCESSOR_SESSION]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_old_session_rejection_landing_during_the_swap_is_retried_on_the_successor() {
        let stub = ScriptedHelix::new([(
            PROBED,
            vec![
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                Answer::Gated(then(Answer::Status(StatusCode::BAD_REQUEST))),
                Answer::Created(RECOVERED_ID),
            ],
        )]);
        let mut retry = Retrying::after_pass(&stub).await;
        retry.drive_until_posted(PROBED, 2).await;

        retry.run().move_to_session(SUCCESSOR_SESSION);
        stub.open_gate();
        retry.finish().await;

        assert_eq!(
            (stub.sessions_posted(PROBED), retry.status_of(PROBED)),
            (
                vec![
                    PASS_SESSION.to_owned(),
                    PASS_SESSION.to_owned(),
                    SUCCESSOR_SESSION.to_owned()
                ],
                SubStatus::Active
            )
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_auth_rejection_landing_during_the_swap_still_ends_the_topic() {
        let stub = ScriptedHelix::new([(
            PROBED,
            vec![
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                Answer::Gated(then(Answer::Reauth)),
                Answer::Created(RECOVERED_ID),
            ],
        )]);
        let mut retry = Retrying::after_pass(&stub).await;
        retry.drive_until_posted(PROBED, 2).await;

        retry.run().move_to_session(SUCCESSOR_SESSION);
        stub.open_gate();
        retry.finish().await;

        assert_eq!(
            (stub.sessions_posted(PROBED).len(), retry.status_of(PROBED)),
            (2, failed("unauthorized"))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_the_run_mid_attempt_fails_the_topic_and_nothing_lands_afterwards() {
        let stub = ScriptedHelix::new([(
            PROBED,
            vec![
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                Answer::Gated(then(Answer::Created(RECOVERED_ID))),
            ],
        )]);
        let mut retry = Retrying::after_pass(&stub).await;
        retry.drive_until_posted(PROBED, 2).await;

        retry.run = None;
        stub.open_gate();
        tokio::time::sleep(ATTEMPT_CEILINGS.iter().sum::<Duration>()).await;

        assert_eq!(
            (
                retry.status_of(PROBED),
                retry.created_events_for(PROBED).len(),
                stub.sessions_posted(PROBED).len()
            ),
            (failed("HTTP 500"), 0, 2)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_retry_whose_row_a_newer_pass_rebuilt_stops_without_posting() {
        let stub = ScriptedHelix::new([(
            PROBED,
            vec![
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                Answer::Created("sub-newer-pass"),
            ],
        )]);
        let mut retry = Retrying::after_pass(&stub).await;
        let newer = Retrying::after_pass_on(&stub, Arc::clone(&retry.tracker)).await;
        assert!(newer.run.is_none());

        retry.finish().await;

        assert_eq!(
            (stub.sessions_posted(PROBED).len(), retry.record(PROBED)),
            (2, (SubStatus::Active, Some("sub-newer-pass".to_owned())))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_conflict_on_a_retry_counts_as_recovered_without_a_subscription_id() {
        let stub = ScriptedHelix::new([(
            PROBED,
            vec![
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                Answer::Status(StatusCode::CONFLICT),
            ],
        )]);
        let mut retry = Retrying::after_pass(&stub).await;

        retry.finish().await;

        assert_eq!(
            (retry.record(PROBED), retry.created_events_for(PROBED)),
            (
                (SubStatus::Active, None),
                vec![serde_json::json!({
                    "type": PROBED,
                    "subscription_id": null,
                    "condition": stub.condition_posted(PROBED),
                })]
            )
        );
    }

    #[test]
    fn a_conflict_on_a_retry_logs_the_recovery_at_info_without_a_warning() {
        let (_, lines) = crate::log_capture::capture_blocking(tracing::Level::INFO, async {
            tokio::time::pause();
            let stub = ScriptedHelix::new([(
                PROBED,
                vec![
                    Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                    Answer::Status(StatusCode::CONFLICT),
                ],
            )]);
            let mut retry = Retrying::after_pass(&stub).await;
            retry.finish().await;
        });

        let logged: Vec<_> = crate::log_capture::forge_lines(&lines)
            .into_iter()
            .map(|line| {
                (
                    line.level,
                    line.message().to_owned(),
                    line.field("kind").unwrap_or_default().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            logged,
            vec![(
                tracing::Level::INFO,
                "eventsub subscription recovered after retry".to_owned(),
                PROBED.to_owned(),
            )]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_conflict_on_the_first_attempt_stays_failed_and_starts_no_retry() {
        let stub = ScriptedHelix::new([(
            PROBED,
            vec![
                Answer::Status(StatusCode::CONFLICT),
                Answer::Created(RECOVERED_ID),
            ],
        )]);

        let retry = Retrying::after_pass(&stub).await;

        assert_eq!(
            (retry.run.is_none(), retry.status_of(PROBED)),
            (true, failed("already exists on another session"))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_revocation_of_a_conflict_recovered_topic_fails_its_row_by_kind() {
        let stub = ScriptedHelix::new([(
            PROBED,
            vec![
                Answer::Status(StatusCode::INTERNAL_SERVER_ERROR),
                Answer::Status(StatusCode::CONFLICT),
            ],
        )]);
        let mut retry = Retrying::after_pass(&stub).await;
        retry.finish().await;

        mark_revoked(&retry.tracker, "sub-unlisted", PROBED, "user_removed");

        assert_eq!(retry.status_of(PROBED), failed("revoked: user_removed"));
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_a_pass_midway_leaves_no_topic_retrying() {
        let stub = ScriptedHelix::new([
            (
                "channel.chat.message",
                vec![Answer::Gated(then(Answer::Created(RECOVERED_ID)))],
            ),
            (
                PROBED,
                vec![Answer::Status(StatusCode::INTERNAL_SERVER_ERROR)],
            ),
        ]);
        let tracker = SubscriptionTracker::default();
        let helix: Arc<dyn HelixTransport> = stub.clone();
        let publisher: Arc<dyn EventPublisher> = Arc::new(RecordingBus::default());
        let mut pass = Box::pin(subscribe_all(
            &helix,
            PASS_SESSION,
            "broadcaster",
            "user",
            &publisher,
            &tracker,
        ));
        let mut posts = stub.posts.subscribe();
        let every_topic_posted = tokio::select! {
            _ = &mut pass => false,
            every = posts.wait_for(|posts| posts.len() >= TOPICS.len()) => every.is_ok(),
        };
        assert!(
            every_topic_posted,
            "the gated topic must hold the pass open"
        );
        let status = |tracker: &SubscriptionTracker| {
            tracker
                .read()
                .unwrap()
                .iter()
                .find(|rec| rec.kind == PROBED)
                .map(|rec| rec.status.clone())
                .unwrap()
        };
        assert_eq!(status(&tracker), retrying("HTTP 500"));

        drop(pass);

        assert_eq!(
            status(&tracker),
            failed("HTTP 500"),
            "a pass torn down before it finished (session end, keepalive timeout, shutdown) \
             leaves no retry behind its Retrying rows"
        );
    }
}
