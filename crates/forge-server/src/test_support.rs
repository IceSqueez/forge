use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use forge_registry::SubActionRegistry;
use forge_runtime::{
    ActionCancelRegistry, Catalog, EventBus, NullEventLogRepo, spawn_action_engine,
};
use forge_storage::test_support::{MemoryCredentials, MemorySettings};
use forge_storage::{
    ActionRepo, CatalogRevision, CredentialsRepo, GlobalsRepo, MockActionRepo, MockGlobalsRepo,
    MockHistoryRepo, MockOverlayRepo, MockTriggerInstanceRepo, MockUserGlobalsRepo, OverlayRepo,
    SettingsRepo, UserGlobalsRepo,
};

use crate::config::ServerConfig;

const EPHEMERAL_PORT: u16 = 0;

pub struct ServerDeps {
    settings: Arc<dyn SettingsRepo>,
    credentials: Arc<dyn CredentialsRepo>,
    actions: Arc<dyn ActionRepo>,
    globals: Arc<dyn GlobalsRepo>,
    user_globals: Arc<dyn UserGlobalsRepo>,
    overlays: Arc<dyn OverlayRepo>,
    overlay_root: PathBuf,
}

impl ServerDeps {
    pub fn new(overlay_root: impl Into<PathBuf>) -> Self {
        let mut overlays = MockOverlayRepo::new();
        overlays.expect_get().returning(|_| Ok(None));
        Self {
            settings: Arc::new(MemorySettings::new()),
            credentials: Arc::new(MemoryCredentials::default()),
            actions: Arc::new(MockActionRepo::new()),
            globals: Arc::new(MockGlobalsRepo::new()),
            user_globals: Arc::new(MockUserGlobalsRepo::new()),
            overlays: Arc::new(overlays),
            overlay_root: overlay_root.into(),
        }
    }

    #[must_use]
    pub fn with_settings(mut self, settings: Arc<dyn SettingsRepo>) -> Self {
        self.settings = settings;
        self
    }

    #[must_use]
    pub fn with_credentials(mut self, credentials: Arc<dyn CredentialsRepo>) -> Self {
        self.credentials = credentials;
        self
    }

    #[must_use]
    pub fn with_actions(mut self, actions: Arc<dyn ActionRepo>) -> Self {
        self.actions = actions;
        self
    }

    #[must_use]
    pub fn with_globals(mut self, globals: Arc<dyn GlobalsRepo>) -> Self {
        self.globals = globals;
        self
    }

    #[must_use]
    pub fn with_user_globals(mut self, user_globals: Arc<dyn UserGlobalsRepo>) -> Self {
        self.user_globals = user_globals;
        self
    }

    #[must_use]
    pub fn with_overlays(mut self, overlays: Arc<dyn OverlayRepo>) -> Self {
        self.overlays = overlays;
        self
    }

    pub fn build(self) -> ServerConfig {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = Arc::new(spawn_action_engine(
            Arc::clone(&bus),
            Catalog::new(
                Arc::clone(&self.actions),
                Arc::new(MockTriggerInstanceRepo::new()),
                CatalogRevision::new(),
            ),
            Arc::clone(&self.actions),
            Arc::new(MockHistoryRepo::new()),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
            forge_runtime::DateNamesHandle::default(),
        ));
        let mut config = ServerConfig::new(
            self.settings,
            self.credentials,
            bus,
            self.actions,
            self.globals,
            self.user_globals,
            self.overlays,
            engine,
            self.overlay_root,
        );
        config.bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), EPHEMERAL_PORT);
        config
    }
}
