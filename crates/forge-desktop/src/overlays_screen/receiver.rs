use std::sync::Arc;

use forge_audio::AudioRoute;
use forge_components::tr;
use forge_storage::{OverlayId, SettingsRepo, StorageError};
use gpui::Context;

use super::OverlaysView;
use crate::async_bridge;
use crate::audio_routes::{AudioDomain, load_audio_routes, set_destination, set_route};
use crate::repo_failure::RepoFailure;

impl OverlaysView {
    pub(super) fn load_receiver(&mut self, cx: &mut Context<Self>) {
        let settings = Arc::clone(&self.handles.settings_repo);
        async_bridge::run_async(
            &self.handles.rt_handle,
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

        let settings = Arc::clone(&self.handles.settings_repo);
        let router = Arc::clone(&self.handles.audio_router);
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                route_audio_to(settings.as_ref(), next.as_ref())
                    .await
                    .map_err(RepoFailure::from)?;
                router.apply().await;
                Ok(())
            },
            move |this, result: Result<(), RepoFailure>, cx| {
                if let Err(message) = result {
                    this.apply_receiver(previous, cx);
                    this.report(
                        &tr!("overlays_receiver_save_failed", reason = message.text()),
                        cx,
                    );
                }
            },
            cx,
        );
    }
}

pub(super) async fn route_audio_to(
    settings: &dyn SettingsRepo,
    destination: Option<&OverlayId>,
) -> Result<(), StorageError> {
    let mut replaced = Vec::new();
    let written = claim_and_set_destination(settings, destination, &mut replaced).await;
    if written.is_err() {
        for (domain, route) in replaced {
            if let Err(error) = set_route(settings, domain, route).await {
                tracing::warn!(%error, "audio route not restored after a failed receiver change");
            }
        }
    }
    written
}

async fn claim_and_set_destination(
    settings: &dyn SettingsRepo,
    destination: Option<&OverlayId>,
    replaced: &mut Vec<(AudioDomain, AudioRoute)>,
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
                replaced.push((domain, route));
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

#[cfg(test)]
mod tests {
    use forge_storage::settings::core_settings::AUDIO_OVERLAY_ID;

    use super::*;
    use crate::audio_routes::{CLIPS_ROUTE, SPEECH_ROUTE};
    use crate::test_support::test_backend;

    const RECEIVER: &str = "stage-audio";

    fn receiver() -> OverlayId {
        OverlayId::new(RECEIVER)
    }

    #[tokio::test]
    async fn choosing_a_receiver_moves_local_routes_onto_it_and_leaves_overlay_ones() {
        let (backend, _writes) = test_backend();
        backend.seed_raw(&SPEECH_ROUTE, AudioRoute::Both.as_str());

        route_audio_to(backend.as_ref(), Some(&receiver()))
            .await
            .expect("every write lands");

        let stored = load_audio_routes(backend.as_ref()).await;
        assert_eq!(
            (stored.speech, stored.clips, stored.destination),
            (AudioRoute::Both, AudioRoute::Overlay, Some(receiver()))
        );
    }

    #[tokio::test]
    async fn a_refused_destination_write_puts_both_claimed_routes_back() {
        let (backend, _writes) = test_backend();
        backend.seed_raw(&SPEECH_ROUTE, AudioRoute::Local.as_str());
        backend.seed_raw(&CLIPS_ROUTE, AudioRoute::Local.as_str());
        backend.fail_writes(AUDIO_OVERLAY_ID.name());

        let outcome = route_audio_to(backend.as_ref(), Some(&receiver())).await;

        let stored = load_audio_routes(backend.as_ref()).await;
        assert_eq!(
            (outcome.is_err(), stored.speech, stored.clips),
            (true, AudioRoute::Local, AudioRoute::Local)
        );
    }

    #[tokio::test]
    async fn a_refused_second_route_puts_the_first_back_and_sets_no_destination() {
        let (backend, _writes) = test_backend();
        backend.seed_raw(&SPEECH_ROUTE, AudioRoute::Local.as_str());
        backend.fail_writes(CLIPS_ROUTE.name());

        let outcome = route_audio_to(backend.as_ref(), Some(&receiver())).await;

        let stored = load_audio_routes(backend.as_ref()).await;
        assert_eq!(
            (outcome.is_err(), stored.speech, stored.destination),
            (true, AudioRoute::Local, None)
        );
    }

    #[tokio::test]
    async fn clearing_the_receiver_leaves_the_routes_as_they_are() {
        let (backend, _writes) = test_backend();
        backend.seed_raw(&SPEECH_ROUTE, AudioRoute::Overlay.as_str());
        backend.seed_raw(&AUDIO_OVERLAY_ID, RECEIVER);

        route_audio_to(backend.as_ref(), None)
            .await
            .expect("every write lands");

        let stored = load_audio_routes(backend.as_ref()).await;
        assert_eq!(
            (stored.speech, stored.destination),
            (AudioRoute::Overlay, None)
        );
    }
}
