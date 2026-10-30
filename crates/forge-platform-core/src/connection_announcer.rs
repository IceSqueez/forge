use std::sync::{Arc, Mutex, RwLock};

use forge_events::EventPublisher;

use crate::chat::{ConnectionState, ConnectionTransition, connection_state_changed_event};

pub struct ConnectionAnnouncer {
    platform_id: String,
    publisher: RwLock<Option<Arc<dyn EventPublisher>>>,
    announced: Mutex<ConnectionState>,
}

impl ConnectionAnnouncer {
    pub fn new(platform_id: impl Into<String>) -> Self {
        Self {
            platform_id: platform_id.into(),
            publisher: RwLock::new(None),
            announced: Mutex::new(ConnectionState::Disconnected),
        }
    }

    pub fn attach(&self, publisher: Arc<dyn EventPublisher>) {
        *self
            .publisher
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(publisher);
    }

    pub fn announce(&self, state: ConnectionState, transition: ConnectionTransition) {
        let changed = {
            let mut announced = self
                .announced
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *announced == state {
                false
            } else {
                *announced = state;
                true
            }
        };
        let publisher = self
            .publisher
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if changed && let Some(publisher) = publisher {
            publisher.publish(connection_state_changed_event(
                &self.platform_id,
                state,
                transition,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_events::Event;

    use super::*;

    const PLATFORM: &str = "donatello";

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);

    impl EventPublisher for Recorder {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    impl Recorder {
        fn announcements(&self) -> Vec<(String, ConnectionState, bool)> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|event| {
                    (
                        event.payload["platform_id"].as_str().unwrap().to_owned(),
                        serde_json::from_value(event.payload["state"].clone()).unwrap(),
                        event.payload["expected"].as_bool().unwrap(),
                    )
                })
                .collect()
        }
    }

    fn attached() -> (ConnectionAnnouncer, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        let announcer = ConnectionAnnouncer::new(PLATFORM);
        announcer.attach(Arc::clone(&recorder) as Arc<dyn EventPublisher>);
        (announcer, recorder)
    }

    #[test]
    fn state_change_publishes_the_platform_state_and_transition() {
        let (announcer, recorder) = attached();
        announcer.announce(ConnectionState::Connected, ConnectionTransition::Expected);
        announcer.announce(
            ConnectionState::Reconnecting,
            ConnectionTransition::Unexpected,
        );
        assert_eq!(
            recorder.announcements(),
            [
                (PLATFORM.to_owned(), ConnectionState::Connected, true),
                (PLATFORM.to_owned(), ConnectionState::Reconnecting, false),
            ]
        );
    }

    #[test]
    fn repeating_the_announced_state_publishes_nothing() {
        let (announcer, recorder) = attached();
        announcer.announce(ConnectionState::Connected, ConnectionTransition::Expected);
        announcer.announce(ConnectionState::Connected, ConnectionTransition::Unexpected);
        assert_eq!(recorder.announcements().len(), 1);
    }

    #[test]
    fn announcing_disconnected_from_the_initial_state_publishes_nothing() {
        let (announcer, recorder) = attached();
        announcer.announce(
            ConnectionState::Disconnected,
            ConnectionTransition::Unexpected,
        );
        assert!(recorder.announcements().is_empty());
    }

    #[test]
    fn state_announced_before_attach_is_tracked_without_publishing() {
        let recorder = Arc::new(Recorder::default());
        let announcer = ConnectionAnnouncer::new(PLATFORM);
        announcer.announce(ConnectionState::Connected, ConnectionTransition::Expected);
        announcer.attach(Arc::clone(&recorder) as Arc<dyn EventPublisher>);
        announcer.announce(ConnectionState::Connected, ConnectionTransition::Expected);
        announcer.announce(
            ConnectionState::Disconnected,
            ConnectionTransition::Expected,
        );
        assert_eq!(
            recorder.announcements(),
            [(PLATFORM.to_owned(), ConnectionState::Disconnected, true)]
        );
    }

    #[test]
    fn a_later_attach_replaces_the_publisher_for_every_following_announcement() {
        let (announcer, retired) = attached();
        announcer.announce(ConnectionState::Connected, ConnectionTransition::Expected);
        let current = Arc::new(Recorder::default());

        announcer.attach(Arc::clone(&current) as Arc<dyn EventPublisher>);
        announcer.announce(
            ConnectionState::Reconnecting,
            ConnectionTransition::Unexpected,
        );

        assert_eq!(retired.announcements().len(), 1);
        assert_eq!(
            current.announcements(),
            [(PLATFORM.to_owned(), ConnectionState::Reconnecting, false)]
        );
    }
}
