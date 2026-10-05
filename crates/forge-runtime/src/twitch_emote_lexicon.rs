use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use forge_events::{Event, EventSource};
use forge_registry::chat_emote_codes;
use tokio::task::JoinHandle;

use crate::bus::{Delivery, EventBus};

pub const LEARNED_TWITCH_EMOTE_CAPACITY: usize = 4096;
const LEXICON_OBSERVER: &str = "twitch-emote-lexicon";

#[derive(Default)]
struct LearnedCodes {
    recency: VecDeque<String>,
    known: HashSet<String>,
}

impl LearnedCodes {
    fn learn(&mut self, code: String) {
        if self.known.contains(&code) {
            if let Some(position) = self.recency.iter().position(|seen| *seen == code) {
                self.recency.remove(position);
            }
        } else {
            self.known.insert(code.clone());
        }
        self.recency.push_back(code);
        while self.recency.len() > LEARNED_TWITCH_EMOTE_CAPACITY {
            if let Some(evicted) = self.recency.pop_front() {
                self.known.remove(&evicted);
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct TwitchEmoteLexicon(Arc<Mutex<LearnedCodes>>);

impl TwitchEmoteLexicon {
    pub fn learn_from(&self, event: &Event) {
        if event.source != EventSource::Twitch {
            return;
        }
        let codes = chat_emote_codes(event);
        if codes.is_empty() {
            return;
        }
        let mut learned = self.learned();
        for code in codes {
            learned.learn(code);
        }
    }

    pub fn codes_in(&self, text: &str) -> Vec<String> {
        let learned = self.learned();
        let mut found: Vec<String> = Vec::new();
        for word in text.split_whitespace() {
            if learned.known.contains(word) && !found.iter().any(|code| code == word) {
                found.push(word.to_owned());
            }
        }
        found
    }

    fn learned(&self) -> MutexGuard<'_, LearnedCodes> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub fn spawn_twitch_emote_learning(bus: &EventBus, lexicon: TwitchEmoteLexicon) -> JoinHandle<()> {
    let mut events = bus.subscribe_observer(LEXICON_OBSERVER);
    tokio::spawn(async move {
        loop {
            match events.next().await {
                Delivery::Event(event) => lexicon.learn_from(&event),
                Delivery::Skipped(_) => continue,
                Delivery::Closed => break,
            }
        }
    })
}
