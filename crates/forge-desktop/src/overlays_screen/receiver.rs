use std::sync::Arc;

use forge_components::tr;
use forge_storage::OverlayId;
use gpui::Context;

use super::OverlaysView;
use crate::async_bridge;
use crate::audio_routes::{load_audio_routes, set_destination};

impl OverlaysView {
    /// Read once when the screen mounts; afterwards this screen is the only writer it can observe.
    pub(super) fn load_receiver(&mut self, cx: &mut Context<Self>) {
        let settings = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { load_audio_routes(settings.as_ref()).await.destination },
            |this, destination, cx| this.apply_receiver(destination, cx),
            cx,
        );
    }

    fn apply_receiver(&mut self, destination: Option<OverlayId>, cx: &mut Context<Self>) {
        self.receiver = destination;
        if let Some(panel) = self.panel_view() {
            let on = {
                let id = panel.read(cx).overlay_id();
                self.receiver.as_ref() == Some(id)
            };
            panel.update(cx, |panel, cx| panel.set_receiver(on, cx));
        }
        cx.notify();
    }

    /// Only one overlay receives forge audio, so turning it on here moves it off whichever overlay
    /// held it; turning it off here leaves no receiver.
    pub(super) fn set_receiver(&mut self, id: OverlayId, on: bool, cx: &mut Context<Self>) {
        let next = if on {
            Some(id)
        } else if self.receiver.as_ref() == Some(&id) {
            None
        } else {
            return;
        };
        let previous = self.receiver.clone();
        self.apply_receiver(next.clone(), cx);

        let settings = Arc::clone(&self.settings_repo);
        let router = Arc::clone(&self.audio_router);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                set_destination(settings.as_ref(), next.as_ref())
                    .await
                    .map_err(|e| e.to_string())?;
                router.apply().await;
                Ok(())
            },
            move |this, result: Result<(), String>, cx| {
                if let Err(message) = result {
                    this.apply_receiver(previous, cx);
                    this.report(
                        &tr!("overlays_receiver_save_failed", reason = message.as_str()),
                        cx,
                    );
                }
            },
            cx,
        );
    }
}
