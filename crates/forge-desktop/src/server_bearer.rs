use std::sync::Arc;

use forge_components::{
    ConfirmTone, ForgePalette, OverlayPosition, PushToast, ToastKind, confirm_modal, overlay, tr,
};
use forge_server::ServerHandle;
use forge_storage::{CredentialId, CredentialsRepo, SERVER_BEARER_CREDENTIAL_ID};
use forge_types::Secret;
use gpui::{AnyElement, App, ClickEvent, Context, prelude::*};

use crate::repo_failure::RepoFailure;

pub(crate) async fn current_bearer_token(
    server: Option<ServerHandle>,
    credentials: Arc<dyn CredentialsRepo>,
) -> Result<Option<Secret>, RepoFailure> {
    match server {
        Some(handle) => Ok(Some(handle.bearer_token().await)),
        None => credentials
            .load(&CredentialId::new(SERVER_BEARER_CREDENTIAL_ID))
            .await
            .map(|stored| stored.map(Secret::new))
            .map_err(RepoFailure::from),
    }
}

pub(crate) fn token_load_failed(reason: &str, cx: &mut App) {
    tracing::warn!(error = %reason, "failed to read the server bearer token");
    cx.push_toast(ToastKind::Error, tr!("server_token_load_failed"));
}

pub(crate) fn token_regenerate_failed(reason: &str, cx: &mut App) {
    tracing::warn!(error = %reason, "failed to regenerate the server bearer token");
    cx.push_toast(ToastKind::Error, tr!("server_token_regenerate_failed"));
}

pub(crate) fn regenerate_confirm<V: 'static>(
    palette: &ForgePalette,
    cancel: fn(&mut V, &mut Context<V>),
    confirm: fn(&mut V, &mut Context<V>),
    cx: &mut Context<V>,
) -> AnyElement {
    let card = confirm_modal(
        tr!("server_token_regen_title"),
        tr!("server_token_regen_message"),
        ConfirmTone::Destructive,
        palette,
    )
    .esc_hint(tr!("widget_confirm_esc_to_cancel"))
    .on_cancel(
        "server-token-regen-cancel",
        tr!("common_cancel"),
        cx.listener(move |this, _: &ClickEvent, _, cx| cancel(this, cx)),
    )
    .on_confirm(
        "server-token-regen-confirm",
        tr!("server_btn_regenerate"),
        cx.listener(move |this, _: &ClickEvent, _, cx| confirm(this, cx)),
    );
    let view = cx.entity();
    overlay(card, palette)
        .position(OverlayPosition::Center)
        .on_dismiss("server-token-regen-scrim", move |_window, cx| {
            view.update(cx, cancel);
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use forge_storage::StorageError;
    use time::OffsetDateTime;

    use super::*;
    use crate::test_support::{runtime, stopped_handle, test_backend};

    const STORED_TOKEN: &str = "fg_stored";

    struct Vault {
        rows: Mutex<HashMap<String, String>>,
        load_fails: bool,
    }

    impl Vault {
        fn holding(stored: Option<&str>, load_fails: bool) -> Arc<dyn CredentialsRepo> {
            let rows = stored
                .map(|token| (SERVER_BEARER_CREDENTIAL_ID.to_owned(), token.to_owned()))
                .into_iter()
                .collect();
            Arc::new(Self {
                rows: Mutex::new(rows),
                load_fails,
            })
        }
    }

    #[async_trait::async_trait]
    impl CredentialsRepo for Vault {
        async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
            self.rows
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), plaintext.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            if self.load_fails {
                return Err(StorageError::Connection {
                    reason: "credentials store unreachable".to_owned(),
                });
            }
            Ok(self.rows.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.rows.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .keys()
                .map(|key| CredentialId::new(key.clone()))
                .collect())
        }

        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    #[test]
    fn a_server_answers_with_its_live_token_over_the_stored_one() {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let server = stopped_handle(&rt, &backend);
        let live = rt.block_on(server.bearer_token());

        let answered = rt.block_on(current_bearer_token(
            Some(server),
            Vault::holding(Some(STORED_TOKEN), false),
        ));

        assert_eq!(answered, Ok(Some(live)));
    }

    #[test]
    fn a_server_answers_with_the_regenerated_token() {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let server = stopped_handle(&rt, &backend);
        let retired = rt.block_on(server.bearer_token());
        let regenerated = rt
            .block_on(server.regenerate_bearer_token())
            .expect("regenerate");

        let answered = rt.block_on(current_bearer_token(
            Some(server),
            Vault::holding(Some(retired.expose()), false),
        ));

        assert_eq!(answered, Ok(Some(regenerated)));
    }

    #[test]
    fn without_a_server_the_stored_token_answers_and_a_missing_or_unreadable_one_is_none() {
        let rt = runtime();
        for (stored, load_fails, expected, case) in [
            (
                Some(STORED_TOKEN),
                false,
                Some(STORED_TOKEN),
                "a stored token",
            ),
            (None, false, None, "an empty store"),
            (Some(STORED_TOKEN), true, None, "an unreadable store"),
        ] {
            let answered = rt.block_on(current_bearer_token(
                None,
                Vault::holding(stored, load_fails),
            ));

            assert_eq!(
                answered
                    .ok()
                    .flatten()
                    .as_ref()
                    .map(forge_types::Secret::expose),
                expected,
                "{case}"
            );
        }
    }
}
