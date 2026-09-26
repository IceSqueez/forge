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

/// Holds at most `capacity` events; overflow drops the oldest and folds each dropped message into a gap at the front.
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
        let mut gap = FeedGap::default();
        while let Some(item) = self.items.pop_front() {
            match item {
                FeedItem::Gap(leading) => gap.absorb(leading),
                FeedItem::Message(_) => {
                    gap.messages += 1;
                    self.queued -= 1;
                    break;
                }
                FeedItem::Annotation(_) => {
                    self.queued -= 1;
                    break;
                }
            }
        }
        if gap.is_empty() {
            return;
        }
        match self.items.front_mut() {
            Some(FeedItem::Gap(next)) => next.absorb(gap),
            _ => self.items.push_front(FeedItem::Gap(gap)),
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
