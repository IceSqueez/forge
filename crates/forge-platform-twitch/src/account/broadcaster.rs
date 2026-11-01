use std::sync::RwLock;

use forge_types::RecoverRwLock;

use crate::credentials::StoredCredential;
use crate::error::TwitchError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BroadcasterIdentity {
    user_id: String,
    login: String,
}

impl BroadcasterIdentity {
    pub fn new(user_id: impl Into<String>, login: impl Into<String>) -> Option<Self> {
        let user_id = user_id.into();
        (!user_id.is_empty()).then(|| Self {
            user_id,
            login: login.into(),
        })
    }

    pub(crate) fn from_stored(stored: &StoredCredential) -> Option<Self> {
        Self::new(stored.user_id.as_str(), stored.login.as_str())
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn login(&self) -> &str {
        &self.login
    }
}

#[derive(Default)]
pub struct BroadcasterSlot {
    active: RwLock<Option<BroadcasterIdentity>>,
}

impl BroadcasterSlot {
    pub fn holding(identity: Option<BroadcasterIdentity>) -> Self {
        Self {
            active: RwLock::new(identity),
        }
    }

    pub(crate) fn replace(&self, identity: Option<BroadcasterIdentity>) {
        *self.active.write_recover() = identity;
    }

    pub(crate) fn current(&self) -> Result<BroadcasterIdentity, TwitchError> {
        self.active
            .read_recover()
            .clone()
            .ok_or(TwitchError::NotSignedIn)
    }
}
