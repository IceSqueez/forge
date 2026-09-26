use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_events::Event;
use forge_runtime::{Delivery, EventBus, EventSubscription};
use gpui::{AsyncApp, Entity};
use tokio::sync::Notify;

use crate::chat_feed::{ChatFeed, FeedGap};
use crate::event_loss::EventLoss;

pub const UI_CHAT_FEED: &str = "ui_chat_feed";

const INBOX_CAPACITY: usize = 4096;
const REPAINT_INTERVAL: Duration = Duration::from_millis(50);

pub enum FeedItem {
    Message(Arc<Event>),
    Annotation(Arc<Event>),
    Gap(FeedGap),
}

impl FeedItem {
    fn classify(event: Arc<Event>) -> Option<FeedItem> {
        if ChatFeed::is_message(&event) {
            Some(FeedItem::Message(event))
        } else if ChatFeed::annotates_rows(&event) {
            Some(FeedItem::Annotation(event))
        } else {
            None
        }
    }
}

/// Holds at most `capacity` events; overflow drops the oldest message into a gap in its place, and
/// an annotation only once no message is left, so moderation still reaches rows already on screen.
pub struct FeedInbox {
    items: VecDeque<FeedItem>,
    queued: usize,
    capacity: usize,
}

impl FeedInbox {
    pub fn new(capacity: usize) -> Self {
        Self {
            items: VecDeque::new(),
            queued: 0,
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, item: FeedItem) {
        match item {
            FeedItem::Gap(gap) => self.push_gap(gap),
            event => {
                self.items.push_back(event);
                self.queued += 1;
                while self.queued > self.capacity {
                    self.evict_oldest();
                }
            }
        }
    }

    pub fn take(&mut self) -> Vec<FeedItem> {
        self.queued = 0;
        self.items.drain(..).collect()
    }

    fn push_gap(&mut self, gap: FeedGap) {
        if gap.is_empty() {
            return;
        }
        match self.items.back_mut() {
            Some(FeedItem::Gap(last)) => last.absorb(gap),
            _ => self.items.push_back(FeedItem::Gap(gap)),
        }
    }

    fn evict_oldest(&mut self) {
        let victim = self
            .items
            .iter()
            .position(|item| matches!(item, FeedItem::Message(_)))
            .or_else(|| {
                self.items
                    .iter()
                    .position(|item| matches!(item, FeedItem::Annotation(_)))
            });
        let Some(at) = victim else {
            return;
        };
        let gap = match self.items.remove(at) {
            Some(FeedItem::Message(_)) => FeedGap {
                messages: 1,
                events: 0,
            },
            Some(FeedItem::Annotation(_)) => FeedGap {
                messages: 0,
                events: 1,
            },
            _ => return,
        };
        self.queued -= 1;
        self.place_gap(at, gap);
    }

    fn place_gap(&mut self, at: usize, gap: FeedGap) {
        let previous_is_gap = at
            .checked_sub(1)
            .is_some_and(|before| matches!(self.items.get(before), Some(FeedItem::Gap(_))));
        if !previous_is_gap {
            match self.items.get_mut(at) {
                Some(FeedItem::Gap(next)) => next.absorb(gap),
                _ => self.items.insert(at, FeedItem::Gap(gap)),
            }
            return;
        }
        let following = match self.items.get(at) {
            Some(FeedItem::Gap(_)) => match self.items.remove(at) {
                Some(FeedItem::Gap(next)) => next,
                _ => FeedGap::default(),
            },
            _ => FeedGap::default(),
        };
        if let Some(FeedItem::Gap(previous)) = self.items.get_mut(at - 1) {
            previous.absorb(gap);
            previous.absorb(following);
        }
    }
}

struct FeedChannel {
    inbox: Mutex<FeedInbox>,
    wake: Notify,
}

impl FeedChannel {
    fn push(&self, item: FeedItem) {
        self.inbox
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(item);
        self.wake.notify_one();
    }

    fn take(&self) -> Vec<FeedItem> {
        self.inbox.lock().unwrap_or_else(|p| p.into_inner()).take()
    }
}

/// Subscribes at construction so nothing published before `start` is missed.
pub struct ChatFeedBridge {
    channel: Arc<FeedChannel>,
}

impl ChatFeedBridge {
    pub fn subscribe(bus: &EventBus, rt_handle: &tokio::runtime::Handle) -> Self {
        let channel = Arc::new(FeedChannel {
            inbox: Mutex::new(FeedInbox::new(INBOX_CAPACITY)),
            wake: Notify::new(),
        });
        rt_handle.spawn(forward(
            bus.subscribe_observer(UI_CHAT_FEED),
            Arc::clone(&channel),
        ));
        Self { channel }
    }

    pub fn start(self, cx: &mut AsyncApp, feed: Entity<ChatFeed>, loss: Entity<EventLoss>) {
        let channel = self.channel;
        cx.spawn(async move |cx| {
            loop {
                channel.wake.notified().await;
                let items = channel.take();
                let dropped_here: u64 = items
                    .iter()
                    .map(|item| match item {
                        FeedItem::Gap(gap) => gap.messages,
                        FeedItem::Message(_) | FeedItem::Annotation(_) => 0,
                    })
                    .sum();
                feed.update(cx, |feed, cx| {
                    let mut changed = false;
                    for item in items {
                        changed |= match item {
                            FeedItem::Message(event) | FeedItem::Annotation(event) => {
                                feed.apply_event(&event)
                            }
                            FeedItem::Gap(gap) => feed.record_gap(gap),
                        };
                    }
                    if changed {
                        cx.notify();
                    }
                });
                loss.update(cx, |loss, cx| {
                    if loss.record_display_dropped(dropped_here) {
                        cx.notify();
                    }
                });
                cx.background_executor().timer(REPAINT_INTERVAL).await;
            }
        })
        .detach();
    }
}

async fn forward(mut subscription: EventSubscription, channel: Arc<FeedChannel>) {
    loop {
        match subscription.next().await {
            Delivery::Event(event) => {
                if let Some(item) = FeedItem::classify(event) {
                    channel.push(item);
                }
            }
            Delivery::Skipped(events) => channel.push(FeedItem::Gap(FeedGap {
                messages: 0,
                events,
            })),
            Delivery::Closed => break,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use forge_events::EventSource;
    use forge_runtime::{Config, NullEventLogRepo};
    use forge_types::{
        ChatModerationAction, ChatModerationPayload, ChatPayload, ChatSegment, EventId,
        ModerationMarks,
    };
    use gpui::{AppContext as _, TestAppContext};

    use super::*;

    const CAP: usize = 4;

    fn chat_event(msg_id: &str) -> Event {
        let payload = ChatPayload {
            platform_msg_id: msg_id.to_owned(),
            author: "bob".to_owned(),
            author_color: None,
            segments: vec![ChatSegment::Text {
                text: "hi".to_owned(),
            }],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        };
        Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({ ChatPayload::KEY: payload }),
        )
    }

    fn delete_event(msg_id: &str) -> Event {
        let payload = ChatModerationPayload {
            action: ChatModerationAction::DeleteMessage {
                message_id: msg_id.to_owned(),
            },
        };
        Event::new(
            EventSource::Twitch,
            "chat.moderation",
            serde_json::json!({ ChatModerationPayload::KEY: payload }),
        )
    }

    fn message(msg_id: &str) -> FeedItem {
        FeedItem::Message(Arc::new(chat_event(msg_id)))
    }

    fn annotation(msg_id: &str) -> FeedItem {
        FeedItem::Annotation(Arc::new(delete_event(msg_id)))
    }

    fn gap(messages: u64, events: u64) -> FeedItem {
        FeedItem::Gap(FeedGap { messages, events })
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Shape {
        Msg(String),
        Note,
        Gap(u64, u64),
    }

    fn shapes(items: Vec<FeedItem>) -> Vec<Shape> {
        items
            .into_iter()
            .map(|item| match item {
                FeedItem::Message(event) => Shape::Msg(
                    event.payload[ChatPayload::KEY]["platform_msg_id"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ),
                FeedItem::Annotation(_) => Shape::Note,
                FeedItem::Gap(gap) => Shape::Gap(gap.messages, gap.events),
            })
            .collect()
    }

    fn msg(id: &str) -> Shape {
        Shape::Msg(id.to_owned())
    }

    fn inbox_after(pushes: Vec<FeedItem>) -> Vec<Shape> {
        let mut inbox = FeedInbox::new(CAP);
        for item in pushes {
            inbox.push(item);
        }
        shapes(inbox.take())
    }

    #[test]
    fn an_inbox_filled_exactly_to_capacity_drops_nothing() {
        let pushed = (0..CAP).map(|i| message(&format!("m{i}"))).collect();

        assert_eq!(
            inbox_after(pushed),
            vec![msg("m0"), msg("m1"), msg("m2"), msg("m3")]
        );
    }

    #[test]
    fn overflow_drops_the_oldest_messages_into_one_leading_gap_with_the_exact_count() {
        for over in [1_usize, 3, 9] {
            let pushed = (0..CAP + over).map(|i| message(&format!("m{i}"))).collect();

            let mut expected = vec![Shape::Gap(over as u64, 0)];
            expected.extend((over..CAP + over).map(|i| msg(&format!("m{i}"))));
            assert_eq!(inbox_after(pushed), expected, "over={over}");
        }
    }

    #[test]
    fn an_evicted_annotation_is_not_counted_as_a_dropped_message() {
        let mut pushed = vec![annotation("m0")];
        pushed.extend((1..=CAP).map(|i| message(&format!("m{i}"))));

        assert_eq!(
            inbox_after(pushed),
            vec![msg("m1"), msg("m2"), msg("m3"), msg("m4")]
        );
    }

    #[test]
    fn eviction_folds_into_a_skip_gap_that_sits_behind_the_evicted_message() {
        let mut pushed = vec![message("m0"), gap(0, 5)];
        pushed.extend((1..=CAP).map(|i| message(&format!("m{i}"))));

        assert_eq!(
            inbox_after(pushed),
            vec![Shape::Gap(1, 5), msg("m1"), msg("m2"), msg("m3"), msg("m4")]
        );
    }

    #[test]
    fn back_to_back_skip_gaps_merge_but_a_message_between_them_keeps_them_apart() {
        assert_eq!(
            inbox_after(vec![gap(0, 2), gap(0, 3), message("m0"), gap(0, 4)]),
            vec![Shape::Gap(0, 5), msg("m0"), Shape::Gap(0, 4)]
        );
    }

    #[test]
    fn an_empty_gap_is_never_queued() {
        assert_eq!(
            inbox_after(vec![message("m0"), gap(0, 0), message("m1")]),
            vec![msg("m0"), msg("m1")]
        );
    }

    #[test]
    fn gaps_do_not_count_against_capacity() {
        let mut pushed = Vec::new();
        for i in 0..CAP {
            pushed.push(gap(0, 1));
            pushed.push(message(&format!("m{i}")));
        }

        assert_eq!(inbox_after(pushed).len(), 2 * CAP);
    }

    #[test]
    fn take_frees_the_whole_capacity_for_the_next_batch() {
        let mut inbox = FeedInbox::new(CAP);
        for i in 0..CAP {
            inbox.push(message(&format!("a{i}")));
        }
        inbox.take();
        for i in 0..CAP {
            inbox.push(message(&format!("b{i}")));
        }

        assert_eq!(
            shapes(inbox.take()),
            vec![msg("b0"), msg("b1"), msg("b2"), msg("b3")]
        );
    }

    fn channel() -> Arc<FeedChannel> {
        Arc::new(FeedChannel {
            inbox: Mutex::new(FeedInbox::new(INBOX_CAPACITY)),
            wake: Notify::new(),
        })
    }

    fn small_bus(observer_capacity: usize) -> Arc<EventBus> {
        let config = Config {
            bus_observer_capacity: observer_capacity,
            ..Config::default()
        };
        EventBus::with_config(Arc::new(NullEventLogRepo), &config)
    }

    #[tokio::test]
    async fn forward_keeps_chat_rows_and_their_annotations_in_order_and_drops_the_rest() {
        let bus = small_bus(64);
        let subscription = bus.subscribe_observer(UI_CHAT_FEED);
        let parent = EventId::new();
        bus.publish(chat_event("m0"));
        bus.publish(Event::new(
            EventSource::Core,
            "timer.tick",
            serde_json::json!({}),
        ));
        bus.publish(Event::caused_by(
            EventSource::Core,
            "command.matched",
            serde_json::json!({ "command": "!lurk" }),
            parent,
        ));
        bus.publish(Event::new(
            EventSource::Core,
            "command.matched",
            serde_json::json!({ "command": "!lurk" }),
        ));
        bus.publish(delete_event("m0"));
        bus.publish(chat_event("m1"));
        drop(bus);
        let channel = channel();

        forward(subscription, Arc::clone(&channel)).await;

        assert_eq!(
            shapes(channel.take()),
            vec![msg("m0"), Shape::Note, Shape::Note, msg("m1")]
        );
    }

    #[tokio::test]
    async fn forward_turns_observer_lag_into_an_event_gap_ahead_of_the_surviving_rows() {
        const OBSERVER_CAP: usize = 2;
        const PUBLISHED: usize = 5;
        let bus = small_bus(OBSERVER_CAP);
        let subscription = bus.subscribe_observer(UI_CHAT_FEED);
        for i in 0..PUBLISHED {
            bus.publish(chat_event(&format!("m{i}")));
        }
        drop(bus);
        let channel = channel();

        forward(subscription, Arc::clone(&channel)).await;

        assert_eq!(
            shapes(channel.take()),
            vec![
                Shape::Gap(0, (PUBLISHED - OBSERVER_CAP) as u64),
                msg("m3"),
                msg("m4")
            ]
        );
    }

    fn started_bridge(
        cx: &mut TestAppContext,
        items: Vec<FeedItem>,
    ) -> (Entity<ChatFeed>, Entity<EventLoss>) {
        let channel = channel();
        for item in items {
            channel.push(item);
        }
        let feed = cx.new(|_| ChatFeed::new());
        let loss = cx.new(|_| EventLoss::new());
        ChatFeedBridge { channel }.start(&mut cx.to_async(), feed.clone(), loss.clone());
        cx.run_until_parked();
        (feed, loss)
    }

    #[gpui::test]
    fn the_bridge_places_a_queued_gap_between_the_rows_it_arrived_between(cx: &mut TestAppContext) {
        let (feed, _) = started_bridge(cx, vec![message("m0"), gap(2, 1), message("m1")]);

        feed.read_with(cx, |feed, _| {
            let first = feed.start_seq();
            assert_eq!(
                (
                    feed.messages().len(),
                    feed.gap_before(None, first),
                    feed.gap_before(Some(first), first + 1),
                ),
                (
                    2,
                    FeedGap::default(),
                    FeedGap {
                        messages: 2,
                        events: 1
                    }
                )
            );
        });
    }

    #[gpui::test]
    fn the_bridge_adds_only_display_dropped_messages_to_the_loss_total(cx: &mut TestAppContext) {
        let (_, loss) = started_bridge(cx, vec![gap(3, 0), message("m0"), gap(0, 7)]);

        assert_eq!(loss.read_with(cx, |loss, _| loss.total()), 3);
    }

    fn feed_repaints_after(cx: &mut TestAppContext, item: FeedItem) -> usize {
        let channel = channel();
        channel.push(item);
        let feed = cx.new(|_| ChatFeed::new());
        let notified = Rc::new(Cell::new(0_usize));
        let _observer = cx.update(|cx| {
            let notified = Rc::clone(&notified);
            cx.observe(&feed, move |_, _| notified.set(notified.get() + 1))
        });

        ChatFeedBridge { channel }.start(
            &mut cx.to_async(),
            feed.clone(),
            cx.new(|_| EventLoss::new()),
        );
        cx.run_until_parked();
        notified.get()
    }

    #[gpui::test]
    fn a_batch_repaints_the_feed_only_when_it_changed_a_row(cx: &mut TestAppContext) {
        assert_eq!(
            (
                feed_repaints_after(cx, annotation("ghost")),
                feed_repaints_after(cx, message("m0")),
            ),
            (0, 1)
        );
    }
}
