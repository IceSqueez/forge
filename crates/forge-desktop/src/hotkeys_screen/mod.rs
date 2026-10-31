use forge_components::FONT_LABEL;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BreadcrumbCrumb, Icon, ToastKind, body_family, icon, page_frame, scaled, tr,
};
use forge_events::Event;
use forge_hotkey::HotkeyClient;
use forge_runtime::EventBus;
use forge_storage::{DataProvider, SettingsRepo, StorageError};
use forge_types::TriggerInstanceId;
use gpui::{Context, EventEmitter, Pixels, Point, Task, Window, div, prelude::*, px};
use time::OffsetDateTime;

use crate::actions::SHORTCUTS;
use crate::async_bridge::{self, BridgeFlow, Loadable, SnapshotWriter, WriteOutcome, drain_events};
use crate::combo_capture::ComboCapture;
use crate::combo_conflict::{ClipKey, clip_keys, combo_holder};
use crate::hub_crumb::hub_crumb;
use crate::integrations::BuiltinObject;
use crate::navigation::NavRequested;
use crate::repo_failure::RepoFailure;
use crate::runtime_event::{self, RuntimeEvent};
use crate::shortcut_overrides::{ShortcutOverrides, load_overrides, save_overrides};
use forge_components::ActivePresentation;
use forge_components::PushToast;
use forge_hotkey::HotkeyReconciler;
use forge_hotkey::{BindingRow, HotkeyEdge, conflict_count, load_bindings, registered_combos};
use forge_runtime::IntegrationSlot;

mod action_modal;
mod app_row;
mod app_shortcuts;
mod binding_row;
mod binding_writes;
mod capture;
mod conflict;
mod delete;
mod footer;
mod hero;
mod list;
mod row_style;
mod stats;

use action_modal::OpenModal;
use app_shortcuts::OpenAppModal;
use capture::Capture;
use conflict::ConflictPrompt;
use delete::DeletePrompt;
use list::RowKey;

const RELATIVE_TIME_TICK: Duration = Duration::from_secs(30);
const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);

const LABEL_FS: Pixels = FONT_LABEL;

const HEADER_GAP: Pixels = px(5.0);
const HEADER_GLYPH: Pixels = px(13.0);

struct LastFired {
    combo: String,
    at: OffsetDateTime,
}

pub(crate) struct HotkeysScreenView {
    client: Arc<HotkeyClient>,
    reconciler: Arc<HotkeyReconciler>,
    engine: Option<IntegrationSlot<BuiltinObject>>,
    backend: Arc<dyn DataProvider>,
    settings_repo: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    enabled: bool,
    bindings: Vec<BindingRow>,
    clip_keys: Vec<ClipKey>,
    shortcuts: Loadable<ShortcutOverrides>,
    shortcut_writer: SnapshotWriter,
    conflicts: usize,
    last_fired: Option<LastFired>,
    last_synthesized: Option<OffsetDateTime>,
    capture: Capture,
    key_capture: ComboCapture,
    conflict: Option<ConflictPrompt>,
    delete_prompt: Option<DeletePrompt>,
    modal: Option<OpenModal>,
    app_modal: Option<OpenAppModal>,
    menu_open: Option<RowKey>,
    menu_click_pos: Option<Point<Pixels>>,
    _bus_bridge: Task<()>,
    _relative_time_tick: Task<()>,
}

impl HotkeysScreenView {
    pub(crate) fn new(
        reconciler: Arc<HotkeyReconciler>,
        engine: Option<IntegrationSlot<BuiltinObject>>,
        backend: Arc<dyn DataProvider>,
        settings_repo: Arc<dyn SettingsRepo>,
        bus: Arc<EventBus>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let bus_bridge = Self::spawn_bus_bridge(bus, cx);
        let client = Arc::clone(reconciler.client());
        let mut view = Self {
            enabled: client.is_enabled(),
            conflicts: conflict_count(&client),
            client,
            reconciler,
            engine,
            backend,
            settings_repo,
            shortcut_writer: SnapshotWriter::new(&rt_handle),
            rt_handle,
            bindings: Vec::new(),
            clip_keys: Vec::new(),
            shortcuts: Loadable::Loading,
            last_fired: None,
            last_synthesized: None,
            capture: Capture::Off,
            key_capture: ComboCapture::default(),
            conflict: None,
            delete_prompt: None,
            modal: None,
            app_modal: None,
            menu_open: None,
            menu_click_pos: None,
            _bus_bridge: bus_bridge,
            _relative_time_tick: Self::spawn_relative_time_tick(cx),
        };
        view.load(cx);
        view.load_shortcuts(cx);
        view
    }

    fn spawn_relative_time_tick(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(RELATIVE_TIME_TICK).await;
                let repainted = this.update(cx, |this, cx| {
                    if this.last_fired.is_some() {
                        cx.notify();
                    }
                });
                if repainted.is_err() {
                    break;
                }
            }
        })
    }

    fn spawn_bus_bridge(bus: Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                if !batch.iter().any(runtime_event::is_hotkey_event) {
                    return BridgeFlow::Continue;
                }
                match this.update(cx, |this, cx| this.on_hotkey_events(batch, cx)) {
                    Ok(()) => BridgeFlow::Continue,
                    Err(_) => BridgeFlow::Stop,
                }
            })
            .await;
        })
    }

    fn on_hotkey_events(&mut self, batch: &[Event], cx: &mut Context<Self>) {
        for event in batch {
            match runtime_event::decode(event) {
                Some(RuntimeEvent::HotkeyPressed(pressed)) => {
                    self.last_fired = Some(LastFired {
                        combo: pressed.combo,
                        at: event.timestamp,
                    });
                }
                Some(RuntimeEvent::HotkeyEnableFailed(failed)) if !failed.combos.is_empty() => {
                    cx.push_toast(
                        ToastKind::Warn,
                        tr!(
                            "hotkeys_toast_enable_partial",
                            count = i64::try_from(failed.combos.len()).unwrap_or(i64::MAX)
                        ),
                    );
                }
                _ => {}
            }
        }
        self.refresh_from_client(cx);
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let backend = Arc::clone(&self.backend);
        let registered = registered_combos(&self.client);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let clips = backend
                    .soundboard_clips_repo()
                    .list()
                    .await
                    .map_err(RepoFailure::from)?;
                let rows = load_bindings(backend, registered)
                    .await
                    .map_err(|error| RepoFailure::hotkey(&error))?;
                Ok((rows, clip_keys(&clips)))
            },
            |this, result, cx| this.apply_bindings(result, cx),
            cx,
        );
    }

    fn apply_bindings(
        &mut self,
        result: Result<(Vec<BindingRow>, Vec<ClipKey>), RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((rows, clips)) => {
                self.bindings = rows;
                self.clip_keys = clips;
                self.refresh_from_client(cx);
            }
            Err(failure) => self.on_repo_error(&failure, cx),
        }
    }

    fn load_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcuts = Loadable::Loading;
        let repo = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            load_overrides(repo),
            |this, result, cx| this.apply_shortcuts(result, cx),
            cx,
        );
        cx.notify();
    }

    fn rollback_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcuts = Loadable::Loading;
        let repo = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            load_overrides(repo),
            |this, result, cx| {
                this.apply_shortcuts(result, cx);
                if let Some(shortcuts) = this.shortcuts.loaded() {
                    shortcuts.apply(cx);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn apply_shortcuts(
        &mut self,
        result: Result<ShortcutOverrides, StorageError>,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = &result {
            tracing::warn!(error = %e, "failed to load keyboard shortcuts");
        }
        self.shortcuts = Loadable::settled(result);
        cx.notify();
    }

    fn persist_shortcuts(&mut self, cx: &mut Context<Self>) {
        let Some(shortcuts) = self.shortcuts.loaded() else {
            return;
        };
        shortcuts.apply(cx);
        let repo = Arc::clone(&self.settings_repo);
        let map = shortcuts.snapshot();
        self.shortcut_writer
            .write(save_overrides(repo, map), Self::on_shortcuts_written, cx);
        cx.notify();
    }

    fn on_shortcuts_written(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        if let WriteOutcome::Failed(reason) = outcome {
            self.on_repo_error(&reason, cx);
            self.rollback_shortcuts(cx);
        }
    }

    fn refresh_from_client(&mut self, cx: &mut Context<Self>) {
        let registered = registered_combos(&self.client);
        for row in &mut self.bindings {
            row.registered = registered.iter().any(|(_, combo)| combo == &row.combo);
        }
        self.conflicts = conflict_count(&self.client);
        self.enabled = self.client.is_enabled();
        self.last_synthesized = self.client.last_synthesized_release();
        cx.notify();
    }

    fn on_repo_error(&mut self, failure: &RepoFailure, cx: &mut Context<Self>) {
        tracing::warn!(error = %failure, "hotkey binding operation failed");
        let message = failure.text();
        cx.push_toast(
            ToastKind::Error,
            tr!("hotkeys_toast_error", message = message.as_str()),
        );
        cx.notify();
    }

    fn enabled_count(&self) -> usize {
        self.bindings.iter().filter(|row| row.enabled()).count()
    }

    fn row_by_key(&self, key: TriggerInstanceId) -> Option<&BindingRow> {
        self.bindings.iter().find(|row| row.key == key)
    }

    fn row_of_instance(&self, instance_id: TriggerInstanceId) -> Option<&BindingRow> {
        self.bindings.iter().find(|row| {
            row.halves()
                .any(|(_, half)| half.instance_id == instance_id)
        })
    }

    fn locked_edge_for(&self, combo: &str) -> Option<HotkeyEdge> {
        combo_holder(&self.bindings, combo, None)
            .and_then(|row| row.free_edge())
            .map(HotkeyEdge::opposite)
    }

    fn global_count(&self) -> usize {
        self.bindings.iter().filter(|row| row.registered).count()
    }

    fn total_count(&self) -> usize {
        self.bindings.len() + SHORTCUTS.len()
    }

    fn active_count(&self) -> usize {
        self.enabled_count()
            + self
                .shortcuts
                .loaded()
                .map_or(0, ShortcutOverrides::bound_count)
    }
}

impl EventEmitter<NavRequested> for HotkeysScreenView {}

impl Render for HotkeysScreenView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_right = div()
            .flex()
            .items_center()
            .gap(scaled(HEADER_GAP, density))
            .child(icon(Icon::Keyboard, HEADER_GLYPH, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(LABEL_FS)
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "hotkeys_header_summary",
                        count = i64::try_from(self.active_count()).unwrap_or(i64::MAX)
                    )),
            );

        let body = div()
            .id("hotkeys-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(SCROLL_PAD_Y, density))
                    .px(scaled(SCROLL_PAD_X, density))
                    .flex()
                    .flex_col()
                    .child(self.render_hero(&palette, cx))
                    .child(self.render_stats(&palette, density))
                    .child(self.render_bindings(&palette, cx))
                    .child(self.render_footer(&palette, density)),
            );

        let frame = page_frame(
            vec![
                hub_crumb(Some(forge_hotkey::HOTKEY_INTEGRATION.category), cx),
                BreadcrumbCrumb::leaf(tr!("hotkeys_hero_title")),
            ],
            &palette,
        )
        .header_right(header_right)
        .density(density)
        .body(body);

        let prompt = if let Some(conflict) = &self.conflict {
            Some(self.render_conflict(conflict, &palette, cx))
        } else {
            self.delete_prompt
                .as_ref()
                .map(|prompt| self.render_delete_confirm(prompt, &palette, cx))
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(self.modal.as_ref().map(|open| open.view.clone()))
            .children(self.app_modal.as_ref().map(|open| open.view.clone()))
            .children(prompt)
    }
}

#[cfg(test)]
mod tests {
    use forge_hotkey::HotkeyConfig;
    use gpui::{Entity, TestAppContext};

    use super::*;
    use crate::navigation::Screen;
    use crate::shortcut_overrides::KEYBOARD_SHORTCUTS as SHORTCUTS_SETTING;
    use crate::test_support::{
        ReadFault, SETTINGS_READ_FAILED, Sandboxed, SettingWrite, TestBackend, hub_crumb_targets,
        install_presentation, pump, quiet_bus, runtime, sandboxed_provider, test_backend,
        writes_to,
    };
    use capture::Capture;
    use forge_components::Toasts;

    const KEYBOARD_SHORTCUTS: &str = SHORTCUTS_SETTING.name();
    use tokio::sync::mpsc::UnboundedReceiver;

    #[gpui::test]
    fn the_crumb_leads_back_to_the_hotkeys_category_on_the_hub(cx: &mut TestAppContext) {
        let rt = runtime();
        let bus = quiet_bus(&rt);
        let storage = rt.block_on(sandboxed_provider());
        let (settings, _writes) = test_backend();
        let reconciler = {
            let _entered = rt.enter();
            let (client, _backend) = forge_hotkey::testing::test_client(
                HotkeyConfig::default(),
                Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>,
            );
            HotkeyReconciler::new(
                client,
                storage.trigger_instance_repo(),
                storage.soundboard_clips_repo(),
            )
        };
        let backend = Arc::clone(&storage);
        let handle = rt.handle().clone();

        let targets = hub_crumb_targets(cx, |_, cx| {
            HotkeysScreenView::new(
                reconciler,
                None,
                backend,
                settings as Arc<dyn SettingsRepo>,
                bus,
                handle,
                cx,
            )
        });

        let category = forge_hotkey::HOTKEY_INTEGRATION.category;
        assert!(!targets.is_empty(), "no click reached the crumb");
        assert!(
            targets
                .iter()
                .all(|screen| *screen == Screen::Integrations(Some(category))),
            "{targets:?}"
        );
    }

    const HOME: &str = "nav.home";
    const HOME_DEFAULT: &str = "ctrl-1";
    const CHAT: &str = "nav.chat";
    const STORED: &str = r#"{"nav.chat":{"chord":"ctrl-8"}}"#;
    const SETTLE_ROUNDS: usize = 3;

    struct Harness {
        rt: tokio::runtime::Runtime,
        settings: Arc<TestBackend>,
        writes: UnboundedReceiver<SettingWrite>,
        storage: Sandboxed<Arc<dyn DataProvider>>,
    }

    impl Harness {
        fn new(stored: &str) -> Self {
            let rt = runtime();
            let storage = rt.block_on(sandboxed_provider());
            let (settings, writes) = test_backend();
            settings.seed_raw(&SHORTCUTS_SETTING, stored);
            Self {
                rt,
                settings,
                writes,
                storage,
            }
        }

        fn open(&self, cx: &mut TestAppContext) -> Entity<HotkeysScreenView> {
            install_presentation(cx);
            cx.update(|cx| cx.set_global(Toasts::new()));
            let bus = quiet_bus(&self.rt);
            let reconciler = {
                let _entered = self.rt.enter();
                let (client, _backend) = forge_hotkey::testing::test_client(
                    HotkeyConfig::default(),
                    Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>,
                );
                HotkeyReconciler::new(
                    client,
                    self.storage.trigger_instance_repo(),
                    self.storage.soundboard_clips_repo(),
                )
            };
            let backend = Arc::clone(&*self.storage);
            let settings = Arc::clone(&self.settings) as Arc<dyn SettingsRepo>;
            let handle = self.rt.handle().clone();
            let view = cx.update(|cx| {
                cx.new(|cx| {
                    HotkeysScreenView::new(reconciler, None, backend, settings, bus, handle, cx)
                })
            });
            self.settle(cx);
            view
        }

        fn settle(&self, cx: &mut TestAppContext) {
            for _ in 0..SETTLE_ROUNDS {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }

        fn shortcut_writes(&mut self) -> Vec<String> {
            writes_to(&mut self.writes, KEYBOARD_SHORTCUTS)
        }
    }

    fn shown_chord(
        view: &Entity<HotkeysScreenView>,
        id: &str,
        cx: &mut TestAppContext,
    ) -> Option<String> {
        let entry = crate::actions::shortcut_entry(id).unwrap();
        view.read_with(cx, |v, _| {
            v.shortcuts
                .loaded()
                .and_then(|shortcuts| shortcuts.chord_of(entry).map(str::to_owned))
        })
    }

    type Edit = fn(&mut HotkeysScreenView, &mut Context<HotkeysScreenView>);

    fn edit_paths() -> [(&'static str, Edit); 4] {
        [
            ("reset", |v, cx| v.reset_app_binding(CHAT, cx)),
            ("unbind", |v, cx| v.unbind_app_binding(HOME, cx)),
            ("toggle", |v, cx| v.toggle_app_binding(HOME, cx)),
            ("captured chord", |v, cx| {
                v.apply_capture(Capture::App(HOME), "ctrl-9".to_owned(), cx)
            }),
        ]
    }

    #[gpui::test]
    fn no_app_shortcut_edit_after_a_failed_load_writes_over_the_stored_shortcuts(
        cx: &mut TestAppContext,
    ) {
        let unreachable = crate::test_support::shown_in_english(StorageError::Connection {
            reason: SETTINGS_READ_FAILED.to_owned(),
        });
        let undecodable = crate::test_support::shown_in_english(StorageError::UndecodableSetting {
            key: KEYBOARD_SHORTCUTS.to_owned(),
        });
        for (fault, stored, failing_read, reason) in [
            ("unreachable store", STORED, true, unreachable),
            ("undecodable value", "not json", false, undecodable),
        ] {
            for (case, edit) in edit_paths() {
                let mut harness = Harness::new(stored);
                if failing_read {
                    harness
                        .settings
                        .fault_reads(KEYBOARD_SHORTCUTS, ReadFault::Fails);
                }
                let view = harness.open(cx);

                view.update(cx, edit);
                harness.settle(cx);

                let failure = view.read_with(cx, |v, _| v.shortcuts.failure());
                assert_eq!(
                    (harness.shortcut_writes(), failure),
                    (Vec::<String>::new(), Some(reason.clone())),
                    "{fault}: {case}"
                );
            }
        }
    }

    #[gpui::test]
    fn no_app_shortcut_edit_while_the_load_is_pending_writes_and_the_load_lands_intact(
        cx: &mut TestAppContext,
    ) {
        for (case, edit) in edit_paths() {
            let mut harness = Harness::new(STORED);
            let gate = Arc::new(tokio::sync::Notify::new());
            harness
                .settings
                .fault_reads(KEYBOARD_SHORTCUTS, ReadFault::Held(Arc::clone(&gate)));
            let view = harness.open(cx);

            view.update(cx, edit);
            harness.settle(cx);
            let writes_while_pending = harness.shortcut_writes();
            harness.settings.heal_reads(KEYBOARD_SHORTCUTS);
            gate.notify_one();
            harness.settle(cx);

            assert_eq!(
                (writes_while_pending, shown_chord(&view, CHAT, cx)),
                (Vec::<String>::new(), Some("ctrl-8".to_owned())),
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn an_app_shortcut_cannot_be_opened_or_captured_until_the_shortcuts_load(
        cx: &mut TestAppContext,
    ) {
        let entry = crate::actions::shortcut_entry(HOME).unwrap();
        for (state, fault) in [
            ("failed", ReadFault::Fails),
            (
                "pending",
                ReadFault::Held(Arc::new(tokio::sync::Notify::new())),
            ),
        ] {
            let harness = Harness::new(STORED);
            harness.settings.fault_reads(KEYBOARD_SHORTCUTS, fault);
            let view = harness.open(cx);

            view.update(cx, |v, cx| {
                v.edit_app_binding(entry, cx);
                v.start_capture(Capture::App(HOME), cx);
            });

            let refused = view.read_with(cx, |v, _| {
                (v.app_modal.is_none(), matches!(v.capture, Capture::Off))
            });
            assert_eq!(refused, (true, true), "{state}");
        }
    }

    #[gpui::test]
    fn retrying_a_failed_app_shortcut_load_restores_them_and_edits_persist_again(
        cx: &mut TestAppContext,
    ) {
        let mut harness = Harness::new(STORED);
        harness
            .settings
            .fault_reads(KEYBOARD_SHORTCUTS, ReadFault::Fails);
        let view = harness.open(cx);
        harness.settings.heal_reads(KEYBOARD_SHORTCUTS);

        view.update(cx, |v, cx| v.load_shortcuts(cx));
        harness.settle(cx);
        view.update(cx, |v, cx| v.unbind_app_binding(HOME, cx));
        harness.settle(cx);

        let written: Vec<(Option<String>, Option<String>)> = harness
            .shortcut_writes()
            .iter()
            .map(|raw| {
                let restored = crate::test_support::stored_shortcut_overrides(Some(raw)).unwrap();
                let chord_of = |id| {
                    restored
                        .chord_of(crate::actions::shortcut_entry(id).unwrap())
                        .map(str::to_owned)
                };
                (chord_of(HOME), chord_of(CHAT))
            })
            .collect();
        assert_eq!(written, vec![(None, Some("ctrl-8".to_owned()))]);
    }

    #[gpui::test]
    fn a_failed_app_shortcut_write_toasts_and_rolls_back_to_the_stored_shortcuts(
        cx: &mut TestAppContext,
    ) {
        let harness = Harness::new(STORED);
        harness.settings.fail_writes(KEYBOARD_SHORTCUTS);
        let view = harness.open(cx);

        view.update(cx, |v, cx| v.unbind_app_binding(HOME, cx));
        harness.settle(cx);

        let toasted = cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| toast.kind)
                .collect::<Vec<_>>()
        });
        assert_eq!(
            (toasted, shown_chord(&view, HOME, cx)),
            (vec![ToastKind::Error], Some(HOME_DEFAULT.to_owned()))
        );
    }
}
