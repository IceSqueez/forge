use forge_events::EventSource;

pub struct EventFilter {
    pub source: Option<EventSource>,
    pub kind_prefix: Option<String>,
}
