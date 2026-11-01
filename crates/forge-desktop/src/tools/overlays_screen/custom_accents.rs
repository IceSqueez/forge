use std::sync::Arc;

use forge_components::{PushToast, ToastKind, tr};
use forge_overlay::hex_color_channels;
use forge_storage::settings::core_settings::OVERLAY_CUSTOM_ACCENTS;
use forge_storage::{SettingsRepo, read_stored_setting, write_setting};
use gpui::Context;
use tokio::runtime::Handle;

use crate::app::async_bridge::{self, Loadable, SnapshotWriter, WriteOutcome};

pub(super) const MAX_CUSTOM_ACCENTS: usize = 24;

pub(super) fn normalized_accent(typed: &str) -> Option<String> {
    let color = typed.trim().to_ascii_lowercase();
    hex_color_channels(&color).map(|_| color)
}

pub(super) fn with_accent(colors: &[String], color: &str) -> Vec<String> {
    let kept = colors.iter().filter(|known| *known != color).cloned();
    let mut next: Vec<String> = kept.collect();
    next.push(color.to_owned());
    let overflow = next.len().saturating_sub(MAX_CUSTOM_ACCENTS);
    next.drain(..overflow);
    next
}

pub(super) fn without_accent(colors: &[String], color: &str) -> Vec<String> {
    colors
        .iter()
        .filter(|known| *known != color)
        .cloned()
        .collect()
}

pub(super) fn known_accents(stored: Vec<String>) -> Vec<String> {
    stored.iter().fold(Vec::new(), |known, typed| {
        normalized_accent(typed).map_or(known.clone(), |color| with_accent(&known, &color))
    })
}

pub(super) struct CustomAccents {
    repo: Arc<dyn SettingsRepo>,
    rt_handle: Handle,
    colors: Loadable<Vec<String>>,
    writer: SnapshotWriter,
}

impl CustomAccents {
    pub(super) fn new(
        repo: Arc<dyn SettingsRepo>,
        rt_handle: Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let writer = SnapshotWriter::new(&rt_handle);
        let mut store = Self {
            repo,
            rt_handle,
            colors: Loadable::Loading,
            writer,
        };
        store.load(cx);
        store
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.colors = Loadable::Loading;
        let repo = Arc::clone(&self.repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                read_stored_setting(repo.as_ref(), &OVERLAY_CUSTOM_ACCENTS)
                    .await
                    .map(|stored| known_accents(stored.unwrap_or_default()))
            },
            move |this, result, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%error, "failed to load custom overlay accents");
                }
                this.colors = Loadable::settled(result);
                cx.notify();
            },
            cx,
        );
    }

    pub(super) fn colors(&self) -> &[String] {
        self.colors.loaded().map_or(&[], Vec::as_slice)
    }

    pub(super) fn add(&mut self, color: &str, cx: &mut Context<Self>) {
        self.replace(with_accent(self.colors(), color), cx);
    }

    pub(super) fn remove(&mut self, color: &str, cx: &mut Context<Self>) {
        self.replace(without_accent(self.colors(), color), cx);
    }

    fn replace(&mut self, next: Vec<String>, cx: &mut Context<Self>) {
        let Some(colors) = self.colors.loaded_mut() else {
            return;
        };
        colors.clone_from(&next);
        let repo = Arc::clone(&self.repo);
        self.writer.write(
            async move { write_setting(repo.as_ref(), &OVERLAY_CUSTOM_ACCENTS, &next).await },
            Self::write_settled,
            cx,
        );
        cx.notify();
    }

    fn write_settled(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        if let WriteOutcome::Failed(error) = outcome {
            cx.push_toast(
                ToastKind::Error,
                tr!("overlays_accent_save_failed", reason = error.text()),
            );
            self.load(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_components::Toasts;
    use gpui::{AppContext, Entity, TestAppContext};
    use tokio::runtime::Runtime;

    use super::*;
    use crate::test_support::{error_toasts, pump, runtime, test_backend};

    const FIRST: &str = "#000001";
    const SECOND: &str = "#000002";

    fn list(colors: &[&str]) -> Vec<String> {
        colors.iter().map(|color| (*color).to_owned()).collect()
    }

    fn full_list() -> Vec<String> {
        (0..MAX_CUSTOM_ACCENTS)
            .map(|at| format!("#0000{at:02x}"))
            .collect()
    }

    fn open(
        repo: Arc<dyn SettingsRepo>,
        rt: &Runtime,
        cx: &mut TestAppContext,
    ) -> Entity<CustomAccents> {
        let handle = rt.handle().clone();
        cx.new(|cx| CustomAccents::new(repo, handle, cx))
    }

    fn settle(rt: &Runtime, cx: &mut TestAppContext) {
        for _ in 0..3 {
            pump(rt);
            cx.run_until_parked();
        }
    }

    fn shown(store: &Entity<CustomAccents>, cx: &mut TestAppContext) -> Vec<String> {
        store.read_with(cx, |store, _| store.colors().to_vec())
    }

    fn stored(repo: &Arc<dyn SettingsRepo>, rt: &Runtime) -> Option<Vec<String>> {
        rt.block_on(read_stored_setting(repo.as_ref(), &OVERLAY_CUSTOM_ACCENTS))
            .unwrap()
    }

    #[gpui::test]
    fn an_added_color_is_stored_and_the_next_session_lists_it(cx: &mut TestAppContext) {
        let rt = runtime();
        let (settings, _writes) = test_backend();
        let repo = settings.settings_repo();
        let store = open(Arc::clone(&repo), &rt, cx);
        settle(&rt, cx);

        store.update(cx, |store, cx| {
            store.add(FIRST, cx);
            store.add(SECOND, cx);
        });
        settle(&rt, cx);

        let next_session = open(Arc::clone(&repo), &rt, cx);
        settle(&rt, cx);
        assert_eq!(stored(&repo, &rt), Some(list(&[FIRST, SECOND])));
        assert_eq!(shown(&next_session, cx), list(&[FIRST, SECOND]));
    }

    #[gpui::test]
    fn a_removed_color_leaves_the_list_and_the_store(cx: &mut TestAppContext) {
        let rt = runtime();
        let (settings, _writes) = test_backend();
        let repo = settings.settings_repo();
        let store = open(Arc::clone(&repo), &rt, cx);
        settle(&rt, cx);
        store.update(cx, |store, cx| {
            store.add(FIRST, cx);
            store.add(SECOND, cx);
        });
        settle(&rt, cx);

        store.update(cx, |store, cx| store.remove(FIRST, cx));
        settle(&rt, cx);

        assert_eq!(shown(&store, cx), list(&[SECOND]));
        assert_eq!(stored(&repo, &rt), Some(list(&[SECOND])));
    }

    #[gpui::test]
    fn a_failed_save_reports_an_error_and_returns_to_what_is_stored(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_global(Toasts::new()));
        let rt = runtime();
        let (settings, _writes) = test_backend();
        let repo = settings.settings_repo();
        let store = open(Arc::clone(&repo), &rt, cx);
        settle(&rt, cx);
        store.update(cx, |store, cx| store.add(FIRST, cx));
        settle(&rt, cx);
        settings.fail_writes(OVERLAY_CUSTOM_ACCENTS.name());

        store.update(cx, |store, cx| store.add(SECOND, cx));
        let optimistic = shown(&store, cx);
        settle(&rt, cx);

        assert_eq!(optimistic, list(&[FIRST, SECOND]));
        assert_eq!(shown(&store, cx), list(&[FIRST]));
        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
    }

    #[gpui::test]
    fn a_color_added_before_the_list_loads_writes_nothing_over_the_stored_one(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (settings, _writes) = test_backend();
        let repo = settings.settings_repo();
        let first = open(Arc::clone(&repo), &rt, cx);
        settle(&rt, cx);
        first.update(cx, |store, cx| store.add(FIRST, cx));
        settle(&rt, cx);

        let loading = open(Arc::clone(&repo), &rt, cx);
        loading.update(cx, |store, cx| store.add(SECOND, cx));
        settle(&rt, cx);

        assert_eq!(stored(&repo, &rt), Some(list(&[FIRST])));
        assert_eq!(shown(&loading, cx), list(&[FIRST]));
    }

    #[test]
    fn a_typed_color_is_kept_only_as_lowercase_six_digit_hex() {
        for (typed, expected) in [
            ("#AABBCC", Some("#aabbcc")),
            ("  #123456 ", Some("#123456")),
            ("123456", None),
            ("#12345", None),
            ("#1234567", None),
            ("#12345g", None),
            ("red", None),
            ("", None),
        ] {
            assert_eq!(normalized_accent(typed).as_deref(), expected, "{typed:?}");
        }
    }

    #[test]
    fn adding_a_known_color_moves_it_last_instead_of_listing_it_twice() {
        assert_eq!(
            with_accent(&list(&["#000001", "#000002"]), "#000001"),
            list(&["#000002", "#000001"])
        );
    }

    #[test]
    fn the_list_drops_its_oldest_color_past_the_cap() {
        let full: Vec<String> = (0..MAX_CUSTOM_ACCENTS)
            .map(|at| format!("#0000{at:02x}"))
            .collect();

        let next = with_accent(&full, "#ffffff");

        assert_eq!(next.len(), MAX_CUSTOM_ACCENTS);
        assert_eq!(next[0], full[1]);
        assert_eq!(next.last().map(String::as_str), Some("#ffffff"));
    }

    #[test]
    fn the_last_free_place_takes_a_color_without_dropping_any() {
        let mut one_short = full_list();
        one_short.pop();

        let next = with_accent(&one_short, "#ffffff");

        assert_eq!(next.len(), MAX_CUSTOM_ACCENTS);
        assert_eq!(next[..one_short.len()], one_short[..]);
    }

    #[test]
    fn moving_a_known_color_in_a_full_list_drops_nothing() {
        let full = full_list();

        let next = with_accent(&full, &full[3]);

        assert_eq!(next.len(), MAX_CUSTOM_ACCENTS);
        assert_eq!(next[0], full[0]);
        assert_eq!(next.last(), Some(&full[3]));
    }

    #[test]
    fn a_stored_list_over_the_cap_keeps_its_newest_colors() {
        let mut stored = full_list();
        stored.push("#ffffff".to_owned());

        let known = known_accents(stored.clone());

        assert_eq!(known[..], stored[1..]);
    }

    #[test]
    fn removal_takes_only_the_named_color() {
        assert_eq!(
            without_accent(&list(&["#000001", "#000002"]), "#000001"),
            list(&["#000002"])
        );
        assert_eq!(
            without_accent(&list(&["#000001"]), "#000009"),
            list(&["#000001"])
        );
    }

    #[test]
    fn a_stored_list_keeps_valid_colors_once_and_skips_the_rest() {
        assert_eq!(
            known_accents(list(&["#ABCDEF", "nonsense", "#abcdef", "#000001"])),
            list(&["#abcdef", "#000001"])
        );
    }
}
