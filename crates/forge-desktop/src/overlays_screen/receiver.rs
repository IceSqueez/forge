use std::sync::Arc;

use forge_audio::AudioRoute;
use forge_components::tr;
use forge_storage::{OverlayId, SettingsRepo, StorageError};
use gpui::Context;

use super::OverlaysView;
use crate::async_bridge;
use crate::audio_routes::{AudioDomain, load_audio_routes, set_destination, set_route};

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

    pub(super) fn apply_receiver(
        &mut self,
        destination: Option<OverlayId>,
        cx: &mut Context<Self>,
    ) {
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
                route_audio_to(settings.as_ref(), next.as_ref())
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

/// A receiver only hears audio whose route plays an overlay, so choosing one moves every
/// local-only domain onto the overlay; clearing it leaves the routes as they are.
pub(super) async fn route_audio_to(
    settings: &dyn SettingsRepo,
    destination: Option<&OverlayId>,
) -> Result<(), StorageError> {
    if destination.is_some() {
        let routes = load_audio_routes(settings).await;
        for (domain, route) in [
            (AudioDomain::Speech, routes.speech),
            (AudioDomain::Clips, routes.clips),
        ] {
            let claimed = route_for_receiver(route);
            if claimed != route {
                set_route(settings, domain, claimed).await?;
            }
        }
    }
    set_destination(settings, destination).await
}

pub(super) fn route_for_receiver(route: AudioRoute) -> AudioRoute {
    if route.plays_overlay() {
        route
    } else {
        AudioRoute::Overlay
    }
}

/// Clears the stored receiver when it is `id`, before the overlay goes away: overlay ids are
/// minted from the name, so a stale id would hand the audio to a re-created namesake.
pub(super) async fn release_receiver(
    settings: &dyn SettingsRepo,
    id: &OverlayId,
) -> Result<bool, StorageError> {
    if load_audio_routes(settings).await.destination.as_ref() != Some(id) {
        return Ok(false);
    }
    set_destination(settings, None).await?;
    Ok(true)
}

pub(super) async fn restore_receiver(settings: &dyn SettingsRepo, id: &OverlayId) {
    if let Err(error) = set_destination(settings, Some(id)).await {
        tracing::warn!(overlay = %id, %error, "audio receiver not restored after a failed delete");
    }
}
