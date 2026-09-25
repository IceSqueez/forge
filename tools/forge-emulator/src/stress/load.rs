use serde_json::{Value, json};

use super::marker::marker;
use super::profile::{Stimulus, StimulusKind};
use crate::fixture::TwitchAccount;
use crate::twitch::Viewer;

const FIRST_SENDER_USER_ID: u64 = 400_000_000;
pub const STRESS_REWARD_ID: &str = "stress-reward";
pub const STRESS_REWARD_TITLE: &str = "Hydrate";
pub const STRESS_CHEER_BITS: u64 = 500;
pub const STRESS_SUB_TIER: &str = "1000";
const CHATTER: [&str; 6] = [
    "hello chat",
    "that was a clean play",
    "LUL no way",
    "what game is this",
    "gg",
    "can we get a hype train going in here today",
];

/// What the generator hands the fake Twitch for one sequence number.
#[derive(Debug, Clone, PartialEq)]
pub enum Delivery {
    Chat {
        viewer: Viewer,
        text: String,
    },
    Notification {
        subscription_type: &'static str,
        event: Value,
    },
}

/// Deterministic: sequence `seq` always yields the same sender and payload.
pub fn delivery(
    stimulus: &Stimulus,
    seq: u64,
    senders: u32,
    broadcaster: &TwitchAccount,
) -> Delivery {
    let sender = seq % u64::from(senders.max(1)) + 1;
    let user_id = (FIRST_SENDER_USER_ID + sender).to_string();
    let login = format!("stress_{sender}");
    let display = format!("Stress{sender}{}", marker(seq));
    let tag = marker(seq);
    match stimulus.kind {
        StimulusKind::Chat => {
            let line = CHATTER[usize::try_from(seq % CHATTER.len() as u64).unwrap_or(0)];
            Delivery::Chat {
                viewer: Viewer::new(user_id, login),
                text: format!("{line} {tag}"),
            }
        }
        StimulusKind::Command => Delivery::Chat {
            viewer: Viewer::new(user_id, login),
            text: format!("{} {tag}", stimulus.text.as_deref().unwrap_or_default()),
        },
        kind => Delivery::Notification {
            subscription_type: kind.subscription_type(),
            event: notification_event(kind, seq, &user_id, &login, &display, broadcaster),
        },
    }
}

/// Payload shapes as the EventSub reference documents them for each subscription type.
fn notification_event(
    kind: StimulusKind,
    seq: u64,
    user_id: &str,
    login: &str,
    display: &str,
    broadcaster: &TwitchAccount,
) -> Value {
    let mut event = json!({
        "user_id": user_id,
        "user_login": login,
        "user_name": display,
        "broadcaster_user_id": broadcaster.user_id,
        "broadcaster_user_login": broadcaster.login,
        "broadcaster_user_name": broadcaster.login,
    });
    let extra = match kind {
        StimulusKind::Follow => json!({ "followed_at": "2026-09-25T10:00:00Z" }),
        StimulusKind::Subscribe => json!({ "tier": STRESS_SUB_TIER, "is_gift": false }),
        StimulusKind::Cheer => json!({
            "is_anonymous": false,
            "message": format!("cheer{STRESS_CHEER_BITS} {}", marker(seq)),
            "bits": STRESS_CHEER_BITS,
        }),
        StimulusKind::Reward => json!({
            "id": format!("stress-redemption-{seq}"),
            "user_input": "",
            "status": "unfulfilled",
            "reward": {
                "id": STRESS_REWARD_ID,
                "title": STRESS_REWARD_TITLE,
                "cost": 100,
                "prompt": "",
            },
            "redeemed_at": "2026-09-25T10:00:00Z",
        }),
        StimulusKind::Chat | StimulusKind::Command => json!({}),
    };
    if let (Some(event), Some(extra)) = (event.as_object_mut(), extra.as_object()) {
        event.extend(extra.clone());
    }
    event
}

/// Splits a flood between weighted stimuli by largest accumulated credit, so every prefix of the
/// sequence holds each stimulus close to its share and the order never depends on timing.
#[derive(Debug, Clone)]
pub struct FloodMix {
    weights: Vec<(usize, u64)>,
    credit: Vec<i64>,
    total: i64,
}

impl FloodMix {
    /// `stimuli` indexes are kept, so the pick names the profile's own stimulus.
    pub fn new(stimuli: &[Stimulus]) -> Self {
        let weights: Vec<(usize, u64)> = stimuli
            .iter()
            .enumerate()
            .filter_map(|(index, stimulus)| stimulus.weight.map(|w| (index, u64::from(w))))
            .filter(|(_, weight)| *weight > 0)
            .collect();
        let total = weights.iter().map(|(_, w)| *w).sum::<u64>();
        Self {
            credit: vec![0; weights.len()],
            weights,
            total: i64::try_from(total).unwrap_or(i64::MAX),
        }
    }

    pub fn next_pick(&mut self) -> Option<usize> {
        if self.weights.is_empty() {
            return None;
        }
        for (slot, (_, weight)) in self.weights.iter().enumerate() {
            self.credit[slot] += i64::try_from(*weight).unwrap_or(i64::MAX);
        }
        let (best, _) = self
            .credit
            .iter()
            .enumerate()
            .max_by(|(a_slot, a), (b_slot, b)| a.cmp(b).then(b_slot.cmp(a_slot)))?;
        self.credit[best] -= self.total;
        Some(self.weights[best].0)
    }
}

/// How many events a constant `per_second` rate owes after `elapsed_ms`.
pub fn due(per_second: f64, elapsed_ms: u64) -> u64 {
    if !(per_second.is_finite() && per_second > 0.0) {
        return 0;
    }
    let owed = per_second * elapsed_ms as f64 / 1_000.0;
    owed.floor() as u64
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::marker::find_marker;
    use super::*;

    fn stimulus(kind: StimulusKind, weight: Option<u32>, per_minute: Option<u32>) -> Stimulus {
        Stimulus {
            kind,
            weight,
            per_minute,
            text: Some("!hype".to_owned()),
            runs: vec!["Any".to_owned()],
        }
    }

    fn picks(mix: &mut FloodMix, count: usize) -> Vec<usize> {
        (0..count).map(|_| mix.next_pick().unwrap()).collect()
    }

    #[test]
    fn every_prefix_of_the_flood_holds_each_stimulus_within_one_of_its_share() {
        let stimuli = [
            stimulus(StimulusKind::Chat, Some(97), None),
            stimulus(StimulusKind::Command, Some(3), None),
        ];
        let mut mix = FloodMix::new(&stimuli);
        let sequence = picks(&mut mix, 1_000);
        for prefix in 1..=sequence.len() {
            let commands = sequence[..prefix].iter().filter(|pick| **pick == 1).count() as f64;
            let share = prefix as f64 * 3.0 / 100.0;
            assert!(
                (commands - share).abs() <= 1.0,
                "prefix {prefix}: {commands} commands"
            );
        }
    }

    #[test]
    fn steady_stimuli_are_never_picked_for_the_flood() {
        let stimuli = [
            stimulus(StimulusKind::Follow, None, Some(60)),
            stimulus(StimulusKind::Chat, Some(1), None),
        ];
        let mut mix = FloodMix::new(&stimuli);
        assert!(picks(&mut mix, 50).iter().all(|pick| *pick == 1));
    }

    #[test]
    fn a_flood_with_no_weighted_stimulus_picks_nothing() {
        let mut mix = FloodMix::new(&[stimulus(StimulusKind::Follow, None, Some(6))]);
        assert_eq!(mix.next_pick(), None);
    }

    #[test]
    fn owed_events_follow_the_rate_and_round_down() {
        for (per_second, elapsed_ms, owed) in [
            (50.0, 0, 0),
            (50.0, 1_000, 50),
            (1.0, 999, 0),
            (1.0, 1_000, 1),
            (0.2, 5_000, 1),
            (0.0, 10_000, 0),
            (-5.0, 10_000, 0),
            (f64::NAN, 10_000, 0),
            (f64::INFINITY, 10_000, 0),
        ] {
            assert_eq!(
                due(per_second, elapsed_ms),
                owed,
                "{per_second}/s at {elapsed_ms} ms"
            );
        }
    }

    #[test]
    fn senders_cycle_through_exactly_the_configured_crowd() {
        let chat = stimulus(StimulusKind::Chat, Some(1), None);
        let account = TwitchAccount::default();
        let user = |seq| match delivery(&chat, seq, 3, &account) {
            Delivery::Chat { viewer, .. } => viewer.user_id,
            other => panic!("chat stimulus delivered {other:?}"),
        };
        let first_cycle: Vec<String> = (0..3).map(user).collect();
        let mut distinct = first_cycle.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), 3);
        assert_eq!((3..6).map(user).collect::<Vec<_>>(), first_cycle);
    }

    #[test]
    fn the_same_sequence_always_yields_the_same_delivery() {
        let account = TwitchAccount::default();
        for kind in [StimulusKind::Chat, StimulusKind::Reward] {
            let definition = stimulus(kind, Some(1), None);
            assert_eq!(
                delivery(&definition, 42, 100, &account),
                delivery(&definition, 42, 100, &account)
            );
        }
    }

    #[test]
    fn every_kind_carries_its_sequence_marker_where_an_action_can_render_it() {
        let account = TwitchAccount::default();
        for kind in [
            StimulusKind::Chat,
            StimulusKind::Command,
            StimulusKind::Follow,
            StimulusKind::Subscribe,
            StimulusKind::Cheer,
            StimulusKind::Reward,
        ] {
            let traced = match delivery(&stimulus(kind, Some(1), None), 1234, 10, &account) {
                Delivery::Chat { text, .. } => text,
                Delivery::Notification { event, .. } => {
                    event["user_name"].as_str().unwrap().to_owned()
                }
            };
            assert_eq!(find_marker(&traced), Some(1234), "{kind:?}: {traced}");
        }
    }

    #[test]
    fn a_command_delivery_starts_with_its_phrase_so_the_command_trigger_matches() {
        let command = stimulus(StimulusKind::Command, Some(1), None);
        let Delivery::Chat { text, .. } = delivery(&command, 9, 10, &TwitchAccount::default())
        else {
            panic!("command delivered as a notification");
        };
        assert!(text.starts_with("!hype "), "{text}");
    }

    #[test]
    fn notifications_name_the_seeded_broadcaster_and_the_trigger_config_values() {
        let account = TwitchAccount::default();
        let event = |kind| match delivery(&stimulus(kind, Some(1), None), 5, 10, &account) {
            Delivery::Notification { event, .. } => event,
            other => panic!("{kind:?} delivered {other:?}"),
        };
        for kind in [
            StimulusKind::Follow,
            StimulusKind::Subscribe,
            StimulusKind::Cheer,
            StimulusKind::Reward,
        ] {
            assert_eq!(
                event(kind)["broadcaster_user_id"],
                account.user_id.as_str(),
                "{kind:?}"
            );
        }
        assert_eq!(
            event(StimulusKind::Reward)["reward"]["id"],
            STRESS_REWARD_ID
        );
        assert_eq!(event(StimulusKind::Cheer)["bits"], STRESS_CHEER_BITS);
        assert_eq!(event(StimulusKind::Subscribe)["tier"], STRESS_SUB_TIER);
    }
}
