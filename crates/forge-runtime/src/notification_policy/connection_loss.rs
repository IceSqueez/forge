use std::collections::HashMap;
use std::time::Duration;

use tokio::time::Instant;

use super::intent::NotificationIntent;

pub(crate) const CONNECTION_LOSS_GRACE: Duration = Duration::from_secs(10);
pub(crate) const CONNECTION_STABLE_REARM: Duration = Duration::from_secs(60);
pub(crate) const MIN_CUE_SPACING: Duration = Duration::from_secs(30);

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

    pub(super) fn discard_due(&mut self, now: Instant) {
        for link in self.links.values_mut() {
            if link
                .lost_at
                .is_some_and(|at| at + CONNECTION_LOSS_GRACE <= now)
            {
                link.lost_at = None;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);
    const TWITCH: &str = "twitch";
    const KICK: &str = "kick";

    fn flush(gate: &mut ConnectionLossGate, at: Instant) -> Vec<NotificationIntent> {
        let mut out = Vec::new();
        gate.flush_due(at, &mut out);
        out
    }

    fn cue(platform_ids: &[&str]) -> Vec<NotificationIntent> {
        vec![NotificationIntent::ConnectionLossCue {
            platform_ids: platform_ids.iter().map(|id| (*id).to_owned()).collect(),
        }]
    }

    fn connected_gate(platforms: &[&str], at: Instant) -> ConnectionLossGate {
        let mut gate = ConnectionLossGate::default();
        for platform in platforms {
            gate.connected((*platform).to_owned(), at);
        }
        gate
    }

    #[test]
    fn unexpected_loss_cues_exactly_when_the_grace_period_ends() {
        for (wait, cues) in [
            (CONNECTION_LOSS_GRACE - Duration::from_millis(1), false),
            (CONNECTION_LOSS_GRACE, true),
        ] {
            let t0 = Instant::now();
            let mut gate = connected_gate(&[TWITCH], t0);
            gate.disconnected(TWITCH, false, t0);
            assert_eq!(
                flush(&mut gate, t0 + wait) == cue(&[TWITCH]),
                cues,
                "wait {wait:?}"
            );
        }
    }

    #[test]
    fn deadline_tracks_the_end_of_the_grace_period() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH], t0);
        gate.disconnected(TWITCH, false, t0 + SECOND);
        assert_eq!(gate.deadline(), Some(t0 + SECOND + CONNECTION_LOSS_GRACE));
    }

    #[test]
    fn recovery_within_grace_cancels_the_cue() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH], t0);
        gate.disconnected(TWITCH, false, t0);
        gate.connected(TWITCH.to_owned(), t0 + CONNECTION_LOSS_GRACE - SECOND);
        assert!(flush(&mut gate, t0 + 2 * CONNECTION_LOSS_GRACE).is_empty());
        assert_eq!(gate.deadline(), None);
    }

    #[test]
    fn expected_drop_never_cues() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH], t0);
        gate.disconnected(TWITCH, true, t0);
        assert!(flush(&mut gate, t0 + 2 * CONNECTION_LOSS_GRACE).is_empty());
    }

    #[test]
    fn expected_drop_during_grace_cancels_the_pending_cue() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH], t0);
        gate.disconnected(TWITCH, false, t0);
        gate.disconnected(TWITCH, true, t0 + SECOND);
        assert!(flush(&mut gate, t0 + CONNECTION_LOSS_GRACE).is_empty());
    }

    #[test]
    fn drop_of_a_never_connected_link_is_ignored() {
        let t0 = Instant::now();
        let mut gate = ConnectionLossGate::default();
        gate.disconnected(TWITCH, false, t0);
        assert_eq!(gate.deadline(), None);
    }

    #[test]
    fn repeated_down_states_measure_grace_from_the_first_drop() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH], t0);
        gate.disconnected(TWITCH, false, t0);
        gate.disconnected(TWITCH, false, t0 + 5 * SECOND);
        assert_eq!(flush(&mut gate, t0 + CONNECTION_LOSS_GRACE), cue(&[TWITCH]));
    }

    #[test]
    fn link_rearms_only_after_staying_connected_for_the_stability_window() {
        for (stable_for, cues_again) in [
            (CONNECTION_STABLE_REARM - SECOND, false),
            (CONNECTION_STABLE_REARM, true),
        ] {
            let t0 = Instant::now();
            let mut gate = connected_gate(&[TWITCH], t0);
            gate.disconnected(TWITCH, false, t0);
            flush(&mut gate, t0 + CONNECTION_LOSS_GRACE);
            let back = t0 + CONNECTION_LOSS_GRACE + SECOND;
            gate.connected(TWITCH.to_owned(), back);
            let second_drop = back + stable_for;
            gate.disconnected(TWITCH, false, second_drop);
            let second = flush(&mut gate, second_drop + CONNECTION_LOSS_GRACE);
            assert_eq!(
                second == cue(&[TWITCH]),
                cues_again,
                "stable for {stable_for:?}"
            );
        }
    }

    #[test]
    fn flapping_link_cues_once_per_outage() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH], t0);
        let mut cues = 0;
        let mut at = t0;
        for _ in 0..5 {
            gate.disconnected(TWITCH, false, at);
            at += CONNECTION_LOSS_GRACE + SECOND;
            cues += flush(&mut gate, at).len();
            gate.connected(TWITCH.to_owned(), at);
            at += 5 * SECOND;
        }
        assert_eq!(cues, 1);
    }

    #[test]
    fn simultaneous_losses_collapse_into_one_sorted_cue() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH, KICK], t0);
        gate.disconnected(TWITCH, false, t0);
        gate.disconnected(KICK, false, t0);
        assert_eq!(
            flush(&mut gate, t0 + CONNECTION_LOSS_GRACE),
            cue(&[KICK, TWITCH])
        );
    }

    #[test]
    fn cues_are_spaced_by_the_minimum_cue_spacing() {
        for (gap, cues) in [(MIN_CUE_SPACING - SECOND, false), (MIN_CUE_SPACING, true)] {
            let t0 = Instant::now();
            let mut gate = connected_gate(&[TWITCH, KICK], t0);
            gate.disconnected(TWITCH, false, t0);
            let first_cue = t0 + CONNECTION_LOSS_GRACE;
            flush(&mut gate, first_cue);
            gate.disconnected(KICK, false, first_cue + gap - CONNECTION_LOSS_GRACE);
            assert_eq!(
                flush(&mut gate, first_cue + gap) == cue(&[KICK]),
                cues,
                "gap {gap:?}"
            );
        }
    }

    #[test]
    fn discard_drops_only_losses_whose_grace_has_ended() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH, KICK], t0);
        gate.disconnected(TWITCH, false, t0);
        gate.disconnected(KICK, false, t0 + SECOND);
        gate.discard_due(t0 + CONNECTION_LOSS_GRACE);
        assert_eq!(gate.deadline(), Some(t0 + SECOND + CONNECTION_LOSS_GRACE));
    }

    #[test]
    fn discarded_loss_leaves_the_link_armed_for_the_next_loss() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH], t0);
        gate.disconnected(TWITCH, false, t0);
        let discarded = t0 + CONNECTION_LOSS_GRACE;
        gate.discard_due(discarded);
        gate.connected(TWITCH.to_owned(), discarded + SECOND);
        gate.disconnected(TWITCH, false, discarded + 2 * SECOND);
        assert_eq!(
            flush(&mut gate, discarded + MIN_CUE_SPACING),
            cue(&[TWITCH])
        );
    }

    #[test]
    fn discarded_loss_does_not_start_the_cue_spacing() {
        let t0 = Instant::now();
        let mut gate = connected_gate(&[TWITCH, KICK], t0);
        gate.disconnected(TWITCH, false, t0);
        let discarded = t0 + CONNECTION_LOSS_GRACE;
        gate.discard_due(discarded);
        gate.disconnected(KICK, false, discarded);
        assert_eq!(
            flush(&mut gate, discarded + CONNECTION_LOSS_GRACE),
            cue(&[KICK])
        );
    }
}
