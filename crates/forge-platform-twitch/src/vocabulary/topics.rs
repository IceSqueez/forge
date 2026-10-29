use super::eventsub_topics::eventsub_topics;

macro_rules! topic_names {
    ($($name:ident => $topic:literal,)*) => {
        $(pub const $name: &str = $topic;)*
    };
}

eventsub_topics!(topic_names);
