use std::sync::Arc;

use forge_components::{BreadcrumbCrumb, Icon, ToastKind, body_family, icon, page_frame, tr};
use forge_events::Event;
use forge_hotkey::HotkeyClient;
use forge_runtime::EventBus;
use forge_storage::settings::reserved_keys::KEYBOARD_SHORTCUTS;
use forge_storage::{DataProvider, SettingsRepo, StorageError};
use forge_types::TriggerInstanceId;
use gpui::{Context, EventEmitter, Pixels, Point, Task, Window, div, prelude::*, px};
use time::OffsetDateTime;

use crate::actions::SHORTCUTS;
use crate::async_bridge::{self, BridgeFlow, drain_events};
use crate::combo_capture::ComboCapture;
use crate::combo_conflict::{ClipKey, clip_keys, combo_holder};
use crate::hotkey_bindings::{
    BindingRow, HOTKEY_EVENT_PREFIX, HOTKEY_PRESSED_KIND, HotkeyEdge, conflict_count,
    load_bindings, registered_combos,
};
use crate::hotkey_sync::HotkeyReconciler;
use crate::hub_crumb::hub_crumb;
use crate::integration_supervisor::IntegrationSlot;
use crate::presentation::ActivePresentation;
use crate::shortcut_overrides::{ShortcutOverrides, save_overrides};
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;

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

const ENABLE_FAILED_KIND: &str = "hotkey.engine.enable_failed";
const COMBO_FIELD: &str = "combo";
const COMBOS_FIELD: &str = "combos";

const SCROLL_PAD_X: Pixels = px(22.0);
const SCROLL_PAD_Y: Pixels = px(18.0);

const LABEL_FS: Pixels = px(11.5);

const HEADER_GAP: Pixels = px(5.0);
const HEADER_GLYPH: Pixels = px(13.0);

struct LastFired {
    combo: String,
    at: OffsetDateTime,
}

pub struct HotkeysScreenView {
    client: Arc<HotkeyClient>,
    reconciler: Arc<HotkeyReconciler>,
    engine: Option<IntegrationSlot>,
    backend: Arc<dyn DataProvider>,
    settings_repo: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    enabled: bool,
    bindings: Vec<BindingRow>,
    clip_keys: Vec<ClipKey>,
    shortcuts: ShortcutOverrides,
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
}

impl HotkeysScreenView {
    pub fn new(
        reconciler: Arc<HotkeyReconciler>,
        engine: Option<IntegrationSlot>,
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
            rt_handle,
            bindings: Vec::new(),
            clip_keys: Vec::new(),
            shortcuts: ShortcutOverrides::default(),
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
        };
        view.load(cx);
        view.load_shortcuts(cx);
        view
    }

    fn spawn_bus_bridge(bus: Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            drain_events(&bus, cx, move |batch, cx| {
                if !batch
                    .iter()
                    .any(|event| event.kind.starts_with(HOTKEY_EVENT_PREFIX))
                {
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
            match event.kind.as_str() {
                HOTKEY_PRESSED_KIND => {
                    if let Some(combo) = event.payload.get(COMBO_FIELD).and_then(|v| v.as_str()) {
                        self.last_fired = Some(LastFired {
                            combo: combo.to_owned(),
                            at: event.timestamp,
                        });
                    }
                }
                ENABLE_FAILED_KIND => {
                    let failed = event
                        .payload
                        .get(COMBOS_FIELD)
                        .and_then(|v| v.as_array())
                        .map(|combos| combos.len())
                        .unwrap_or(0);
                    if failed > 0 {
                        cx.push_toast(
                            ToastKind::Warn,
                            tr!("hotkeys_toast_enable_partial", count = failed as i64),
                        );
                    }
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
                    .map_err(|e| e.to_string())?;
                let rows = load_bindings(backend, registered).await?;
                Ok((rows, clip_keys(&clips)))
            },
            |this, result, cx| this.apply_bindings(result, cx),
            cx,
        );
    }

    fn apply_bindings(
        &mut self,
        result: Result<(Vec<BindingRow>, Vec<ClipKey>), String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((rows, clips)) => {
                self.bindings = rows;
                self.clip_keys = clips;
                self.refresh_from_client(cx);
            }
            Err(message) => self.on_repo_error(&message, cx),
        }
    }

    fn load_shortcuts(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.get_string(KEYBOARD_SHORTCUTS).await },
            |this, result, cx| this.apply_shortcuts(result, cx),
            cx,
        );
    }

    fn apply_shortcuts(
        &mut self,
        result: Result<Option<String>, StorageError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(raw) => self.shortcuts.replace_stored(raw.as_deref()),
            Err(e) => self.on_repo_error(&e.to_string(), cx),
        }
        cx.notify();
    }

    fn persist_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcuts.apply(cx);
        let repo = Arc::clone(&self.settings_repo);
        let map = self.shortcuts.snapshot();
        async_bridge::run_async(
            &self.rt_handle,
            save_overrides(repo, map),
            |this, result: Result<(), String>, cx| match result {
                Ok(()) => cx.notify(),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
        cx.notify();
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

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        tracing::warn!(error = %message, "hotkey binding operation failed");
        cx.push_toast(
            ToastKind::Error,
            tr!("hotkeys_toast_error", message = message),
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
        self.enabled_count() + self.shortcuts.bound_count()
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
            .gap(HEADER_GAP)
            .child(icon(Icon::Keyboard, HEADER_GLYPH, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(LABEL_FS)
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "hotkeys_header_summary",
                        count = self.active_count() as i64
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
                    .py(SCROLL_PAD_Y)
                    .px(SCROLL_PAD_X)
                    .flex()
                    .flex_col()
                    .child(self.render_hero(&palette, cx))
                    .child(self.render_stats(&palette))
                    .child(self.render_bindings(&palette, cx))
                    .child(self.render_footer(&palette)),
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
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use forge_hotkey::HotkeyConfig;
    use gpui::TestAppContext;

    use super::*;
    use crate::screen::Screen;
    use crate::test_support::{
        hub_crumb_targets, quiet_bus, runtime, sandboxed_provider, test_backend,
    };

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
}
