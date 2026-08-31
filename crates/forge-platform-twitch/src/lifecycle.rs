use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use forge_platform_core::QuickActionLiveness;

use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

/// Twitch's fixed raid countdown - the upper bound on how long a locally recorded pending
/// raid can still be real.
const RAID_COUNTDOWN: Duration = Duration::from_secs(90);

const POLLS_PATH: &str = "/helix/polls";
const PREDICTIONS_PATH: &str = "/helix/predictions";
const STATUS_ACTIVE: &str = "ACTIVE";
const STATUS_LOCKED: &str = "LOCKED";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PollPhase {
    #[default]
    Unknown,
    Absent,
    Active,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PredictionPhase {
    #[default]
    Unknown,
    Absent,
    Active,
    Locked,
}

#[derive(Debug, Clone, Copy, Default)]
struct ChannelEntities {
    poll: PollPhase,
    prediction: PredictionPhase,
    raid_until: Option<Instant>,
}

/// Shared by the EventSub session that writes phases, the raid runners that record a local
/// pending raid, and the bundle that reads both while building quick-action descriptors.
#[derive(Clone, Default)]
pub struct TwitchLifecycle {
    entities: Arc<RwLock<ChannelEntities>>,
}

impl TwitchLifecycle {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn snapshot(&self) -> LifecycleSnapshot {
        let entities = *self.entities.read().unwrap_or_else(|p| p.into_inner());
        LifecycleSnapshot {
            poll: entities.poll,
            prediction: entities.prediction,
            raid_pending: entities
                .raid_until
                .is_some_and(|until| Instant::now() < until),
        }
    }

    pub(crate) fn apply_notification(
        &self,
        subscription_type: &str,
        event: &serde_json::Value,
        self_broadcaster_id: &str,
    ) {
        let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
        match subscription_type {
            "channel.poll.begin" | "channel.poll.progress" => entities.poll = PollPhase::Active,
            "channel.poll.end" => entities.poll = PollPhase::Absent,
            "channel.prediction.begin" => entities.prediction = PredictionPhase::Active,
            "channel.prediction.progress" => {
                if entities.prediction != PredictionPhase::Locked {
                    entities.prediction = PredictionPhase::Active;
                }
            }
            "channel.prediction.lock" => entities.prediction = PredictionPhase::Locked,
            "channel.prediction.end" => entities.prediction = PredictionPhase::Absent,
            "channel.raid"
                if !self_broadcaster_id.is_empty()
                    && raid_is_outgoing(event, self_broadcaster_id) =>
            {
                entities.raid_until = None;
            }
            _ => {}
        }
    }

    pub(crate) fn raid_started(&self) {
        let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
        entities.raid_until = Some(Instant::now() + RAID_COUNTDOWN);
    }

    pub(crate) fn raid_canceled(&self) {
        let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
        entities.raid_until = None;
    }

    /// The pending raid survives: it is a local countdown rather than a phase the notification
    /// stream could have corrected while it was down.
    pub(crate) fn forget_phases(&self) {
        let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
        entities.poll = PollPhase::Unknown;
        entities.prediction = PredictionPhase::Unknown;
    }

    /// A failed or unreadable response leaves the phase untouched, so a seeding outage stays
    /// fail-open instead of claiming the entity is gone.
    pub(crate) async fn seed_from_helix(
        &self,
        transport: &dyn HelixTransport,
        broadcaster_id: &str,
    ) {
        let polls = HelixRequest::new(HelixMethod::Get, POLLS_PATH)
            .query("broadcaster_id", broadcaster_id.to_owned());
        if let Ok(body) = transport.execute(polls).await
            && let Some(rows) = entity_rows(&body)
        {
            let phase = if rows_have_status(rows, STATUS_ACTIVE) {
                PollPhase::Active
            } else {
                PollPhase::Absent
            };
            let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
            entities.poll = phase;
        }

        let predictions = HelixRequest::new(HelixMethod::Get, PREDICTIONS_PATH)
            .query("broadcaster_id", broadcaster_id.to_owned());
        if let Ok(body) = transport.execute(predictions).await
            && let Some(rows) = entity_rows(&body)
        {
            let phase = if rows_have_status(rows, STATUS_ACTIVE) {
                PredictionPhase::Active
            } else if rows_have_status(rows, STATUS_LOCKED) {
                PredictionPhase::Locked
            } else {
                PredictionPhase::Absent
            };
            let mut entities = self.entities.write().unwrap_or_else(|p| p.into_inner());
            entities.prediction = phase;
        }
    }
}

fn raid_is_outgoing(event: &serde_json::Value, self_broadcaster_id: &str) -> bool {
    event
        .get("from_broadcaster_user_id")
        .and_then(|v| v.as_str())
        .is_some_and(|from| from == self_broadcaster_id)
}

fn entity_rows(body: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    body.get("data").and_then(|d| d.as_array())
}

fn rows_have_status(rows: &[serde_json::Value], wanted: &str) -> bool {
    rows.iter()
        .any(|row| row.get("status").and_then(|s| s.as_str()) == Some(wanted))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LifecycleSnapshot {
    poll: PollPhase,
    prediction: PredictionPhase,
    raid_pending: bool,
}

impl LifecycleSnapshot {
    pub(crate) fn poll_in_flight(self) -> QuickActionLiveness {
        match self.poll {
            PollPhase::Active => QuickActionLiveness::Live,
            PollPhase::Absent => QuickActionLiveness::Absent,
            PollPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    pub(crate) fn poll_slot_free(self) -> QuickActionLiveness {
        match self.poll {
            PollPhase::Active => QuickActionLiveness::Absent,
            PollPhase::Absent | PollPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    pub(crate) fn prediction_lockable(self) -> QuickActionLiveness {
        match self.prediction {
            PredictionPhase::Active => QuickActionLiveness::Live,
            PredictionPhase::Absent | PredictionPhase::Locked => QuickActionLiveness::Absent,
            PredictionPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    pub(crate) fn prediction_settleable(self) -> QuickActionLiveness {
        match self.prediction {
            PredictionPhase::Active | PredictionPhase::Locked => QuickActionLiveness::Live,
            PredictionPhase::Absent => QuickActionLiveness::Absent,
            PredictionPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    pub(crate) fn prediction_slot_free(self) -> QuickActionLiveness {
        match self.prediction {
            PredictionPhase::Active | PredictionPhase::Locked => QuickActionLiveness::Absent,
            PredictionPhase::Absent | PredictionPhase::Unknown => QuickActionLiveness::Unknown,
        }
    }

    /// Never `Absent`: a raid can be started outside forge, so the absence of a local pending
    /// raid is not knowledge that none is running.
    pub(crate) fn raid_in_flight(self) -> QuickActionLiveness {
        if self.raid_pending {
            QuickActionLiveness::Live
        } else {
            QuickActionLiveness::Unknown
        }
    }

    pub(crate) fn raid_slot_free(self) -> QuickActionLiveness {
        if self.raid_pending {
            QuickActionLiveness::Absent
        } else {
            QuickActionLiveness::Unknown
        }
    }
}
