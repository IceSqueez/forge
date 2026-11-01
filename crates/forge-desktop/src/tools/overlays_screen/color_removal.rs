use std::sync::Arc;

use forge_components::{
    Confirm, ConfirmTone, ForgePalette, OverlayPosition, PushToast, ToastKind, confirm_modal,
    overlay, tr,
};
use forge_overlay::OverlayKindRegistry;
use forge_runtime::{OverlaySampleSource, OverlayServiceHandle};
use forge_storage::settings::core_settings::OVERLAY_CUSTOM_ACCENTS;
use forge_storage::{
    OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo, write_setting,
};
use forge_types::Variant;
use gpui::{AnyElement, ClickEvent, Context, Entity, EventEmitter, prelude::*};
use tokio::runtime::Handle;

use super::color_choice::COLOR_FIELD_KEYS;
use super::custom_accents::{CustomAccents, normalized_accent, without_accent};
use super::writes::{Regenerated, regenerate};
use crate::app::async_bridge;
use crate::shared::repo_failure::RepoFailure;

#[derive(Clone)]
pub(super) struct OpenConfig {
    pub(super) id: OverlayId,
    pub(super) config: OverlayConfig,
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigReset {
    id: OverlayId,
    before: OverlayConfig,
    after: OverlayConfig,
    fields: usize,
}

fn holds_color(value: &Variant, color: &str) -> bool {
    matches!(value, Variant::String(text) if normalized_accent(text).as_deref() == Some(color))
}

fn without_color(config: &OverlayConfig, color: &str) -> OverlayConfig {
    config
        .iter()
        .filter(|(key, value)| {
            !(COLOR_FIELD_KEYS.contains(&key.as_str()) && holds_color(value, color))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn color_resets(
    definitions: &[OverlayDefinition],
    open: Option<&OpenConfig>,
    color: &str,
    kinds: &OverlayKindRegistry,
) -> Vec<ConfigReset> {
    definitions
        .iter()
        .filter(|definition| kinds.get(&definition.kind_id).is_some())
        .filter_map(|definition| {
            let current = open
                .filter(|open| open.id == definition.id)
                .map_or(&definition.config, |open| &open.config);
            let after = without_color(current, color);
            let fields = current.len() - after.len();
            (fields > 0).then(|| ConfigReset {
                id: definition.id.clone(),
                before: definition.config.clone(),
                after,
                fields,
            })
        })
        .collect()
}

#[derive(Clone)]
pub(super) struct CommittedReset {
    pub(super) id: OverlayId,
    pub(super) config: OverlayConfig,
    pub(super) regenerated: Regenerated,
}

pub(super) enum ColorRemovalEvent {
    Committed {
        color: String,
        resets: Vec<CommittedReset>,
    },
    SavedStateUnknown,
}

enum RemovalOutcome {
    Committed(Vec<CommittedReset>),
    Undone(RepoFailure),
    Stranded(RepoFailure),
}

struct ColorRemovalPrompt {
    color: String,
    open: Option<OpenConfig>,
    overlays: usize,
    fields: usize,
}

pub(super) struct ColorRemovalLaunch {
    pub(super) repo: Arc<dyn OverlayRepo>,
    pub(super) settings: Arc<dyn SettingsRepo>,
    pub(super) service: OverlayServiceHandle,
    pub(super) samples: Arc<dyn OverlaySampleSource>,
    pub(super) kinds: Arc<OverlayKindRegistry>,
    pub(super) rt_handle: Handle,
    pub(super) accents: Entity<CustomAccents>,
}

#[derive(Clone)]
struct RemovalWrite {
    repo: Arc<dyn OverlayRepo>,
    settings: Arc<dyn SettingsRepo>,
    service: OverlayServiceHandle,
    samples: Arc<dyn OverlaySampleSource>,
    kinds: Arc<OverlayKindRegistry>,
}

impl RemovalWrite {
    async fn undo(&self, written: &[&ConfigReset], failure: RepoFailure) -> RemovalOutcome {
        let mut restored = true;
        for reset in written.iter().rev() {
            if let Err(error) = self.repo.update_config(&reset.id, &reset.before).await {
                tracing::warn!(overlay = %reset.id, %error, "overlay config not restored");
                restored = false;
            }
        }
        if restored {
            RemovalOutcome::Undone(failure)
        } else {
            RemovalOutcome::Stranded(failure)
        }
    }

    async fn apply(
        self,
        color: String,
        open: Option<OpenConfig>,
        next_list: Vec<String>,
    ) -> RemovalOutcome {
        let definitions = match self.repo.list().await {
            Ok(definitions) => definitions,
            Err(error) => return RemovalOutcome::Undone(error.into()),
        };
        let resets = color_resets(&definitions, open.as_ref(), &color, &self.kinds);
        let mut written: Vec<&ConfigReset> = Vec::new();
        for reset in &resets {
            match self.service.update_config(&reset.id, &reset.after).await {
                Ok(true) => written.push(reset),
                Ok(false) => {}
                Err(error) => return self.undo(&written, error.into()).await,
            }
        }
        if let Err(error) =
            write_setting(self.settings.as_ref(), &OVERLAY_CUSTOM_ACCENTS, &next_list).await
        {
            return self.undo(&written, error.into()).await;
        }
        let mut committed = Vec::with_capacity(written.len());
        for reset in written {
            committed.push(CommittedReset {
                id: reset.id.clone(),
                config: reset.after.clone(),
                regenerated: regenerate(&self.service, self.samples.as_ref(), &reset.id).await,
            });
        }
        RemovalOutcome::Committed(committed)
    }
}

pub(super) struct ColorRemoval {
    write: RemovalWrite,
    rt_handle: Handle,
    accents: Entity<CustomAccents>,
    pending: Confirm<ColorRemovalPrompt>,
}

impl EventEmitter<ColorRemovalEvent> for ColorRemoval {}

impl ColorRemoval {
    pub(super) fn new(launch: ColorRemovalLaunch) -> Self {
        Self {
            write: RemovalWrite {
                repo: launch.repo,
                settings: launch.settings,
                service: launch.service,
                samples: launch.samples,
                kinds: launch.kinds,
            },
            rt_handle: launch.rt_handle,
            accents: launch.accents,
            pending: Confirm::default(),
        }
    }

    pub(super) fn prompting(&self) -> bool {
        self.pending.is_pending()
    }

    pub(super) fn request(
        &mut self,
        color: String,
        open: Option<OpenConfig>,
        cx: &mut Context<Self>,
    ) {
        let repo = Arc::clone(&self.write.repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.list().await.map_err(RepoFailure::from) },
            move |this, result, cx| this.settle_request(color, open, result, cx),
            cx,
        );
    }

    fn settle_request(
        &mut self,
        color: String,
        open: Option<OpenConfig>,
        result: Result<Vec<OverlayDefinition>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        let definitions = match result {
            Ok(definitions) => definitions,
            Err(failure) => {
                Self::toast_refused(&failure, cx);
                return;
            }
        };
        let resets = color_resets(&definitions, open.as_ref(), &color, &self.write.kinds);
        if resets.is_empty() {
            self.accents
                .update(cx, |store, cx| store.remove(&color, cx));
            return;
        }
        self.pending.request(ColorRemovalPrompt {
            color,
            open,
            overlays: resets.len(),
            fields: resets.iter().map(|reset| reset.fields).sum(),
        });
        cx.notify();
    }

    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        self.pending.cancel();
        cx.notify();
    }

    pub(super) fn confirm(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.pending.take() else {
            return;
        };
        let next_list = without_accent(self.accents.read(cx).colors(), &prompt.color);
        let adopted_list = next_list.clone();
        let color = prompt.color.clone();
        async_bridge::run_async(
            &self.rt_handle,
            self.write
                .clone()
                .apply(prompt.color, prompt.open, next_list),
            move |this: &mut Self, outcome, cx| {
                this.settle_confirm(color, adopted_list, outcome, cx);
            },
            cx,
        );
        cx.notify();
    }

    fn settle_confirm(
        &mut self,
        color: String,
        list: Vec<String>,
        outcome: RemovalOutcome,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            RemovalOutcome::Committed(resets) => {
                self.accents.update(cx, |store, cx| store.adopt(list, cx));
                cx.emit(ColorRemovalEvent::Committed { color, resets });
            }
            RemovalOutcome::Undone(failure) => Self::toast_refused(&failure, cx),
            RemovalOutcome::Stranded(failure) => {
                tracing::warn!(%failure, "color removal left overlays partly reset");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("overlays_accent_remove_stranded", reason = failure.text()),
                );
                cx.emit(ColorRemovalEvent::SavedStateUnknown);
            }
        }
    }

    fn toast_refused(failure: &RepoFailure, cx: &mut Context<Self>) {
        tracing::warn!(%failure, "color removal refused");
        cx.push_toast(
            ToastKind::Error,
            tr!("overlays_accent_remove_failed", reason = failure.text()),
        );
    }

    pub(super) fn render_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let prompt = self.pending.get()?;
        let card = confirm_modal(
            tr!("overlays_accent_remove_title"),
            format!(
                "{} {}",
                tr!(
                    "overlays_accent_remove_body",
                    overlays = i64::try_from(prompt.overlays).unwrap_or(i64::MAX)
                ),
                tr!(
                    "overlays_accent_remove_fields",
                    fields = i64::try_from(prompt.fields).unwrap_or(i64::MAX)
                ),
            ),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(prompt.color.clone())
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "overlays-color-remove-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        )
        .on_confirm(
            "overlays-color-remove-confirm",
            tr!("overlays_accent_remove_confirm"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
        );

        let weak = cx.entity().downgrade();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("overlays-color-remove-dismiss", move |_window, cx| {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the view may close before the result arrives"
                    )]
                    let _ = weak.update(cx, |this, cx| this.cancel(cx));
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};
    use std::path::PathBuf;
    use std::sync::Mutex;

    use forge_components::{Density, Presentation, ThemeId, Toasts};
    use forge_overlay::config::{ACCENT, BACKGROUND_COLOR, HEADLINE};
    use forge_overlay::kinds::alert::KIND_ID;
    use forge_runtime::{EventBus, OverlayFrameSink, OverlayReceivers, OverlayServiceParts};
    use forge_storage::{StorageError, read_stored_setting};
    use forge_types::OverlayCredential;
    use gpui::{AppContext, Subscription, TestAppContext};
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use tokio::runtime::Runtime;

    use super::*;
    use crate::test_support::{
        NeutralOverlaySamples, StubEventLog, TestBackend, error_toasts, pump, runtime, test_backend,
    };

    const CUSTOM: &str = "#102030";
    const OTHER: &str = "#405060";
    const FIRST: &str = "first";
    const SECOND: &str = "second";
    const UNTOUCHED: &str = "untouched";
    const VANISHED_KIND: &str = "overlay.vanished";
    const MAX_SETTLE_TURNS: usize = 200_000;

    fn config(entries: &[(&str, &str)]) -> OverlayConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
            .collect()
    }

    fn definition(id: &str, kind_id: &str, stored: OverlayConfig) -> OverlayDefinition {
        OverlayDefinition {
            id: OverlayId::new(id),
            display_name: id.to_owned(),
            kind_id: kind_id.to_owned(),
            enabled: true,
            position: 0,
            config: stored,
            config_schema_version: 1,
            generator_version: 1,
            source_overrides: Vec::new(),
            credential: OverlayCredential::new(id),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn builtin_kinds() -> OverlayKindRegistry {
        let mut kinds = OverlayKindRegistry::new();
        forge_overlay::register_builtin_kinds(&mut kinds).expect("builtin kinds register");
        kinds
    }

    #[derive(Default)]
    struct MemoryOverlays {
        rows: Mutex<BTreeMap<String, OverlayDefinition>>,
        refused: Mutex<HashSet<String>>,
        writes_left: Mutex<Option<usize>>,
        writes: Mutex<usize>,
    }

    impl MemoryOverlays {
        fn stored(&self, id: &str) -> OverlayConfig {
            self.rows.lock().unwrap()[id].config.clone()
        }

        fn writes(&self) -> usize {
            *self.writes.lock().unwrap()
        }

        fn refuse(&self, id: &str) {
            self.refused.lock().unwrap().insert(id.to_owned());
        }

        fn allow_only(&self, writes: usize) {
            *self.writes_left.lock().unwrap() = Some(writes);
        }

        fn spend_write(&self) -> bool {
            let mut left = self.writes_left.lock().unwrap();
            match left.as_mut() {
                None => true,
                Some(0) => false,
                Some(remaining) => {
                    *remaining -= 1;
                    true
                }
            }
        }
    }

    #[async_trait::async_trait]
    impl OverlayRepo for MemoryOverlays {
        async fn list(&self) -> Result<Vec<OverlayDefinition>, StorageError> {
            Ok(self.rows.lock().unwrap().values().cloned().collect())
        }

        async fn get(&self, id: &OverlayId) -> Result<Option<OverlayDefinition>, StorageError> {
            Ok(self.rows.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn get_by_credential(
            &self,
            _: &OverlayCredential,
        ) -> Result<Option<OverlayDefinition>, StorageError> {
            Ok(None)
        }

        async fn create(
            &self,
            _: &str,
            _: &str,
            _: u32,
        ) -> Result<OverlayDefinition, StorageError> {
            unreachable!("removing a color never creates an overlay")
        }

        async fn reissue_all_credentials(&self) -> Result<Vec<OverlayId>, StorageError> {
            unreachable!("removing a color never reissues credentials")
        }

        async fn save(&self, definition: &OverlayDefinition) -> Result<(), StorageError> {
            let refused = self
                .refused
                .lock()
                .unwrap()
                .contains(definition.id.as_str());
            if refused || !self.spend_write() {
                return Err(StorageError::Connection {
                    reason: "refused".to_owned(),
                });
            }
            *self.writes.lock().unwrap() += 1;
            self.rows
                .lock()
                .unwrap()
                .insert(definition.id.as_str().to_owned(), definition.clone());
            Ok(())
        }

        async fn restore(&self, _: &OverlayDefinition) -> Result<(), StorageError> {
            unreachable!("removing a color never restores an overlay")
        }

        async fn set_enabled(&self, _: &OverlayId, _: bool) -> Result<bool, StorageError> {
            unreachable!("removing a color never toggles an overlay")
        }

        async fn delete(&self, _: &OverlayId) -> Result<bool, StorageError> {
            unreachable!("removing a color never deletes an overlay")
        }

        async fn get_retained_content(
            &self,
            _: &OverlayId,
        ) -> Result<Option<OverlayConfig>, StorageError> {
            Ok(None)
        }

        async fn set_retained_content(
            &self,
            _: &OverlayId,
            _: &OverlayConfig,
        ) -> Result<(), StorageError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordedReloads(Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl OverlayFrameSink for RecordedReloads {
        async fn deliver_content(
            &self,
            _: &OverlayId,
            _: serde_json::Value,
            _: Option<u64>,
        ) -> OverlayReceivers {
            OverlayReceivers::default()
        }

        async fn deliver_reload(&self, identity: &OverlayId) {
            self.0.lock().unwrap().push(identity.as_str().to_owned());
        }

        async fn revoke(&self, _: &OverlayId) {}
    }

    struct Heard {
        committed: Vec<(String, Vec<OverlayId>)>,
        unknown: usize,
        _sub: Subscription,
    }

    struct Rig {
        removal: Entity<ColorRemoval>,
        accents: Entity<CustomAccents>,
        heard: Entity<Heard>,
        repo: Arc<MemoryOverlays>,
        settings: Arc<TestBackend>,
        reloads: Arc<RecordedReloads>,
        rt: Runtime,
        _root: TempDir,
    }

    fn settle(rt: &Runtime, cx: &mut TestAppContext) {
        for _ in 0..6 {
            pump(rt);
            cx.run_until_parked();
        }
    }

    fn rig(cx: &mut TestAppContext, rows: &[(&str, OverlayConfig)], listed: &[&str]) -> Rig {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        let rt = runtime();
        let root = TempDir::new().unwrap();
        let (settings, _writes) = test_backend();
        let repo = Arc::new(MemoryOverlays::default());
        for (id, stored) in rows {
            repo.rows
                .lock()
                .unwrap()
                .insert((*id).to_owned(), definition(id, KIND_ID, stored.clone()));
        }
        let reloads = Arc::new(RecordedReloads::default());
        let kinds = Arc::new(builtin_kinds());
        let bus = rt.block_on(async { EventBus::new(Arc::new(StubEventLog)) });
        let service = OverlayServiceHandle::new(OverlayServiceParts {
            default_root: PathBuf::from(root.path()),
            date_names: forge_runtime::DateNamesHandle::default(),
            tasks: forge_net::TaskOwner::new(),
            repo: Arc::clone(&repo) as Arc<dyn OverlayRepo>,
            settings: settings.settings_repo(),
            kinds: Arc::clone(&kinds),
            bus,
            frames: Arc::clone(&reloads) as Arc<dyn OverlayFrameSink>,
            media: None,
            speaker: None,
            latest: None,
        });
        let handle = rt.handle().clone();
        let settings_repo = settings.settings_repo();
        let accents = cx.new(|cx| CustomAccents::new(settings_repo, handle.clone(), cx));
        settle(&rt, cx);
        accents.update(cx, |store, cx| {
            for color in listed {
                store.add(color, cx);
            }
        });
        settle(&rt, cx);
        let launch = ColorRemovalLaunch {
            repo: Arc::clone(&repo) as Arc<dyn OverlayRepo>,
            settings: settings.settings_repo(),
            service,
            samples: Arc::new(NeutralOverlaySamples),
            kinds,
            rt_handle: handle,
            accents: accents.clone(),
        };
        let removal = cx.new(|_| ColorRemoval::new(launch));
        let heard = cx.new(|cx| Heard {
            committed: Vec::new(),
            unknown: 0,
            _sub: cx.subscribe(
                &removal,
                |this: &mut Heard, _removal, event, _cx| match event {
                    ColorRemovalEvent::Committed { color, resets } => {
                        let ids = resets.iter().map(|reset| reset.id.clone()).collect();
                        this.committed.push((color.clone(), ids));
                    }
                    ColorRemovalEvent::SavedStateUnknown => this.unknown += 1,
                },
            ),
        });
        Rig {
            removal,
            accents,
            heard,
            repo,
            settings,
            reloads,
            rt,
            _root: root,
        }
    }

    impl Rig {
        fn request(&self, open: Option<OpenConfig>, cx: &mut TestAppContext) {
            self.removal.update(cx, |removal, cx| {
                removal.request(CUSTOM.to_owned(), open, cx);
            });
            settle(&self.rt, cx);
        }

        fn confirm(&self, cx: &mut TestAppContext) {
            self.removal.update(cx, |removal, cx| removal.confirm(cx));
            for _ in 0..MAX_SETTLE_TURNS {
                settle(&self.rt, cx);
                if self.answered(cx) {
                    return;
                }
                std::thread::yield_now();
            }
            panic!("the removal never answered");
        }

        fn answered(&self, cx: &mut TestAppContext) -> bool {
            let unknown = self.heard.read_with(cx, |heard, _| heard.unknown);
            self.committed(cx) + unknown + cx.update(|cx| error_toasts(cx)) > 0
        }

        fn counts(&self, cx: &TestAppContext) -> Option<(usize, usize)> {
            self.removal.read_with(cx, |removal, _| {
                removal
                    .pending
                    .get()
                    .map(|prompt| (prompt.overlays, prompt.fields))
            })
        }

        fn listed(&self, cx: &TestAppContext) -> Vec<String> {
            self.accents
                .read_with(cx, |store, _| store.colors().to_vec())
        }

        fn stored_list(&self) -> Option<Vec<String>> {
            let repo = self.settings.settings_repo();
            self.rt
                .block_on(read_stored_setting(repo.as_ref(), &OVERLAY_CUSTOM_ACCENTS))
                .unwrap()
        }

        fn committed(&self, cx: &TestAppContext) -> usize {
            self.heard.read_with(cx, |heard, _| heard.committed.len())
        }

        fn reloads(&self) -> Vec<String> {
            self.reloads.0.lock().unwrap().clone()
        }
    }

    fn two_users() -> [(&'static str, OverlayConfig); 3] {
        [
            (
                FIRST,
                config(&[(ACCENT, CUSTOM), (BACKGROUND_COLOR, CUSTOM)]),
            ),
            (SECOND, config(&[(ACCENT, CUSTOM), (HEADLINE, "kept")])),
            (UNTOUCHED, config(&[(ACCENT, OTHER)])),
        ]
    }

    fn list(colors: &[&str]) -> Vec<String> {
        colors.iter().map(|color| (*color).to_owned()).collect()
    }

    #[test]
    fn the_count_takes_every_overlay_and_color_field_holding_the_color() {
        let rows = [
            definition(
                FIRST,
                KIND_ID,
                config(&[(ACCENT, CUSTOM), (BACKGROUND_COLOR, CUSTOM)]),
            ),
            definition(SECOND, KIND_ID, config(&[(BACKGROUND_COLOR, CUSTOM)])),
            definition(
                UNTOUCHED,
                KIND_ID,
                config(&[(ACCENT, OTHER), (HEADLINE, CUSTOM)]),
            ),
        ];

        let resets = color_resets(&rows, None, CUSTOM, &builtin_kinds());

        let counted: Vec<(&str, usize)> = resets
            .iter()
            .map(|reset| (reset.id.as_str(), reset.fields))
            .collect();
        assert_eq!(counted, vec![(FIRST, 2), (SECOND, 1)]);
    }

    #[test]
    fn the_count_reads_a_stored_color_in_any_letter_case() {
        let rows = [definition(FIRST, KIND_ID, config(&[(ACCENT, "#ABCDEF")]))];

        let resets = color_resets(&rows, None, "#abcdef", &builtin_kinds());

        assert_eq!(resets.len(), 1);
    }

    #[test]
    fn the_count_skips_a_look_this_build_lacks_because_it_cannot_be_saved() {
        let rows = [definition(
            FIRST,
            VANISHED_KIND,
            config(&[(ACCENT, CUSTOM)]),
        )];

        assert!(color_resets(&rows, None, CUSTOM, &builtin_kinds()).is_empty());
    }

    #[test]
    fn the_open_overlay_counts_by_what_its_panel_shows_and_restores_what_is_stored() {
        let stored = config(&[(HEADLINE, "stored")]);
        let shown = config(&[(HEADLINE, "typed"), (ACCENT, CUSTOM)]);
        let rows = [definition(FIRST, KIND_ID, stored.clone())];
        let open = OpenConfig {
            id: OverlayId::new(FIRST),
            config: shown,
        };

        let resets = color_resets(&rows, Some(&open), CUSTOM, &builtin_kinds());

        assert_eq!(resets.len(), 1);
        assert_eq!(resets[0].before, stored);
        assert_eq!(resets[0].after, config(&[(HEADLINE, "typed")]));
    }

    #[gpui::test]
    fn a_color_in_use_prompts_with_its_counts_and_writes_nothing(cx: &mut TestAppContext) {
        let rig = rig(cx, &two_users(), &[CUSTOM]);

        rig.request(None, cx);

        assert_eq!(rig.counts(cx), Some((2, 3)));
        assert_eq!(rig.repo.writes(), 0);
        assert_eq!(rig.listed(cx), list(&[CUSTOM]));
    }

    #[gpui::test]
    fn cancel_leaves_every_overlay_and_the_list_alone(cx: &mut TestAppContext) {
        let rig = rig(cx, &two_users(), &[CUSTOM]);
        rig.request(None, cx);

        rig.removal.update(cx, |removal, cx| removal.cancel(cx));
        settle(&rig.rt, cx);

        assert_eq!(rig.counts(cx), None);
        assert_eq!(rig.repo.writes(), 0);
        assert_eq!(rig.listed(cx), list(&[CUSTOM]));
        assert_eq!(rig.stored_list(), Some(list(&[CUSTOM])));
        assert!(rig.reloads().is_empty());
    }

    #[gpui::test]
    fn an_unused_color_goes_at_once_without_a_prompt(cx: &mut TestAppContext) {
        let rig = rig(
            cx,
            &[(UNTOUCHED, config(&[(ACCENT, OTHER)]))],
            &[CUSTOM, OTHER],
        );

        rig.request(None, cx);

        assert_eq!(rig.counts(cx), None);
        assert_eq!(rig.listed(cx), list(&[OTHER]));
        assert_eq!(rig.stored_list(), Some(list(&[OTHER])));
        assert_eq!(rig.repo.writes(), 0);
    }

    #[gpui::test]
    fn confirm_resets_every_using_overlay_reloads_its_page_and_drops_the_color(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx, &two_users(), &[CUSTOM, OTHER]);
        rig.request(None, cx);

        rig.confirm(cx);

        assert_eq!(rig.repo.stored(FIRST), OverlayConfig::new());
        assert_eq!(rig.repo.stored(SECOND), config(&[(HEADLINE, "kept")]));
        assert_eq!(rig.repo.stored(UNTOUCHED), config(&[(ACCENT, OTHER)]));
        assert_eq!(rig.listed(cx), list(&[OTHER]));
        assert_eq!(rig.stored_list(), Some(list(&[OTHER])));
        assert_eq!(rig.reloads(), list(&[FIRST, SECOND]));
        assert_eq!(rig.counts(cx), None);
        rig.heard.read_with(cx, |heard, _| {
            assert_eq!(
                heard.committed,
                vec![(
                    CUSTOM.to_owned(),
                    vec![OverlayId::new(FIRST), OverlayId::new(SECOND)]
                )]
            );
        });
        assert_eq!(cx.update(|cx| error_toasts(cx)), 0);
    }

    #[gpui::test]
    fn confirm_saves_the_open_overlays_typed_changes_with_the_reset(cx: &mut TestAppContext) {
        let rig = rig(cx, &[(FIRST, config(&[(ACCENT, CUSTOM)]))], &[CUSTOM]);
        rig.request(
            Some(OpenConfig {
                id: OverlayId::new(FIRST),
                config: config(&[(ACCENT, CUSTOM), (HEADLINE, "typed")]),
            }),
            cx,
        );

        rig.confirm(cx);

        assert_eq!(rig.repo.stored(FIRST), config(&[(HEADLINE, "typed")]));
    }

    #[gpui::test]
    fn a_refused_overlay_undoes_the_ones_already_reset_and_keeps_the_list(cx: &mut TestAppContext) {
        let rows = two_users();
        let rig = rig(cx, &rows, &[CUSTOM]);
        rig.request(None, cx);
        rig.repo.refuse(SECOND);

        rig.confirm(cx);

        assert_eq!(rig.repo.stored(FIRST), rows[0].1);
        assert_eq!(rig.repo.stored(SECOND), rows[1].1);
        assert_eq!(rig.listed(cx), list(&[CUSTOM]));
        assert_eq!(rig.stored_list(), Some(list(&[CUSTOM])));
        assert_eq!(rig.committed(cx), 0);
        assert!(rig.reloads().is_empty());
        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
    }

    #[gpui::test]
    fn a_refused_list_write_undoes_every_overlay(cx: &mut TestAppContext) {
        let rows = two_users();
        let rig = rig(cx, &rows, &[CUSTOM]);
        rig.request(None, cx);
        rig.settings.fail_writes(OVERLAY_CUSTOM_ACCENTS.name());

        rig.confirm(cx);

        assert_eq!(rig.repo.stored(FIRST), rows[0].1);
        assert_eq!(rig.repo.stored(SECOND), rows[1].1);
        assert_eq!(rig.listed(cx), list(&[CUSTOM]));
        assert_eq!(rig.committed(cx), 0);
        assert!(rig.reloads().is_empty());
        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
    }

    #[gpui::test]
    fn an_undo_that_fails_reports_it_and_asks_the_screen_to_reread(cx: &mut TestAppContext) {
        let rig = rig(cx, &two_users(), &[CUSTOM]);
        rig.request(None, cx);
        rig.settings.fail_writes(OVERLAY_CUSTOM_ACCENTS.name());
        rig.repo.allow_only(2);

        rig.confirm(cx);

        assert_eq!(rig.listed(cx), list(&[CUSTOM]));
        assert_eq!(rig.committed(cx), 0);
        assert_eq!(rig.heard.read_with(cx, |heard, _| heard.unknown), 1);
        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
    }
}
