use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use forge_overlay::{OverlayError, ResolvedMedia, clear_show_media, place_show_media};
use forge_storage::OverlayId;
use forge_types::RecoverMutex;

#[derive(Clone, Default)]
pub(crate) struct ShowMediaHolds {
    leases: Arc<Mutex<HashMap<String, usize>>>,
    kept: Arc<Mutex<HashMap<OverlayId, ShowMediaLease>>>,
}

impl ShowMediaHolds {
    pub(crate) fn place(
        &self,
        root: PathBuf,
        identity: String,
        media: &[ResolvedMedia],
    ) -> Result<Option<ShowMediaLease>, OverlayError> {
        let mut leases = self.leases.lock_recover();
        if !place_show_media(&root, &identity, media)? {
            return Ok(None);
        }
        *leases.entry(identity.clone()).or_default() += 1;
        Ok(Some(ShowMediaLease {
            holds: self.clone(),
            root,
            identity,
        }))
    }

    pub(crate) fn keep_applied(&self, id: &OverlayId, lease: Option<ShowMediaLease>) {
        let released = {
            let mut kept = self.kept.lock_recover();
            match lease {
                Some(lease) => kept.insert(id.clone(), lease),
                None => kept.remove(id),
            }
        };
        drop(released);
    }

    fn release(&self, root: &std::path::Path, identity: &str) {
        let mut leases = self.leases.lock_recover();
        let Some(held) = leases.get_mut(identity) else {
            return;
        };
        *held = held.saturating_sub(1);
        if *held > 0 {
            return;
        }
        leases.remove(identity);
        if let Err(error) = clear_show_media(root, identity) {
            tracing::warn!(overlay = %identity, %error, "show media left behind after its shows ended");
        }
    }
}

pub(crate) struct ShowMediaLease {
    holds: ShowMediaHolds,
    root: PathBuf,
    identity: String,
}

impl Drop for ShowMediaLease {
    fn drop(&mut self) {
        let holds = self.holds.clone();
        let root = std::mem::take(&mut self.root);
        let identity = std::mem::take(&mut self.identity);
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                drop(runtime.spawn_blocking(move || holds.release(&root, &identity)));
            }
            Err(_) => holds.release(&root, &identity),
        }
    }
}
