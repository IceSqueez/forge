use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use forge_audio::{AudioRoute, AudioSink, FanOutSink};
use forge_overlay::kinds::audio::KIND_ID as AUDIO_OVERLAY_KIND;
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
    NotAudioOverlay,
    Ready(OverlayId),
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
        Ok(Some(overlay)) if overlay.kind_id == AUDIO_OVERLAY_KIND => {
            AudioDestination::Ready(id.clone())
        }
        Ok(Some(_)) => AudioDestination::NotAudioOverlay,
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
    DestinationNotAudioOverlay,
}

impl fmt::Display for RouteFallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoDestinationChosen => "no audio overlay is chosen",
            Self::ServerUnavailable => "the server that carries overlay audio is not running",
            Self::DestinationUnreadable => "the chosen audio overlay could not be looked up",
            Self::DestinationNotFound => "the chosen audio overlay no longer exists",
            Self::DestinationNotAudioOverlay => "the chosen overlay is not an audio overlay",
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
        AudioDestination::NotAudioOverlay => fall_back(RouteFallback::DestinationNotAudioOverlay),
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
