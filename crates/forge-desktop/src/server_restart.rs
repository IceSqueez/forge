use forge_server::{ServerError, ServerHandle};

pub(crate) async fn restart_ignoring_disabled(handle: &ServerHandle) -> Result<(), String> {
    match handle.restart().await {
        Ok(()) => Ok(()),
        Err(ServerError::Disabled) => Ok(()),
        Err(other) => Err(other.to_string()),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use forge_server::ServerSettings;
    use forge_storage::SettingsRepo;
    use tokio::net::TcpListener;

    use super::*;
    use crate::test_support::{TestBackend, stopped_server_handle, test_backend};

    const LOOPBACK: &str = "127.0.0.1";

    async fn persist_bind(backend: &Arc<TestBackend>, port: u16) {
        let repo: &dyn SettingsRepo = backend.as_ref();
        ServerSettings::save_bind_address(repo, LOOPBACK)
            .await
            .expect("save bind address");
        ServerSettings::save_port(repo, port)
            .await
            .expect("save port");
    }

    #[tokio::test]
    async fn a_restart_the_settings_switch_refused_is_not_reported_to_the_user() {
        let (backend, _writes) = test_backend();
        ServerSettings::save_enabled(backend.as_ref() as &dyn SettingsRepo, false)
            .await
            .expect("save enabled");
        let handle = stopped_server_handle(&backend).await;

        assert_eq!(restart_ignoring_disabled(&handle).await, Ok(()));
    }

    #[tokio::test]
    async fn a_restart_that_cannot_take_the_port_reports_the_address_it_wanted() {
        let (backend, _writes) = test_backend();
        let squatter = TcpListener::bind(format!("{LOOPBACK}:0"))
            .await
            .expect("squatter bind");
        let port = squatter.local_addr().expect("squatter addr").port();
        persist_bind(&backend, port).await;
        let handle = stopped_server_handle(&backend).await;

        let message = restart_ignoring_disabled(&handle)
            .await
            .expect_err("a port already held must surface");

        assert!(
            message.contains(&port.to_string()),
            "the user must be told which address was refused: {message}"
        );
        drop(squatter);
    }
}
