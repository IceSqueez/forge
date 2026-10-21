use std::collections::HashMap;
use std::time::Duration;

use tokio::time::Instant;

use super::intent::NotificationIntent;

pub const CONNECTION_LOSS_GRACE: Duration = Duration::from_secs(10);
pub const CONNECTION_STABLE_REARM: Duration = Duration::from_secs(60);
pub const MIN_CUE_SPACING: Duration = Duration::from_secs(30);

struct LinkTrack {
    connected_since: Option<Instant>,
    lost_at: Option<Instant>,
    armed: bool,
}

#[derive(Default)]
pub(super) struct ConnectionLossGate {
    links: HashMap<String, LinkTrack>,
    last_cue: Option<Instant>,
}

impl ConnectionLossGate {
    pub(super) fn connected(&mut self, platform_id: String, now: Instant) {
        let link = self.links.entry(platform_id).or_insert(LinkTrack {
            connected_since: None,
            lost_at: None,
            armed: true,
        });
        link.connected_since.get_or_insert(now);
        link.lost_at = None;
    }

    pub(super) fn disconnected(&mut self, platform_id: &str, expected: bool, now: Instant) {
        let Some(link) = self.links.get_mut(platform_id) else {
            return;
        };
        if expected {
            link.connected_since = None;
            link.lost_at = None;
            return;
        }
        let Some(since) = link.connected_since.take() else {
            return;
        };
        if now.saturating_duration_since(since) >= CONNECTION_STABLE_REARM {
            link.armed = true;
        }
        if link.armed {
            link.lost_at = Some(now);
        }
    }

    pub(super) fn flush_due(&mut self, now: Instant, out: &mut Vec<NotificationIntent>) {
        if self.spacing_ends().is_some_and(|ends| now < ends) {
            return;
        }
        let mut lost: Vec<String> = self
            .links
            .iter_mut()
            .filter(|(_, link)| {
                link.lost_at
                    .is_some_and(|at| at + CONNECTION_LOSS_GRACE <= now)
            })
            .map(|(platform_id, link)| {
                link.lost_at = None;
                link.armed = false;
                platform_id.clone()
            })
            .collect();
        if lost.is_empty() {
            return;
        }
        lost.sort();
        self.last_cue = Some(now);
        out.push(NotificationIntent::ConnectionLossCue { platform_ids: lost });
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        let grace_ends = self
            .links
            .values()
            .filter_map(|link| link.lost_at)
            .min()
            .map(|at| at + CONNECTION_LOSS_GRACE)?;
        Some(
            self.spacing_ends()
                .map_or(grace_ends, |ends| grace_ends.max(ends)),
        )
    }

    fn spacing_ends(&self) -> Option<Instant> {
        self.last_cue.map(|at| at + MIN_CUE_SPACING)
    }
}
