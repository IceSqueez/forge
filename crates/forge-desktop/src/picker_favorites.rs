use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use forge_components::{FavoritesStatus, GridPicker, GridPickerEvent, PushToast, ToastKind, tr};
use forge_storage::{SettingsRepo, reserved_keys, set_json_setting, stored_json_setting};
use gpui::{App, AppContext, Context, Entity, Global, SharedString, Subscription};
use tokio::runtime::Handle;

use crate::async_bridge::{self, Loadable, SnapshotWriter, WriteOutcome};

#[derive(Clone, Copy)]
pub struct FavoritesList {
    pub key: &'static str,
    pub from_stored: fn(Vec<String>) -> HashSet<SharedString>,
}

pub const SUB_ACTION_FAVORITES: FavoritesList = FavoritesList {
    key: reserved_keys::PICKER_FAVORITES_SUB_ACTIONS,
    from_stored: to_set,
};

pub const TRIGGER_FAVORITES: FavoritesList = FavoritesList {
    key: reserved_keys::PICKER_FAVORITES_TRIGGERS,
    from_stored: to_set,
};

pub fn to_set(ids: Vec<String>) -> HashSet<SharedString> {
    ids.into_iter().map(SharedString::from).collect()
}

pub fn to_ids(favorites: &HashSet<SharedString>) -> Vec<String> {
    favorites.iter().map(|s| s.to_string()).collect()
}

#[derive(Default)]
struct SharedFavorites(HashMap<&'static str, Entity<StoredFavorites>>);

impl Global for SharedFavorites {}

pub struct StoredFavorites {
    list: FavoritesList,
    repo: Arc<dyn SettingsRepo>,
    rt_handle: Handle,
    ids: Loadable<HashSet<SharedString>>,
    writer: SnapshotWriter,
}

impl StoredFavorites {
    pub fn shared(
        list: FavoritesList,
        repo: Arc<dyn SettingsRepo>,
        rt_handle: &Handle,
        cx: &mut App,
    ) -> Entity<Self> {
        let existing = cx
            .try_global::<SharedFavorites>()
            .and_then(|shared| shared.0.get(list.key))
            .cloned();
        if let Some(store) = existing {
            store.update(cx, |store, cx| {
                if store.ids.failure().is_some() {
                    store.load(cx);
                }
            });
            return store;
        }
        let store = cx.new(|cx| {
            let mut store = Self {
                list,
                repo,
                rt_handle: rt_handle.clone(),
                ids: Loadable::Loading,
                writer: SnapshotWriter::new(rt_handle),
            };
            store.load(cx);
            store
        });
        cx.default_global::<SharedFavorites>()
            .0
            .insert(list.key, store.clone());
        store
    }

    pub fn load(&mut self, cx: &mut Context<Self>) {
        self.ids = Loadable::Loading;
        let repo = Arc::clone(&self.repo);
        let list = self.list;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                stored_json_setting::<Vec<String>>(repo.as_ref(), list.key)
                    .await
                    .map(|ids| (list.from_stored)(ids.unwrap_or_default()))
            },
            move |this, result, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%error, key = list.key, "failed to load picker favorites");
                }
                this.ids = Loadable::settled(result);
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    pub fn ids(&self) -> Option<&HashSet<SharedString>> {
        self.ids.loaded()
    }

    pub fn status(&self) -> FavoritesStatus {
        match &self.ids {
            Loadable::Loaded(_) => FavoritesStatus::Ready,
            Loadable::Loading => FavoritesStatus::Pending,
            Loadable::Failed(_) => {
                FavoritesStatus::Failed(tr!("picker_favorites_load_failed").into())
            }
        }
    }

    pub fn toggle(&mut self, id: &SharedString, cx: &mut Context<Self>) {
        let Some(ids) = self.ids.loaded_mut() else {
            return;
        };
        if !ids.remove(id) {
            ids.insert(id.clone());
        }
        let stored = to_ids(ids);
        let repo = Arc::clone(&self.repo);
        let key = self.list.key;
        self.writer.write(
            async move { set_json_setting(repo.as_ref(), key, &stored).await },
            Self::write_settled,
            cx,
        );
        cx.notify();
    }

    fn write_settled(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        if let WriteOutcome::Failed(error) = outcome {
            cx.push_toast(
                ToastKind::Error,
                tr!("picker_favorites_save_failed", reason = error.as_str()),
            );
            self.load(cx);
        }
    }
}

pub fn follow_picker_event(store: &Entity<StoredFavorites>, event: &GridPickerEvent, cx: &mut App) {
    match event {
        GridPickerEvent::FavoriteToggled(id) => store.update(cx, |store, cx| store.toggle(id, cx)),
        GridPickerEvent::FavoritesRetry => store.update(cx, |store, cx| store.load(cx)),
        GridPickerEvent::Picked(_) | GridPickerEvent::Dismissed => {}
    }
}

pub fn bind_picker(
    store: &Entity<StoredFavorites>,
    picker: &Entity<GridPicker>,
    cx: &mut App,
) -> Subscription {
    push_to_picker(store, picker, cx);
    let picker = picker.downgrade();
    cx.observe(store, move |store, cx| {
        if let Some(picker) = picker.upgrade() {
            push_to_picker(&store, &picker, cx);
        }
    })
}

fn push_to_picker(store: &Entity<StoredFavorites>, picker: &Entity<GridPicker>, cx: &mut App) {
    let (ids, status) = {
        let store = store.read(cx);
        (store.ids().cloned(), store.status())
    };
    picker.update(cx, |picker, cx| {
        if let Some(ids) = ids {
            picker.set_favorites(ids, cx);
        }
        picker.set_favorites_status(status, cx);
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use forge_components::Toasts;
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::*;
    use crate::test_support::{
        ReadFault, SettingWrite, TestBackend, error_toasts, install_presentation, pump, runtime,
        test_backend, writes_to,
    };

    const KEPT: &str = "sub.enable";
    const ADDED: &str = "sub.raw";
    const STORED: &str = r#"["sub.enable"]"#;
    const UNDECODABLE: &str = "not json";
    const SETTLE_ROUNDS: usize = 3;

    pub(crate) struct Harness {
        pub(crate) rt: tokio::runtime::Runtime,
        pub(crate) backend: Arc<TestBackend>,
        writes: UnboundedReceiver<SettingWrite>,
    }

    impl Harness {
        pub(crate) fn new(cx: &mut gpui::TestAppContext) -> Self {
            install_presentation(cx);
            cx.update(|cx| {
                cx.set_global(Toasts::new());
                cx.set_global(SharedFavorites::default());
            });
            let (backend, writes) = test_backend();
            Self {
                rt: runtime(),
                backend,
                writes,
            }
        }

        pub(crate) fn shared(
            &self,
            list: FavoritesList,
            cx: &mut gpui::TestAppContext,
        ) -> Entity<StoredFavorites> {
            let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
            let handle = self.rt.handle().clone();
            let store = cx.update(|cx| StoredFavorites::shared(list, repo, &handle, cx));
            self.settle(cx);
            store
        }

        pub(crate) fn settle(&self, cx: &mut gpui::TestAppContext) {
            for _ in 0..SETTLE_ROUNDS {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }

        pub(crate) fn written(&mut self, list: FavoritesList) -> Vec<Vec<String>> {
            writes_to(&mut self.writes, list.key)
                .iter()
                .map(|raw| {
                    let mut ids: Vec<String> = serde_json::from_str(raw).unwrap();
                    ids.sort();
                    ids
                })
                .collect()
        }
    }

    pub(crate) fn shown(
        store: &Entity<StoredFavorites>,
        cx: &mut gpui::TestAppContext,
    ) -> Option<Vec<String>> {
        store.read_with(cx, |store, _| {
            store.ids().map(|ids| {
                let mut ids = to_ids(ids);
                ids.sort();
                ids
            })
        })
    }

    pub(crate) fn star(store: &Entity<StoredFavorites>, id: &str, cx: &mut gpui::TestAppContext) {
        let event = GridPickerEvent::FavoriteToggled(SharedString::from(id.to_owned()));
        cx.update(|cx| follow_picker_event(store, &event, cx));
    }

    type Edit = fn(&Entity<StoredFavorites>, &mut gpui::TestAppContext);

    fn edit_paths() -> [(&'static str, Edit); 2] {
        [
            ("a star click relayed from a picker", |store, cx| {
                star(store, ADDED, cx)
            }),
            ("a direct toggle on the store", |store, cx| {
                store.update(cx, |store, cx| store.toggle(&SharedString::from(ADDED), cx))
            }),
        ]
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    #[gpui::test]
    fn no_star_toggle_after_a_failed_load_writes_over_the_stored_favorites(
        cx: &mut gpui::TestAppContext,
    ) {
        for (fault, stored, failing_read) in [
            ("unreachable store", STORED, true),
            ("undecodable value", UNDECODABLE, false),
        ] {
            for (case, edit) in edit_paths() {
                let mut harness = Harness::new(cx);
                harness.backend.seed(SUB_ACTION_FAVORITES.key, stored);
                if failing_read {
                    harness
                        .backend
                        .fault_reads(SUB_ACTION_FAVORITES.key, ReadFault::Fails);
                }
                let store = harness.shared(SUB_ACTION_FAVORITES, cx);

                edit(&store, cx);
                harness.settle(cx);

                let status = store.read_with(cx, |store, _| store.status());
                assert_eq!(
                    (
                        harness.written(SUB_ACTION_FAVORITES),
                        matches!(status, FavoritesStatus::Failed(_)),
                        shown(&store, cx)
                    ),
                    (Vec::<Vec<String>>::new(), true, None),
                    "{fault}: {case}"
                );
            }
        }
    }

    #[gpui::test]
    fn no_star_toggle_while_the_load_is_pending_writes_and_the_load_still_lands_intact(
        cx: &mut gpui::TestAppContext,
    ) {
        for (case, edit) in edit_paths() {
            let mut harness = Harness::new(cx);
            harness.backend.seed(SUB_ACTION_FAVORITES.key, STORED);
            let gate = Arc::new(tokio::sync::Notify::new());
            harness
                .backend
                .fault_reads(SUB_ACTION_FAVORITES.key, ReadFault::Held(Arc::clone(&gate)));
            let store = harness.shared(SUB_ACTION_FAVORITES, cx);

            edit(&store, cx);
            harness.settle(cx);
            let while_pending = (
                harness.written(SUB_ACTION_FAVORITES),
                store.read_with(cx, |store, _| store.status()),
            );
            harness.backend.heal_reads(SUB_ACTION_FAVORITES.key);
            gate.notify_one();
            harness.settle(cx);

            assert_eq!(
                (while_pending, shown(&store, cx)),
                (
                    (Vec::<Vec<String>>::new(), FavoritesStatus::Pending),
                    Some(ids(&[KEPT]))
                ),
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn retry_from_the_picker_after_a_failed_load_restores_the_stored_favorites_and_a_star_keeps_them(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut harness = Harness::new(cx);
        harness.backend.seed(SUB_ACTION_FAVORITES.key, STORED);
        harness
            .backend
            .fault_reads(SUB_ACTION_FAVORITES.key, ReadFault::Fails);
        let store = harness.shared(SUB_ACTION_FAVORITES, cx);
        harness.backend.heal_reads(SUB_ACTION_FAVORITES.key);

        cx.update(|cx| follow_picker_event(&store, &GridPickerEvent::FavoritesRetry, cx));
        harness.settle(cx);
        star(&store, ADDED, cx);
        harness.settle(cx);

        assert_eq!(
            harness.written(SUB_ACTION_FAVORITES),
            vec![ids(&[KEPT, ADDED])]
        );
    }

    #[gpui::test]
    fn opening_another_picker_after_a_failed_load_reloads_the_shared_favorites(
        cx: &mut gpui::TestAppContext,
    ) {
        let harness = Harness::new(cx);
        harness.backend.seed(TRIGGER_FAVORITES.key, STORED);
        harness
            .backend
            .fault_reads(TRIGGER_FAVORITES.key, ReadFault::Fails);
        let first = harness.shared(TRIGGER_FAVORITES, cx);
        harness.backend.heal_reads(TRIGGER_FAVORITES.key);

        harness.shared(TRIGGER_FAVORITES, cx);

        assert_eq!(shown(&first, cx), Some(ids(&[KEPT])));
    }

    #[gpui::test]
    fn a_failed_write_reports_the_error_and_rolls_back_to_the_stored_favorites(
        cx: &mut gpui::TestAppContext,
    ) {
        let harness = Harness::new(cx);
        harness.backend.seed(SUB_ACTION_FAVORITES.key, STORED);
        harness.backend.fail_writes(SUB_ACTION_FAVORITES.key);
        let store = harness.shared(SUB_ACTION_FAVORITES, cx);

        star(&store, ADDED, cx);
        harness.settle(cx);

        assert_eq!(
            (cx.update(|cx| error_toasts(cx)), shown(&store, cx)),
            (1, Some(ids(&[KEPT])))
        );
    }

    #[gpui::test]
    fn both_trigger_pickers_share_one_copy_so_a_star_toggled_in_one_shows_in_the_other(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut harness = Harness::new(cx);
        harness.backend.seed(TRIGGER_FAVORITES.key, STORED);
        let actions_copy = harness.shared(TRIGGER_FAVORITES, cx);
        let triggers_copy = harness.shared(TRIGGER_FAVORITES, cx);

        star(&actions_copy, ADDED, cx);
        harness.settle(cx);

        assert_eq!(
            (
                shown(&triggers_copy, cx),
                harness.written(TRIGGER_FAVORITES)
            ),
            (Some(ids(&[KEPT, ADDED])), vec![ids(&[KEPT, ADDED])])
        );
    }

    #[gpui::test]
    fn each_favorites_list_keeps_its_own_copy(cx: &mut gpui::TestAppContext) {
        let harness = Harness::new(cx);
        harness.backend.seed(TRIGGER_FAVORITES.key, STORED);
        harness.backend.seed(SUB_ACTION_FAVORITES.key, STORED);
        let triggers = harness.shared(TRIGGER_FAVORITES, cx);
        let sub_actions = harness.shared(SUB_ACTION_FAVORITES, cx);

        star(&triggers, ADDED, cx);
        harness.settle(cx);

        assert_eq!(shown(&sub_actions, cx), Some(ids(&[KEPT])));
    }
}
