use crate::backend::HotkeyId;
use crate::client::HotkeyClient;

pub fn registered_combos(client: &HotkeyClient) -> Vec<(HotkeyId, String)> {
    client
        .registered_combos()
        .into_iter()
        .map(|(id, combo)| (id, combo.as_str().to_owned()))
        .collect()
}

pub fn conflict_count(client: &HotkeyClient) -> usize {
    client.view().conflicts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn conflict_count_counts_a_rejected_re_registration_of_a_held_combo() {
        let (backend, _tx) = crate::backend::tests::MockPortalBackend::new();
        let client =
            crate::client::tests::start_supervised(backend, crate::client::tests::noop_publisher());
        let combo = crate::combo::HotkeyCombo::parse("Ctrl+F3").unwrap();
        client.register(combo.clone()).await.unwrap();

        client.register(combo).await.unwrap_err();

        assert_eq!(conflict_count(&client), 1);
    }
}
