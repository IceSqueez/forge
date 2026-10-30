use std::fmt;

pub const ACTION_OPTIONS_KEY: &str = "action.ids";
pub const TRIGGER_INSTANCE_OPTIONS_KEY: &str = "trigger_instance.ids";
pub const QUEUE_OPTIONS_KEY: &str = "queue.ids";
pub const SCRIPT_OPTIONS_KEY: &str = "script.names";
pub const GLOBAL_OPTIONS_KEY: &str = "global.names";
pub const SOUNDBOARD_CLIP_OPTIONS_KEY: &str = "soundboard.clip_ids";
pub const OVERLAY_OPTIONS_KEY: &str = "overlay.ids";
pub const MANAGED_MEDIA_NAMESPACE: &str = "media.blobs";
pub const LOCAL_FILE_NAMESPACE: &str = "local.files";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InternalNamespace {
    Action,
    TriggerInstance,
    Queue,
    Script,
    Global,
    SoundboardClip,
    Overlay,
    ManagedMedia,
}

impl InternalNamespace {
    pub const ALL: &'static [Self] = &[
        Self::Action,
        Self::TriggerInstance,
        Self::Queue,
        Self::Script,
        Self::Global,
        Self::SoundboardClip,
        Self::Overlay,
        Self::ManagedMedia,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Action => ACTION_OPTIONS_KEY,
            Self::TriggerInstance => TRIGGER_INSTANCE_OPTIONS_KEY,
            Self::Queue => QUEUE_OPTIONS_KEY,
            Self::Script => SCRIPT_OPTIONS_KEY,
            Self::Global => GLOBAL_OPTIONS_KEY,
            Self::SoundboardClip => SOUNDBOARD_CLIP_OPTIONS_KEY,
            Self::Overlay => OVERLAY_OPTIONS_KEY,
            Self::ManagedMedia => MANAGED_MEDIA_NAMESPACE,
        }
    }

    pub fn from_options_key(options_key: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|namespace| namespace.as_str() == options_key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExternalNamespace(String);

impl ExternalNamespace {
    pub fn new(options_key: impl Into<String>) -> Self {
        Self(options_key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceScope {
    Internal,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReferenceNamespace {
    Internal(InternalNamespace),
    External(ExternalNamespace),
}

impl ReferenceNamespace {
    pub fn for_options_key(options_key: &str) -> Self {
        InternalNamespace::from_options_key(options_key).map_or_else(
            || Self::External(ExternalNamespace::new(options_key)),
            Self::Internal,
        )
    }

    pub fn scope(&self) -> ReferenceScope {
        match self {
            Self::Internal(_) => ReferenceScope::Internal,
            Self::External(_) => ReferenceScope::External,
        }
    }

    pub fn internal(&self) -> Option<InternalNamespace> {
        match self {
            Self::Internal(namespace) => Some(*namespace),
            Self::External(_) => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Internal(namespace) => namespace.as_str(),
            Self::External(namespace) => namespace.as_str(),
        }
    }
}

impl fmt::Display for ReferenceNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
