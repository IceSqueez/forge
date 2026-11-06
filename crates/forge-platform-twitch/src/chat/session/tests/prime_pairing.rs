use serde_json::{Value, json};

use super::*;
use crate::subscriptions::{SubStatus, SubscriptionFailure, SubscriptionRecord};

const SUBSCRIBE_KIND: &str = "twitch.channel.subscribe";
const RESUBSCRIBE_KIND: &str = "twitch.channel.subscription.message";
const NOTIFICATION_TOPIC: &str = "channel.chat.notification";
const PAIRING_WAIT: Duration = Duration::from_secs(2);
const SUBSCRIBER: &str = "7001";
const OTHER_SUBSCRIBER: &str = "7002";
const GUEST_CHANNEL: &str = "6001";

#[derive(Debug, Clone, Copy)]
enum Sub {
    New,
    Renewed,
}

impl Sub {
    const BOTH: [Self; 2] = [Self::New, Self::Renewed];

    fn deliver(self, session: &ChatSession, user: &str) {
        match self {
            Self::New => session.publish_subscribe_event(&sub_wire(user, false), "sub-frame"),
            Self::Renewed => session.publish_resubscribe_event(&resub_wire(user), "resub-frame"),
        }
    }

    fn notice_type(self) -> &'static str {
        match self {
            Self::New => "sub",
            Self::Renewed => "resub",
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Self::New => SUBSCRIBE_KIND,
            Self::Renewed => RESUBSCRIBE_KIND,
        }
    }

    fn other(self) -> Self {
        match self {
            Self::New => Self::Renewed,
            Self::Renewed => Self::New,
        }
    }
}

fn sub_wire(user: &str, is_gift: bool) -> Value {
    json!({
        "user_id": user,
        "user_login": format!("login_{user}"),
        "user_name": format!("Name{user}"),
        "tier": "1000",
        "is_gift": is_gift,
    })
}

fn resub_wire(user: &str) -> Value {
    json!({
        "user_id": user,
        "user_login": format!("login_{user}"),
        "user_name": format!("Name{user}"),
        "tier": "1000",
        "cumulative_months": 4,
        "streak_months": 2,
        "message": {"text": "four months"},
        "share_streak": true,
    })
}

fn notice_wire(user: &str, notice_type: &str, is_prime: bool, source: Value) -> Value {
    json!({
        "broadcaster_user_id": "bcast",
        "chatter_user_id": user,
        "notice_type": notice_type,
        notice_type: {"sub_tier": "1000", "is_prime": is_prime},
        "source_broadcaster_user_id": source,
    })
}

fn own_notice(user: &str, notice_type: &str, is_prime: bool) -> Value {
    notice_wire(user, notice_type, is_prime, Value::Null)
}

fn tracker_with_notice_topic(status: Option<SubStatus>) -> SubscriptionTracker {
    let tracker = SubscriptionTracker::default();
    tracker.reset(
        status
            .into_iter()
            .map(|status| SubscriptionRecord {
                kind: NOTIFICATION_TOPIC.to_owned(),
                direction: None,
                version: "1".to_owned(),
                status,
                subscription_id: None,
            })
            .collect(),
    );
    tracker
}

struct BusTap {
    events: EventSubscription,
    _bus: Arc<EventChannel>,
}

fn session_tracking(status: Option<SubStatus>) -> (ChatSession, BusTap) {
    let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
    let events = BusTap {
        events: bus.subscribe(),
        _bus: Arc::clone(&bus),
    };
    let publisher: Arc<dyn EventPublisher> = bus;
    let (session, _state, _shutdown) = session_over(
        publisher,
        Arc::new(MockCreds::with_identity()),
        None,
        tracker_with_notice_topic(status),
        crate::test_support::unreachable_twitch_endpoints(),
    );
    (session, events)
}

fn live_notice_session() -> (ChatSession, BusTap) {
    session_tracking(Some(SubStatus::Active))
}

fn published_subs(tap: &mut BusTap) -> Vec<(String, bool)> {
    let mut subs = Vec::new();
    while let Some(event) = tap.events.try_recv().expect("the bus must stay open") {
        if event.kind == SUBSCRIBE_KIND || event.kind == RESUBSCRIBE_KIND {
            let is_prime = event.payload["is_prime"]
                .as_bool()
                .expect("a published sub must carry its prime flag");
            subs.push((event.kind.clone(), is_prime));
        }
    }
    subs
}

fn one(sub: Sub, is_prime: bool) -> Vec<(String, bool)> {
    vec![(sub.kind().to_owned(), is_prime)]
}

#[tokio::test(start_paused = true)]
async fn a_sub_followed_by_its_notice_publishes_once_with_the_notice_prime_flag() {
    for (sub, is_prime) in Sub::BOTH
        .into_iter()
        .flat_map(|sub| [(sub, true), (sub, false)])
    {
        let (session, mut events) = live_notice_session();
        sub.deliver(&session, SUBSCRIBER);
        session.pair_chat_notification(
            &own_notice(SUBSCRIBER, sub.notice_type(), is_prime),
            "notice-frame",
        );

        assert_eq!(
            published_subs(&mut events),
            one(sub, is_prime),
            "{sub:?} prime {is_prime}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_notice_followed_by_its_sub_publishes_once_with_the_notice_prime_flag() {
    for (sub, is_prime) in Sub::BOTH
        .into_iter()
        .flat_map(|sub| [(sub, true), (sub, false)])
    {
        let (session, mut events) = live_notice_session();
        session.pair_chat_notification(
            &own_notice(SUBSCRIBER, sub.notice_type(), is_prime),
            "notice-frame",
        );
        sub.deliver(&session, SUBSCRIBER);

        assert_eq!(
            published_subs(&mut events),
            one(sub, is_prime),
            "{sub:?} prime {is_prime}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_sub_without_a_notice_stays_unpublished_until_its_wait_ends() {
    for sub in Sub::BOTH {
        let (session, mut events) = live_notice_session();
        sub.deliver(&session, SUBSCRIBER);
        tokio::time::advance(PAIRING_WAIT - Duration::from_millis(1)).await;
        session.release_expired_subs();

        assert_eq!(published_subs(&mut events), Vec::new(), "{sub:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_sub_without_a_notice_publishes_unflagged_once_its_wait_ends() {
    for sub in Sub::BOTH {
        let (session, mut events) = live_notice_session();
        sub.deliver(&session, SUBSCRIBER);
        tokio::time::advance(PAIRING_WAIT).await;
        session.release_expired_subs();

        assert_eq!(published_subs(&mut events), one(sub, false), "{sub:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn the_session_schedules_its_next_release_at_the_end_of_the_sub_wait() {
    let (session, _events) = live_notice_session();
    let delivered_at = Instant::now();
    Sub::New.deliver(&session, SUBSCRIBER);

    assert_eq!(
        session.next_sub_release(),
        Some(delivered_at + PAIRING_WAIT)
    );
}

#[tokio::test(start_paused = true)]
async fn a_sub_publishes_at_once_unflagged_while_the_notice_topic_is_not_live() {
    let not_live = [
        None,
        Some(SubStatus::Pending),
        Some(SubStatus::Retrying(SubscriptionFailure::Transport)),
        Some(SubStatus::Failed(SubscriptionFailure::Refused(
            reqwest::StatusCode::BAD_REQUEST.as_u16(),
        ))),
        Some(SubStatus::Failed(SubscriptionFailure::Revoked(
            "authorization_revoked".to_owned(),
        ))),
    ];
    for status in not_live {
        for sub in Sub::BOTH {
            let (session, mut events) = session_tracking(status.clone());
            sub.deliver(&session, SUBSCRIBER);

            assert_eq!(
                published_subs(&mut events),
                one(sub, false),
                "{sub:?} with the notice topic {status:?}"
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_gift_sub_publishes_at_once_unflagged_and_ignores_a_later_prime_notice() {
    let (session, mut events) = live_notice_session();
    session.publish_subscribe_event(&sub_wire(SUBSCRIBER, true), "gift-frame");
    session.pair_chat_notification(&own_notice(SUBSCRIBER, "sub", true), "notice-frame");

    assert_eq!(published_subs(&mut events), one(Sub::New, false));
}

#[tokio::test(start_paused = true)]
async fn a_shared_chat_notice_never_pairs() {
    for sub in Sub::BOTH {
        let shared_chat_type = format!("shared_chat_{}", sub.notice_type());
        let notices = [
            notice_wire(SUBSCRIBER, sub.notice_type(), true, json!(GUEST_CHANNEL)),
            own_notice(SUBSCRIBER, &shared_chat_type, true),
            notice_wire(SUBSCRIBER, &shared_chat_type, true, json!(GUEST_CHANNEL)),
        ];
        for notice in notices {
            let (session, mut events) = live_notice_session();
            sub.deliver(&session, SUBSCRIBER);
            session.pair_chat_notification(&notice, "notice-frame");
            session.release_waiting_subs();

            assert_eq!(
                published_subs(&mut events),
                one(sub, false),
                "{sub:?} paired with {notice}"
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_notice_with_an_empty_source_broadcaster_pairs_as_the_own_channel() {
    let (session, mut events) = live_notice_session();
    Sub::New.deliver(&session, SUBSCRIBER);
    session.pair_chat_notification(
        &notice_wire(SUBSCRIBER, "sub", true, json!("")),
        "notice-frame",
    );

    assert_eq!(published_subs(&mut events), one(Sub::New, true));
}

#[tokio::test(start_paused = true)]
async fn a_notice_of_a_non_sub_type_never_pairs() {
    for notice_type in [
        "sub_gift",
        "community_sub_gift",
        "gift_paid_upgrade",
        "prime_paid_upgrade",
        "raid",
        "announcement",
        "unknown",
        "Sub",
        "",
    ] {
        let (session, mut events) = live_notice_session();
        Sub::New.deliver(&session, SUBSCRIBER);
        session.pair_chat_notification(&own_notice(SUBSCRIBER, notice_type, true), "notice-frame");
        session.release_waiting_subs();

        assert_eq!(
            published_subs(&mut events),
            one(Sub::New, false),
            "notice type {notice_type:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_notice_of_the_other_sub_kind_never_pairs() {
    for sub in Sub::BOTH {
        let (session, mut events) = live_notice_session();
        sub.deliver(&session, SUBSCRIBER);
        session.pair_chat_notification(
            &own_notice(SUBSCRIBER, sub.other().notice_type(), true),
            "notice-frame",
        );
        session.release_waiting_subs();

        assert_eq!(published_subs(&mut events), one(sub, false), "{sub:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_notice_of_another_chatter_never_pairs() {
    let (session, mut events) = live_notice_session();
    Sub::New.deliver(&session, SUBSCRIBER);
    session.pair_chat_notification(&own_notice(OTHER_SUBSCRIBER, "sub", true), "notice-frame");
    session.release_waiting_subs();

    assert_eq!(published_subs(&mut events), one(Sub::New, false));
}

#[tokio::test(start_paused = true)]
async fn ending_a_session_publishes_every_waiting_sub_unflagged() {
    let (session, mut events) = live_notice_session();
    Sub::New.deliver(&session, SUBSCRIBER);
    Sub::Renewed.deliver(&session, OTHER_SUBSCRIBER);
    session.release_waiting_subs();

    assert_eq!(
        published_subs(&mut events),
        vec![
            (SUBSCRIBE_KIND.to_owned(), false),
            (RESUBSCRIBE_KIND.to_owned(), false)
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn dropping_the_session_publishes_its_waiting_sub_unflagged() {
    let (session, mut events) = live_notice_session();
    Sub::Renewed.deliver(&session, SUBSCRIBER);
    drop(session);

    assert_eq!(published_subs(&mut events), one(Sub::Renewed, false));
}

#[tokio::test(start_paused = true)]
async fn a_paired_sub_publishes_exactly_once_across_expiry_session_end_and_a_resent_notice() {
    let (session, mut events) = live_notice_session();
    Sub::New.deliver(&session, SUBSCRIBER);
    session.pair_chat_notification(&own_notice(SUBSCRIBER, "sub", true), "notice-frame");
    tokio::time::advance(PAIRING_WAIT).await;
    session.release_expired_subs();
    session.pair_chat_notification(&own_notice(SUBSCRIBER, "sub", true), "notice-frame-resent");
    session.release_waiting_subs();
    drop(session);

    assert_eq!(published_subs(&mut events), one(Sub::New, true));
}

fn notification_frame(message_id: &str, subscription_type: &str, event: Value) -> String {
    json!({
        "metadata": {
            "message_type": "notification",
            "message_id": message_id,
            "subscription_type": subscription_type,
        },
        "payload": { "event": event },
    })
    .to_string()
}

async fn connected_peer_with_live_notice_topic() -> (RunningSession, EventSubscription, Peer) {
    let api = eventsub_api().await;
    let mut socket = FakeEventSub::bind().await;
    let (session, tracker) = spawn_tracked_session(&socket.url, &api.uri());
    tracker.reset(tracker_with_notice_topic(Some(SubStatus::Active)).snapshot());
    let events = session.bus.subscribe();
    let peer = socket.accept().await;
    (session, events, peer)
}

async fn events_until_sub(events: &mut EventSubscription) -> Vec<Event> {
    let mut seen = Vec::new();
    let arrived = within_hang_guard(FAKE_WAIT, async {
        loop {
            let event = events.recv().await.expect("the bus must stay open");
            let is_sub = event.kind == SUBSCRIBE_KIND;
            seen.push(event);
            if is_sub {
                return;
            }
        }
    })
    .await;
    assert!(arrived.is_ok(), "no sub was published within {FAKE_WAIT:?}");
    seen
}

#[tokio::test]
async fn a_live_session_publishes_a_sub_left_without_a_notice_while_its_socket_stays_open() {
    let (_session, mut events, mut peer) = connected_peer_with_live_notice_topic().await;
    peer.send(notification_frame(
        "sub-1",
        "channel.subscribe",
        sub_wire(SUBSCRIBER, false),
    ));

    let seen = events_until_sub(&mut events).await;
    peer.wait_until_forge_read_everything().await;

    assert_eq!(
        seen.last().map(|event| &event.payload["is_prime"]),
        Some(&json!(false))
    );
}

#[tokio::test]
async fn a_session_that_ends_publishes_its_waiting_sub_before_it_reconnects() {
    let (_session, mut events, mut peer) = connected_peer_with_live_notice_topic().await;
    peer.send(notification_frame(
        "sub-1",
        "channel.subscribe",
        sub_wire(SUBSCRIBER, false),
    ));
    peer.wait_until_forge_read_everything().await;
    peer.close();

    let reconnecting_before_the_sub: Vec<ConnectionState> = events_until_sub(&mut events)
        .await
        .iter()
        .filter(|event| event.kind == PlatformConnectionChanged::KIND)
        .filter_map(|event| serde_json::from_value(event.payload["state"].clone()).ok())
        .filter(|state| *state == ConnectionState::Reconnecting)
        .collect();

    assert_eq!(reconnecting_before_the_sub, Vec::new());
}
