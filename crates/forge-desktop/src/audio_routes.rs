use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use forge_audio::{AudioRoute, AudioSink, FanOutSink};
use forge_soundboard::ClipRoute;
use forge_storage::{OverlayId, OverlayRepo, SettingsRepo, StorageError, reserved_keys};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDomain {
    Speech,
    Clips,
}

impl AudioDomain {
    fn key(self) -> &'static str {
        match self {
            Self::Speech => reserved_keys::AUDIO_SPEECH_ROUTE,
            Self::Clips => reserved_keys::AUDIO_CLIPS_ROUTE,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Speech => "speech",
            Self::Clips => "clips",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioRoutes {
    pub speech: AudioRoute,
    pub clips: AudioRoute,
    pub destination: Option<OverlayId>,
}

/// An unreadable or unknown stored value yields the local route; a routing setting never blocks boot.
pub async fn load_audio_routes(repo: &dyn SettingsRepo) -> AudioRoutes {
    AudioRoutes {
        speech: stored_route(repo, AudioDomain::Speech).await,
        clips: stored_route(repo, AudioDomain::Clips).await,
        destination: stored_destination(repo).await,
    }
}

pub async fn set_route(
    repo: &dyn SettingsRepo,
    domain: AudioDomain,
    route: AudioRoute,
) -> Result<(), StorageError> {
    repo.set_string(domain.key(), route.as_str()).await
}

/// `None` removes the key, which is how "no audio overlay chosen" is stored.
pub async fn set_destination(
    repo: &dyn SettingsRepo,
    destination: Option<&OverlayId>,
) -> Result<(), StorageError> {
    match destination.map(OverlayId::as_str).map(str::trim) {
        Some(id) if !id.is_empty() => repo.set_string(reserved_keys::AUDIO_OVERLAY_ID, id).await,
        _ => repo
            .delete(reserved_keys::AUDIO_OVERLAY_ID)
            .await
            .map(|_| ()),
    }
}

async fn stored_route(repo: &dyn SettingsRepo, domain: AudioDomain) -> AudioRoute {
    let key = domain.key();
    let raw = match repo.get_string(key).await {
        Ok(Some(raw)) => raw,
        Ok(None) => return AudioRoute::default(),
        Err(e) => {
            tracing::warn!(key, error = %e, "could not read the stored audio route");
            return AudioRoute::default();
        }
    };
    AudioRoute::from_str(&raw).unwrap_or_else(|e| {
        tracing::warn!(key, error = %e, "ignoring an unknown stored audio route");
        AudioRoute::default()
    })
}

async fn stored_destination(repo: &dyn SettingsRepo) -> Option<OverlayId> {
    match repo.get_string(reserved_keys::AUDIO_OVERLAY_ID).await {
        Ok(raw) => raw
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(OverlayId::new),
        Err(e) => {
            tracing::warn!(error = %e, "could not read the chosen audio overlay identity");
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDestination {
    Unset,
    ServerOff,
    Unreadable,
    NotFound,
    Ready(OverlayId),
}

impl AudioDestination {
    pub fn ready(&self) -> Option<&OverlayId> {
        match self {
            Self::Ready(id) => Some(id),
            _ => None,
        }
    }
}

pub async fn resolve_destination(
    repo: &dyn OverlayRepo,
    server_available: bool,
    destination: Option<&OverlayId>,
) -> AudioDestination {
    let Some(id) = destination else {
        return AudioDestination::Unset;
    };
    if !server_available {
        return AudioDestination::ServerOff;
    }
    match repo.get(id).await {
        Ok(Some(_)) => AudioDestination::Ready(id.clone()),
        Ok(None) => AudioDestination::NotFound,
        Err(e) => {
            tracing::warn!(error = %e, "could not look up the chosen audio overlay");
            AudioDestination::Unreadable
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteFallback {
    NoDestinationChosen,
    ServerUnavailable,
    DestinationUnreadable,
    DestinationNotFound,
}

impl fmt::Display for RouteFallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoDestinationChosen => "no audio overlay is chosen",
            Self::ServerUnavailable => "the server that carries overlay audio is not running",
            Self::DestinationUnreadable => "the chosen audio overlay could not be looked up",
            Self::DestinationNotFound => "the chosen audio overlay no longer exists",
        })
    }
}

/// `route.plays_overlay()` holds exactly when `destination` is `Some`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlan {
    pub route: AudioRoute,
    pub destination: Option<OverlayId>,
    pub fallback: Option<RouteFallback>,
}

pub fn plan_route(requested: AudioRoute, destination: &AudioDestination) -> RoutePlan {
    if !requested.plays_overlay() {
        return RoutePlan {
            route: requested,
            destination: None,
            fallback: None,
        };
    }
    match destination {
        AudioDestination::Ready(id) => RoutePlan {
            route: requested,
            destination: Some(id.clone()),
            fallback: None,
        },
        AudioDestination::Unset => fall_back(RouteFallback::NoDestinationChosen),
        AudioDestination::ServerOff => fall_back(RouteFallback::ServerUnavailable),
        AudioDestination::Unreadable => fall_back(RouteFallback::DestinationUnreadable),
        AudioDestination::NotFound => fall_back(RouteFallback::DestinationNotFound),
    }
}

fn fall_back(fallback: RouteFallback) -> RoutePlan {
    RoutePlan {
        route: AudioRoute::Local,
        destination: None,
        fallback: Some(fallback),
    }
}

pub fn compose_sink(
    plan: &RoutePlan,
    local: Arc<dyn AudioSink>,
    overlay: Option<Arc<dyn AudioSink>>,
) -> Arc<dyn AudioSink> {
    match overlay.filter(|_| plan.destination.is_some()) {
        Some(overlay) if plan.route.plays_local() => {
            Arc::new(FanOutSink::new(vec![local, overlay]))
        }
        Some(overlay) => overlay,
        None => local,
    }
}

pub struct RoutesInstall {
    pub speech_sink: Arc<dyn AudioSink>,
    pub clip_route: ClipRoute,
}

/// `overlay_sink` runs at most once, and only when a resolved destination is ready and either
/// plan plays it; its `None` folds back to a local-only composition exactly like a missing sink.
pub fn compose_routes(
    speech_plan: &RoutePlan,
    clips_plan: &RoutePlan,
    destination: &AudioDestination,
    speech_local: Arc<dyn AudioSink>,
    overlay_sink: impl FnOnce(&OverlayId) -> Option<Arc<dyn AudioSink>>,
) -> RoutesInstall {
    let overlay_sink = destination
        .ready()
        .filter(|_| speech_plan.route.plays_overlay() || clips_plan.route.plays_overlay())
        .and_then(overlay_sink);

    RoutesInstall {
        speech_sink: compose_sink(speech_plan, speech_local, overlay_sink.clone()),
        clip_route: ClipRoute::new(clips_plan.route, overlay_sink),
    }
}

pub fn report_plan(domain: AudioDomain, plan: &RoutePlan) {
    match plan.fallback {
        Some(fallback) => tracing::warn!(
            domain = domain.as_str(),
            reason = %fallback,
            "audio route falls back to local playback until the next restart"
        ),
        None => tracing::info!(
            domain = domain.as_str(),
            route = plan.route.as_str(),
            "audio route applied"
        ),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::cell::Cell;

    use forge_overlay::kinds::alert::KIND_ID as ALERT_OVERLAY_KIND;
    use forge_overlay::kinds::blank::KIND_ID as BLANK_OVERLAY_KIND;
    use forge_storage::MockOverlayRepo;
    use forge_storage::settings::MockSettingsRepo;

    use super::*;
    use crate::test_support::{RecordingSink, overlay_named, test_backend, tone};

    const CHOSEN: &str = "stage-audio";

    #[derive(Clone, Copy, Debug)]
    enum Lookup {
        ABlankOverlay,
        AnotherLook,
        Nothing,
        AFailure,
    }

    fn overlays(lookup: Lookup) -> Arc<dyn OverlayRepo> {
        let mut repo = MockOverlayRepo::new();
        repo.expect_get().returning(move |id| match lookup {
            Lookup::ABlankOverlay => Ok(Some(overlay_named(id.as_str(), BLANK_OVERLAY_KIND))),
            Lookup::AnotherLook => Ok(Some(overlay_named(id.as_str(), ALERT_OVERLAY_KIND))),
            Lookup::Nothing => Ok(None),
            Lookup::AFailure => Err(StorageError::Connection {
                reason: "the database went away".to_owned(),
            }),
        });
        Arc::new(repo)
    }

    fn unreadable_settings() -> MockSettingsRepo {
        let mut repo = MockSettingsRepo::new();
        repo.expect_get_string().returning(|key| {
            Err(StorageError::NotFound {
                key: key.to_owned(),
            })
        });
        repo
    }

    #[tokio::test]
    async fn absent_settings_leave_both_domains_on_the_local_device() {
        let (backend, _writes) = test_backend();

        assert_eq!(
            load_audio_routes(backend.as_ref()).await,
            AudioRoutes {
                speech: AudioRoute::Local,
                clips: AudioRoute::Local,
                destination: None,
            }
        );
    }

    #[tokio::test]
    async fn each_domain_is_read_back_from_its_own_key() {
        let (backend, _writes) = test_backend();
        set_route(backend.as_ref(), AudioDomain::Speech, AudioRoute::Overlay)
            .await
            .expect("the route is stored");
        set_route(backend.as_ref(), AudioDomain::Clips, AudioRoute::Both)
            .await
            .expect("the route is stored");

        let routes = load_audio_routes(backend.as_ref()).await;

        assert_eq!(routes.speech, AudioRoute::Overlay);
        assert_eq!(routes.clips, AudioRoute::Both);
    }

    #[tokio::test]
    async fn every_route_survives_a_write_and_a_read() {
        for domain in [AudioDomain::Speech, AudioDomain::Clips] {
            for route in [AudioRoute::Local, AudioRoute::Overlay, AudioRoute::Both] {
                let (backend, _writes) = test_backend();
                set_route(backend.as_ref(), domain, route)
                    .await
                    .expect("the route is stored");

                let stored = load_audio_routes(backend.as_ref()).await;
                let read_back = match domain {
                    AudioDomain::Speech => stored.speech,
                    AudioDomain::Clips => stored.clips,
                };

                assert_eq!(read_back, route, "{domain:?} lost {route:?} in storage");
            }
        }
    }

    #[tokio::test]
    async fn a_stored_route_that_cannot_be_parsed_falls_back_to_the_local_device() {
        for stored in ["", "   ", "remote", "obs", "local overlay"] {
            let (backend, _writes) = test_backend();
            backend
                .set_string(reserved_keys::AUDIO_SPEECH_ROUTE, stored)
                .await
                .expect("the raw value is stored");

            assert_eq!(
                load_audio_routes(backend.as_ref()).await.speech,
                AudioRoute::Local,
                "{stored:?} was not treated as unusable"
            );
        }
    }

    #[tokio::test]
    async fn settings_that_cannot_be_read_still_yield_a_playable_route() {
        assert_eq!(
            load_audio_routes(&unreadable_settings()).await,
            AudioRoutes::default()
        );
    }

    #[tokio::test]
    async fn a_stored_destination_that_names_nothing_is_read_as_no_destination() {
        for blank in ["", "   ", "\t\n"] {
            let (backend, _writes) = test_backend();
            backend
                .set_string(reserved_keys::AUDIO_OVERLAY_ID, blank)
                .await
                .expect("the raw value is stored");

            assert_eq!(
                load_audio_routes(backend.as_ref()).await.destination,
                None,
                "{blank:?} was read as a destination"
            );
        }
    }

    #[tokio::test]
    async fn a_destination_that_names_nothing_removes_the_key_rather_than_blanking_it() {
        for cleared in [None, Some(""), Some("   ")] {
            let (backend, _writes) = test_backend();
            set_destination(backend.as_ref(), Some(&OverlayId::new(CHOSEN)))
                .await
                .expect("the destination is written");

            set_destination(backend.as_ref(), cleared.map(OverlayId::new).as_ref())
                .await
                .expect("the destination is cleared");

            assert_eq!(
                backend
                    .get_string(reserved_keys::AUDIO_OVERLAY_ID)
                    .await
                    .expect("the key is readable"),
                None,
                "{cleared:?} was stored instead of clearing the key"
            );
        }
    }

    #[tokio::test]
    async fn choosing_a_second_receiver_moves_the_audio_off_the_first() {
        let (backend, _writes) = test_backend();
        for receiver in ["sub-alert", CHOSEN] {
            set_destination(backend.as_ref(), Some(&OverlayId::new(receiver)))
                .await
                .expect("the receiver is chosen");
        }

        assert_eq!(
            load_audio_routes(backend.as_ref()).await.destination,
            Some(OverlayId::new(CHOSEN)),
            "two overlays both believe they receive the audio, so every clip plays twice"
        );
    }

    #[tokio::test]
    async fn a_padded_destination_is_stored_and_read_without_its_padding() {
        let (backend, _writes) = test_backend();
        set_destination(backend.as_ref(), Some(&OverlayId::new("  stage-audio  ")))
            .await
            .expect("the destination is written");

        assert_eq!(
            load_audio_routes(backend.as_ref()).await.destination,
            Some(OverlayId::new(CHOSEN))
        );
    }

    #[tokio::test]
    async fn every_lookup_state_has_its_own_destination_verdict() {
        let chosen = OverlayId::new(CHOSEN);
        for (server_available, destination, lookup, expected) in [
            (true, None, Lookup::ABlankOverlay, AudioDestination::Unset),
            (false, None, Lookup::ABlankOverlay, AudioDestination::Unset),
            (
                false,
                Some(&chosen),
                Lookup::ABlankOverlay,
                AudioDestination::ServerOff,
            ),
            (
                true,
                Some(&chosen),
                Lookup::AFailure,
                AudioDestination::Unreadable,
            ),
            (
                true,
                Some(&chosen),
                Lookup::Nothing,
                AudioDestination::NotFound,
            ),
            (
                true,
                Some(&chosen),
                Lookup::AnotherLook,
                AudioDestination::Ready(OverlayId::new(CHOSEN)),
            ),
            (
                true,
                Some(&chosen),
                Lookup::ABlankOverlay,
                AudioDestination::Ready(OverlayId::new(CHOSEN)),
            ),
        ] {
            let repo = overlays(lookup);

            assert_eq!(
                resolve_destination(repo.as_ref(), server_available, destination).await,
                expected,
                "server_available={server_available} destination={destination:?} lookup={lookup:?}"
            );
        }
    }

    #[test]
    fn only_a_ready_destination_lets_an_overlay_route_stand() {
        let ready = OverlayId::new(CHOSEN);
        let unusable = [
            (AudioDestination::Unset, RouteFallback::NoDestinationChosen),
            (
                AudioDestination::ServerOff,
                RouteFallback::ServerUnavailable,
            ),
            (
                AudioDestination::Unreadable,
                RouteFallback::DestinationUnreadable,
            ),
            (
                AudioDestination::NotFound,
                RouteFallback::DestinationNotFound,
            ),
        ];

        for requested in [AudioRoute::Overlay, AudioRoute::Both] {
            assert_eq!(
                plan_route(requested, &AudioDestination::Ready(ready.clone())),
                RoutePlan {
                    route: requested,
                    destination: Some(ready.clone()),
                    fallback: None,
                },
                "{requested:?} lost its ready destination"
            );

            for (state, fallback) in &unusable {
                assert_eq!(
                    plan_route(requested, state),
                    RoutePlan {
                        route: AudioRoute::Local,
                        destination: None,
                        fallback: Some(*fallback),
                    },
                    "{requested:?} against {state:?}"
                );
            }
        }
    }

    #[test]
    fn a_local_route_never_picks_up_a_destination() {
        for state in [
            AudioDestination::Ready(OverlayId::new(CHOSEN)),
            AudioDestination::Unset,
            AudioDestination::ServerOff,
            AudioDestination::Unreadable,
            AudioDestination::NotFound,
        ] {
            assert_eq!(
                plan_route(AudioRoute::Local, &state),
                RoutePlan {
                    route: AudioRoute::Local,
                    destination: None,
                    fallback: None,
                },
                "a local route reacted to {state:?}"
            );
        }
    }

    fn plan_with(route: AudioRoute, destination: Option<&str>) -> RoutePlan {
        RoutePlan {
            route,
            destination: destination.map(OverlayId::new),
            fallback: None,
        }
    }

    #[tokio::test]
    async fn playback_reaches_exactly_the_legs_the_plan_names() {
        for (route, destination, offered_overlay, local_plays, overlay_plays) in [
            (AudioRoute::Local, None, true, 1, 0),
            (AudioRoute::Overlay, Some(CHOSEN), true, 0, 1),
            (AudioRoute::Both, Some(CHOSEN), true, 1, 1),
            (AudioRoute::Overlay, Some(CHOSEN), false, 1, 0),
            (AudioRoute::Overlay, None, true, 1, 0),
            (AudioRoute::Both, None, true, 1, 0),
        ] {
            let local = RecordingSink::new();
            let overlay = RecordingSink::new();
            let sink = compose_sink(
                &plan_with(route, destination),
                Arc::clone(&local) as Arc<dyn AudioSink>,
                offered_overlay.then(|| Arc::clone(&overlay) as Arc<dyn AudioSink>),
            );

            sink.play(tone()).await.expect("the composed sink plays");

            let label = format!("{route:?}/{destination:?}/offered={offered_overlay}");
            assert_eq!(local.calls(), local_plays, "local leg for {label}");
            assert_eq!(overlay.calls(), overlay_plays, "overlay leg for {label}");
        }
    }

    #[test]
    fn a_single_leg_is_handed_back_unwrapped() {
        let local = Arc::new(RecordingSink::default()) as Arc<dyn AudioSink>;
        let overlay = Arc::new(RecordingSink::default()) as Arc<dyn AudioSink>;

        assert!(Arc::ptr_eq(
            &compose_sink(
                &plan_with(AudioRoute::Local, None),
                Arc::clone(&local),
                Some(Arc::clone(&overlay)),
            ),
            &local
        ));
        assert!(Arc::ptr_eq(
            &compose_sink(
                &plan_with(AudioRoute::Overlay, Some(CHOSEN)),
                Arc::clone(&local),
                Some(Arc::clone(&overlay)),
            ),
            &overlay
        ));
    }

    fn overlay_plan(route: AudioRoute) -> RoutePlan {
        plan_with(route, route.plays_overlay().then_some(CHOSEN))
    }

    #[test]
    fn the_overlay_sink_is_built_once_and_only_where_a_ready_destination_is_played() {
        let ready = AudioDestination::Ready(OverlayId::new(CHOSEN));
        for (speech, clips, destination, expected) in [
            (AudioRoute::Local, AudioRoute::Local, ready.clone(), 0),
            (AudioRoute::Overlay, AudioRoute::Local, ready.clone(), 1),
            (AudioRoute::Local, AudioRoute::Overlay, ready.clone(), 1),
            (AudioRoute::Overlay, AudioRoute::Overlay, ready.clone(), 1),
            (AudioRoute::Both, AudioRoute::Both, ready.clone(), 1),
            (
                AudioRoute::Overlay,
                AudioRoute::Overlay,
                AudioDestination::Unset,
                0,
            ),
            (
                AudioRoute::Overlay,
                AudioRoute::Overlay,
                AudioDestination::ServerOff,
                0,
            ),
            (
                AudioRoute::Both,
                AudioRoute::Both,
                AudioDestination::Unreadable,
                0,
            ),
            (
                AudioRoute::Both,
                AudioRoute::Both,
                AudioDestination::NotFound,
                0,
            ),
        ] {
            let built = Cell::new(0_usize);

            let _install = compose_routes(
                &overlay_plan(speech),
                &overlay_plan(clips),
                &destination,
                RecordingSink::new() as Arc<dyn AudioSink>,
                |_| {
                    built.set(built.get() + 1);
                    Some(RecordingSink::new() as Arc<dyn AudioSink>)
                },
            );

            assert_eq!(
                built.get(),
                expected,
                "speech={speech:?} clips={clips:?} against {destination:?}"
            );
        }
    }

    #[test]
    fn the_speech_side_takes_the_overlay_only_when_its_own_route_plays_it() {
        for (speech, clips, speech_takes_the_overlay) in [
            (AudioRoute::Local, AudioRoute::Overlay, false),
            (AudioRoute::Overlay, AudioRoute::Local, true),
            (AudioRoute::Overlay, AudioRoute::Overlay, true),
        ] {
            let local = Arc::new(RecordingSink::default()) as Arc<dyn AudioSink>;
            let overlay = Arc::new(RecordingSink::default()) as Arc<dyn AudioSink>;

            let install = compose_routes(
                &overlay_plan(speech),
                &overlay_plan(clips),
                &AudioDestination::Ready(OverlayId::new(CHOSEN)),
                Arc::clone(&local),
                |_| Some(Arc::clone(&overlay)),
            );

            let expected = if speech_takes_the_overlay {
                &overlay
            } else {
                &local
            };
            assert!(
                Arc::ptr_eq(&install.speech_sink, expected),
                "speech={speech:?} clips={clips:?} composed the wrong speech sink"
            );
        }
    }

    // Why: `ClipRoute` keeps its overlay leg private, so the reference count on the sink the
    // factory handed back is the only way here to see that the clip side was given that same
    // sink rather than nothing; which legs it then feeds is a soundboard-side contract.
    #[test]
    fn the_clip_route_is_handed_the_same_overlay_sink_as_the_speech_side() {
        let overlay = Arc::new(RecordingSink::default()) as Arc<dyn AudioSink>;

        let install = compose_routes(
            &overlay_plan(AudioRoute::Overlay),
            &overlay_plan(AudioRoute::Overlay),
            &AudioDestination::Ready(OverlayId::new(CHOSEN)),
            RecordingSink::new() as Arc<dyn AudioSink>,
            |_| Some(Arc::clone(&overlay)),
        );

        assert!(
            Arc::ptr_eq(&install.speech_sink, &overlay),
            "the speech side was not given the sink the factory built"
        );
        let held_by_both = Arc::strong_count(&overlay);
        drop(install.clip_route);
        assert_eq!(
            Arc::strong_count(&overlay),
            held_by_both - 1,
            "the clip route was not holding the overlay sink the speech side got"
        );
    }
}
