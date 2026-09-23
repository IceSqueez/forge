use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use super::endpoint::PageEndpoint;
use super::page::OverlayPage;
use crate::EmulatorError;
use crate::fixture::SeedReport;

/// Resolves the display names a scenario uses to the identity slugs forge minted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayIndex {
    by_display_name: HashMap<String, String>,
}

impl OverlayIndex {
    pub fn from_seed(seed: &SeedReport) -> Self {
        Self {
            by_display_name: seed
                .overlays
                .iter()
                .map(|overlay| (overlay.display_name.clone(), overlay.id.clone()))
                .collect(),
        }
    }

    pub fn resolve(&self, display_name: &str) -> Option<&str> {
        self.by_display_name.get(display_name).map(String::as_str)
    }
}

/// The browser sources a run has opened, keyed by the display name the scenario uses.
pub struct OverlayPages {
    endpoint: PageEndpoint,
    index: OverlayIndex,
    open: Mutex<HashMap<String, Arc<OverlayPage>>>,
}

/// The frame count each open page had when a step began; a page opened by that step starts at 0,
/// so content delivered during its handshake still counts as the step's own.
#[derive(Debug, Clone, Default)]
pub struct OverlayMarks(HashMap<String, usize>);

impl OverlayMarks {
    pub fn of(&self, display_name: &str) -> usize {
        self.0.get(display_name).copied().unwrap_or(0)
    }
}

impl OverlayPages {
    pub fn for_seed(seed: &SeedReport) -> Result<Self, EmulatorError> {
        Ok(Self {
            endpoint: PageEndpoint::loopback(seed.server.port)?,
            index: OverlayIndex::from_seed(seed),
            open: Mutex::new(HashMap::new()),
        })
    }

    /// Opening the same overlay twice replaces the earlier page, which closes with its task.
    pub async fn open_page(
        &self,
        display_name: &str,
        timeout: Duration,
    ) -> Result<String, EmulatorError> {
        let identity = self
            .index
            .resolve(display_name)
            .ok_or_else(|| EmulatorError::OverlayNotSeeded {
                overlay: display_name.to_owned(),
            })?
            .to_owned();
        let page = OverlayPage::open(&self.endpoint, &identity, timeout).await?;
        self.lock().insert(display_name.to_owned(), Arc::new(page));
        Ok(identity)
    }

    pub fn page(&self, display_name: &str) -> Option<Arc<OverlayPage>> {
        self.lock().get(display_name).map(Arc::clone)
    }

    pub fn marks(&self) -> OverlayMarks {
        OverlayMarks(
            self.lock()
                .iter()
                .map(|(name, page)| (name.clone(), page.len()))
                .collect(),
        )
    }

    /// Drops every page, which aborts its reader and closes its socket.
    pub fn close(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<OverlayPage>>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::fixture::{SeedReport, SeededOverlay, SeededServer};

    fn seed(overlays: Vec<SeededOverlay>) -> SeedReport {
        SeedReport {
            data_dir: std::path::PathBuf::from("/nonexistent"),
            server: SeededServer {
                port: 41234,
                bearer_token: "bearer".to_owned(),
            },
            twitch: None,
            overlays,
            chat_commands: Vec::new(),
        }
    }

    fn overlay(display_name: &str, id: &str) -> SeededOverlay {
        SeededOverlay {
            id: id.to_owned(),
            display_name: display_name.to_owned(),
            kind_id: "overlay.alert".to_owned(),
            credential: "credential".to_owned(),
        }
    }

    #[test]
    fn a_scenarys_display_name_resolves_to_the_identity_the_repository_minted() {
        let index = OverlayIndex::from_seed(&seed(vec![
            overlay("Alert Box", "alert-box"),
            overlay("Sub Goal", "sub-goal-2"),
        ]));

        assert_eq!(index.resolve("Sub Goal"), Some("sub-goal-2"));
        assert_eq!(index.resolve("sub-goal-2"), None);
        assert_eq!(index.resolve("Missing"), None);
    }

    #[tokio::test]
    async fn opening_an_overlay_the_seed_never_created_is_refused_before_any_connection() {
        let pages = OverlayPages::for_seed(&seed(Vec::new())).expect("a non-zero server port");

        let refusal = pages.open_page("Alert Box", Duration::from_millis(1)).await;

        assert!(
            matches!(&refusal, Err(EmulatorError::OverlayNotSeeded { overlay }) if overlay == "Alert Box"),
            "got {refusal:?}"
        );
    }

    #[test]
    fn an_overlay_with_no_page_open_marks_at_zero_rather_than_being_absent() {
        let pages = OverlayPages::for_seed(&seed(vec![overlay("Alert Box", "alert-box")]))
            .expect("a non-zero server port");

        assert_eq!(pages.marks().of("Alert Box"), 0);
        assert!(pages.page("Alert Box").is_none());
    }
}
