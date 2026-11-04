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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use forge_overlay::SHOW_MEDIA_DIRECTORY;
    use forge_overlay::config::IMAGE;
    use tempfile::TempDir;

    use super::*;

    const OVERLAY: &str = "sub-alert";
    const BLOB: &str = "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";

    fn picture() -> ResolvedMedia {
        ResolvedMedia::new(IMAGE, BLOB, "png", b"\x89PNG".to_vec()).unwrap()
    }

    fn overlay_root(home: &TempDir) -> PathBuf {
        let root = home.path().join("overlays");
        std::fs::create_dir_all(root.join(OVERLAY)).unwrap();
        root
    }

    fn show_media_of(root: &Path) -> PathBuf {
        root.join(OVERLAY).join(SHOW_MEDIA_DIRECTORY)
    }

    fn lease(holds: &ShowMediaHolds, root: &Path) -> ShowMediaLease {
        holds
            .place(root.to_path_buf(), OVERLAY.to_owned(), &[picture()])
            .unwrap()
            .unwrap()
    }

    #[test]
    fn show_media_stays_on_disk_until_the_last_show_holding_it_ends() {
        let home = TempDir::new().unwrap();
        let root = overlay_root(&home);
        let holds = ShowMediaHolds::default();
        let first = lease(&holds, &root);
        let second = lease(&holds, &root);

        drop(first);
        assert!(
            show_media_of(&root).join(picture().file_name()).is_file(),
            "an ended show took the file a later show still plays"
        );

        drop(second);
        assert!(!show_media_of(&root).exists());
    }

    #[test]
    fn an_overlay_without_a_folder_takes_no_lease_and_gets_no_folder() {
        let home = TempDir::new().unwrap();
        let root = home.path().join("overlays");
        std::fs::create_dir_all(&root).unwrap();

        let placed = ShowMediaHolds::default()
            .place(root.clone(), OVERLAY.to_owned(), &[picture()])
            .unwrap();

        assert!(placed.is_none());
        assert!(!root.join(OVERLAY).exists());
    }

    #[test]
    fn an_applied_show_keeps_its_media_until_the_next_one_replaces_it_or_the_overlay_goes() {
        let home = TempDir::new().unwrap();
        let root = overlay_root(&home);
        let holds = ShowMediaHolds::default();
        let id = OverlayId::new(OVERLAY);

        holds.keep_applied(&id, Some(lease(&holds, &root)));
        holds.keep_applied(&id, Some(lease(&holds, &root)));
        assert!(
            show_media_of(&root).exists(),
            "replacing the applied show released the media of the one now on screen"
        );

        holds.keep_applied(&id, None);
        assert!(
            !show_media_of(&root).exists(),
            "the replaced show still holds its media, so the folder outlives every show"
        );
    }
}
