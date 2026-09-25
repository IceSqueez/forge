use forge_overlay::{BEHAVIOR_FILE, DeliveryDisposition};
use forge_storage::OverlayDefinition;
use gpui::Context;

use super::OverlaysView;
use crate::async_bridge;

/// No shipped look arms a timer of its own - each leaves hiding to the page runtime - so a behavior
/// file that does has taken over when its show hides.
const OWN_TIMERS: [&str; 2] = ["setTimeout", "setInterval"];

pub(super) fn times_own_hide(behavior: &str) -> bool {
    OWN_TIMERS.iter().any(|timer| behavior.contains(timer))
}

impl OverlaysView {
    /// Read when the selection or the record moves, never on a timer; only a user-owned behavior
    /// file of a look that queues its shows can hold one.
    pub(super) fn probe_hide_timing(
        &mut self,
        definition: &OverlayDefinition,
        cx: &mut Context<Self>,
    ) {
        let ticket = self.hide_probe_gen.next();
        let queues = self
            .kinds
            .get(&definition.kind_id)
            .is_some_and(|descriptor| {
                descriptor.delivery_disposition() == DeliveryDisposition::Transient
            });
        let owned = definition
            .source_overrides
            .iter()
            .any(|file| file == BEHAVIOR_FILE);
        if !queues || !owned {
            self.apply_hide_timing(ticket, false, cx);
            return;
        }

        let service = self.service.clone();
        let id = definition.id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                match service.read_source(&id, BEHAVIOR_FILE).await {
                    Ok(Some(behavior)) => times_own_hide(&behavior),
                    Ok(None) => false,
                    Err(error) => {
                        tracing::warn!(overlay = %id, %error, "overlay behavior file unreadable for the hide-timing check");
                        false
                    }
                }
            },
            move |this, flagged, cx| this.apply_hide_timing(ticket, flagged, cx),
            cx,
        );
    }

    fn apply_hide_timing(&mut self, ticket: u64, flagged: bool, cx: &mut Context<Self>) {
        if !self.hide_probe_gen.is_current(ticket) {
            return;
        }
        if let Some(panel) = self.panel_view() {
            panel.update(cx, |panel, cx| panel.set_hides_itself(flagged, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_behavior_that_arms_a_timer_of_its_own_is_flagged_as_hiding_itself() {
        for (behavior, flagged) in [
            ("setTimeout(() => stage.hidden = true, 4000);", true),
            ("const tick = setInterval(fade, 50);", true),
            ("forge.onShow(show => render(show));", false),
            ("", false),
        ] {
            assert_eq!(times_own_hide(behavior), flagged, "{behavior:?}");
        }
    }
}
