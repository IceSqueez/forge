use forge_types::RecoverMutex;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use forge_events::{Event, EventSource};
use forge_net::TaskOwner;
use forge_registry::chat_emote_codes;

use crate::bus::EventBus;
use crate::delivery::TWITCH_EMOTE_LEXICON;
use forge_events::Delivery;

pub(crate) const LEARNED_TWITCH_EMOTE_CAPACITY: usize = 4096;
const MAX_SIGHTINGS: usize = 2 * LEARNED_TWITCH_EMOTE_CAPACITY;

#[derive(Default)]
struct LearnedCodes {
    sightings: VecDeque<(u64, String)>,
    last_seen: HashMap<String, u64>,
    next_sighting: u64,
}

impl LearnedCodes {
    fn learn(&mut self, code: String) {
        let sighting = self.next_sighting;
        self.next_sighting += 1;
        self.last_seen.insert(code.clone(), sighting);
        self.sightings.push_back((sighting, code));
        while self.last_seen.len() > LEARNED_TWITCH_EMOTE_CAPACITY {
            let Some((sighting, code)) = self.sightings.pop_front() else {
                break;
            };
            if self.is_latest(sighting, &code) {
                self.last_seen.remove(&code);
            }
        }
        if self.sightings.len() > MAX_SIGHTINGS {
            let last_seen = &self.last_seen;
            self.sightings
                .retain(|(sighting, code)| last_seen.get(code) == Some(sighting));
        }
    }

    fn is_latest(&self, sighting: u64, code: &str) -> bool {
        self.last_seen.get(code) == Some(&sighting)
    }

    fn knows(&self, code: &str) -> bool {
        self.last_seen.contains_key(code)
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
            if learned.knows(word) && !found.iter().any(|code| code == word) {
                found.push(word.to_owned());
            }
        }
        found
    }

    fn learned(&self) -> MutexGuard<'_, LearnedCodes> {
        self.0.lock_recover()
    }
}

pub fn spawn_twitch_emote_learning(bus: &EventBus, lexicon: TwitchEmoteLexicon, tasks: &TaskOwner) {
    let mut events = bus.subscribe_observer(TWITCH_EMOTE_LEXICON);
    tasks.spawn_until_stopped(async move {
        loop {
            match events.next().await {
                Delivery::Event(event) => lexicon.learn_from(&event),
                Delivery::Skipped(_) => continue,
                Delivery::Closed => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use forge_types::ChatPayload;
    use serde_json::json;

    use super::*;
    use crate::NullEventLogRepo;

    fn chat_event(source: EventSource, codes: &[String]) -> Event {
        let segments: Vec<serde_json::Value> = codes
            .iter()
            .map(|code| json!({ "type": "emote", "id": "1", "name": code }))
            .collect();
        Event::new(
            source,
            "chat.message",
            json!({
                (ChatPayload::KEY): {
                    "platform_msg_id": "m-1",
                    "author": "NovaFox",
                    "author_color": null,
                    "segments": segments,
                    "badges": [],
                    "is_event": false,
                    "event_detail": null,
                }
            }),
        )
    }

    fn owned(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|code| (*code).to_owned()).collect()
    }

    fn code(n: usize) -> String {
        format!("emote{n}")
    }

    fn learned_from_twitch(codes: &[String]) -> TwitchEmoteLexicon {
        let lexicon = TwitchEmoteLexicon::default();
        lexicon.learn_from(&chat_event(EventSource::Twitch, codes));
        lexicon
    }

    fn knows(lexicon: &TwitchEmoteLexicon, code: &str) -> bool {
        !lexicon.codes_in(code).is_empty()
    }

    #[test]
    fn only_twitch_chat_teaches_emote_codes() {
        for (source, expected) in [
            (EventSource::Twitch, owned(&["LUL"])),
            (EventSource::Kick, vec![]),
            (EventSource::YouTube, vec![]),
        ] {
            let lexicon = TwitchEmoteLexicon::default();
            lexicon.learn_from(&chat_event(source, &owned(&["LUL"])));

            assert_eq!(lexicon.codes_in("LUL hi"), expected, "{source:?}");
        }
    }

    #[test]
    fn the_oldest_code_is_forgotten_only_once_capacity_is_exceeded() {
        for (learned, oldest_kept) in [
            (LEARNED_TWITCH_EMOTE_CAPACITY, true),
            (LEARNED_TWITCH_EMOTE_CAPACITY + 1, false),
        ] {
            let codes: Vec<String> = (0..learned).map(code).collect();

            let lexicon = learned_from_twitch(&codes);

            assert_eq!(knows(&lexicon, &code(0)), oldest_kept, "learned {learned}");
            assert!(knows(&lexicon, &code(1)), "learned {learned}");
            assert!(knows(&lexicon, &code(learned - 1)), "learned {learned}");
        }
    }

    #[test]
    fn seeing_a_code_again_makes_it_the_most_recent() {
        let mut codes: Vec<String> = (0..LEARNED_TWITCH_EMOTE_CAPACITY).map(code).collect();
        codes.push(code(0));
        codes.push("NewEmote".to_owned());

        let lexicon = learned_from_twitch(&codes);

        assert!(knows(&lexicon, &code(0)));
        assert!(!knows(&lexicon, &code(1)));
        assert!(knows(&lexicon, "NewEmote"));
    }

    #[test]
    fn codes_in_matches_whole_words_once_each_in_first_seen_order() {
        let lexicon = learned_from_twitch(&owned(&["LUL", "Kappa"]));

        assert_eq!(
            lexicon.codes_in("LULW Kappa lul LUL! LUL\u{3000}Kappa LUL"),
            owned(&["Kappa", "LUL"])
        );
    }

    #[tokio::test]
    async fn the_learning_task_learns_from_the_bus_until_stopped() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let lexicon = TwitchEmoteLexicon::default();
        let tasks = TaskOwner::new();
        spawn_twitch_emote_learning(&bus, lexicon.clone(), &tasks);

        bus.publish(chat_event(EventSource::Twitch, &owned(&["LUL"])));
        while lexicon.codes_in("LUL hi").is_empty() {
            tokio::task::yield_now().await;
        }

        let stopped = tasks.shutdown(std::time::Duration::from_secs(30)).await;

        assert_eq!(stopped, forge_net::TaskShutdown::Drained);
        assert_eq!(lexicon.codes_in("LUL hi"), owned(&["LUL"]));
    }
}
