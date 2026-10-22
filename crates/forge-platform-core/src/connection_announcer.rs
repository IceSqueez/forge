use std::sync::{Arc, Mutex, OnceLock};

use forge_events::EventPublisher;

use crate::chat::{ConnectionState, ConnectionTransition, connection_state_changed_event};

pub struct ConnectionAnnouncer {
    platform_id: String,
    publisher: OnceLock<Arc<dyn EventPublisher>>,
    announced: Mutex<ConnectionState>,
}

impl ConnectionAnnouncer {
    pub fn new(platform_id: impl Into<String>) -> Self {
        Self {
            platform_id: platform_id.into(),
            publisher: OnceLock::new(),
            announced: Mutex::new(ConnectionState::Disconnected),
        }
    }

    pub fn attach(&self, publisher: Arc<dyn EventPublisher>) {
        let _ = self.publisher.set(publisher);
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
        if changed && let Some(publisher) = self.publisher.get() {
            publisher.publish(connection_state_changed_event(
                &self.platform_id,
                state,
                transition,
            ));
        }
    }
}
