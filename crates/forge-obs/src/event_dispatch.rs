use std::sync::RwLock;

use forge_events::EventPublisher;
use forge_platform_core::HealthDelta;
use forge_types::RecoverRwLock;
use tokio::sync::broadcast;

use crate::health::HealthSnapshot;
use crate::mirror::{AppliedEvent, ObsMirror};

pub(crate) fn changes_option_lists(ev: &obws::events::Event) -> bool {
    use obws::events::Event;
    matches!(
        ev,
        Event::SceneCreated { .. }
            | Event::SceneRemoved { .. }
            | Event::SceneNameChanged { .. }
            | Event::SceneListChanged { .. }
            | Event::CurrentProgramSceneChanged { .. }
            | Event::InputCreated { .. }
            | Event::InputRemoved { .. }
            | Event::InputNameChanged { .. }
            | Event::SceneItemCreated { .. }
            | Event::SceneItemRemoved { .. }
            | Event::SceneItemEnableStateChanged { .. }
            | Event::CurrentSceneTransitionChanged { .. }
            | Event::CurrentProfileChanged { .. }
            | Event::ProfileListChanged { .. }
            | Event::CurrentSceneCollectionChanged { .. }
            | Event::SceneCollectionListChanged { .. }
    )
}

pub(crate) fn changes_content_rows_only(ev: &obws::events::Event) -> bool {
    use obws::events::Event;
    matches!(
        ev,
        Event::CurrentPreviewSceneChanged { .. } | Event::SceneItemLockStateChanged { .. }
    )
}

pub(crate) fn handle_obs_event(
    ev: &obws::events::Event,
    mirror: &ObsMirror,
    health_state: &RwLock<HealthSnapshot>,
    health_tx: &broadcast::Sender<HealthDelta>,
    publisher: &dyn EventPublisher,
) {
    let AppliedEvent {
        previous_scene: from_scene,
        visibility,
        lock,
    } = mirror.apply_event(ev);

    let deltas = crate::events::apply_health_update(ev, &mut health_state.write_recover());
    for delta in deltas {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "having no subscribers is a normal state"
        )]
        let _ = health_tx.send(delta);
    }

    if let Some(bus_event) = crate::events::map_obs_event(ev, from_scene.as_deref()) {
        publisher.publish(bus_event);
    }
    for event in visibility.into_iter().chain(lock) {
        publisher.publish(event);
    }
}
