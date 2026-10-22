use std::sync::Arc;

use forge_components::{
    Density, FONT_SM, ForgePalette, InputEvent, SettingRow, Spacing, TextInput, primary_button,
    secondary_button, segment, segmented, setting_groups, setting_row, setting_rows,
    setting_value_row, spacing, tr,
};
use forge_runtime::ChatHistoryRetentionHandle;
use forge_storage::{
    DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT, DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
    DEFAULT_EVENT_LOG_RETENTION_DAYS, DataProvider, MAX_ACTION_HISTORY_RETENTION_DAYS,
    MAX_CHAT_HISTORY_PER_VIEWER_LIMIT, MAX_EVENT_LOG_RETENTION_DAYS,
    MIN_ACTION_HISTORY_RETENTION_DAYS, MIN_CHAT_HISTORY_PER_VIEWER_LIMIT,
    MIN_EVENT_LOG_RETENTION_DAYS, SettingsRepo, StorageError,
    UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT, action_history_retention_days,
    chat_history_display_limit, chat_history_per_viewer_limit, clamp_action_history_retention_days,
    event_log_retention_days, set_action_history_retention_days, set_chat_history_display_limit,
    set_chat_history_per_viewer_limit, set_event_log_retention_days,
};
use forge_transfer::TransferExclusivity;
use gpui::{
    ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Task, Window, div, prelude::*,
    px,
};

use crate::async_bridge;
use crate::backup_schedule::BackupScheduleView;
use crate::safety_copies::SafetyCopiesView;
use crate::settings::SettingsSection;
use crate::settings_pane_header::SettingsPaneHeader;
use crate::transfer_flows::TransferFlows;
use forge_components::ActivePresentation;

const LIMIT_INPUT_W: Pixels = px(120.0);
const PER_VIEWER_PRESETS: [u32; 3] = [25, 50, 100];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PerViewerChoice {
    Preset(u32),
    Unlimited,
    Custom,
}

pub(crate) fn per_viewer_choice(limit: u32, custom_selected: bool) -> PerViewerChoice {
    if custom_selected {
        PerViewerChoice::Custom
    } else if limit == UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT {
        PerViewerChoice::Unlimited
    } else if PER_VIEWER_PRESETS.contains(&limit) {
        PerViewerChoice::Preset(limit)
    } else {
        PerViewerChoice::Custom
    }
}

struct StoredLimits {
    per_viewer: u32,
    display: u32,
    event_log_retention: u32,
    action_history_retention: u32,
}

pub struct SettingsStorageView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    retention: ChatHistoryRetentionHandle,

    per_viewer_limit: u32,
    custom_selected: bool,
    display_limit: u32,
    retention_days: u32,
    history_retention_days: u32,
    transfers: Option<Entity<TransferFlows>>,
    schedule: Option<Entity<BackupScheduleView>>,
    safety_copies: Option<Entity<SafetyCopiesView>>,
    transfer_watch: Option<Task<()>>,

    per_viewer_input: Entity<TextInput>,
    display_input: Entity<TextInput>,
    retention_input: Entity<TextInput>,
    history_retention_input: Entity<TextInput>,
    _subs: Vec<Subscription>,
}

impl SettingsStorageView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        retention: ChatHistoryRetentionHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let per_viewer_input = cx.new(|cx| {
            TextInput::new(DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });
        let display_input = cx.new(|cx| {
            TextInput::new(DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });
        let retention_input = cx.new(|cx| {
            TextInput::new(DEFAULT_EVENT_LOG_RETENTION_DAYS.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });
        let default_history_retention =
            clamp_action_history_retention_days(DEFAULT_EVENT_LOG_RETENTION_DAYS);
        let history_retention_input = cx.new(|cx| {
            TextInput::new(default_history_retention.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });

        let transfers = TransferFlows::route(cx);

        let mut subs = vec![
            cx.subscribe(&per_viewer_input, |this, _input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                    this.commit_custom_per_viewer(cx);
                }
            }),
            cx.subscribe(&display_input, |this, _input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                    this.commit_display(cx);
                }
            }),
            cx.subscribe(&retention_input, |this, _input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                    this.commit_retention(cx);
                }
            }),
            cx.subscribe(
                &history_retention_input,
                |this, _input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                        this.commit_history_retention(cx);
                    }
                },
            ),
        ];
        if let Some(transfers) = &transfers {
            subs.push(cx.observe(transfers, |_, _, cx| cx.notify()));
        }

        let mut view = Self {
            backend,
            rt_handle,
            retention,
            per_viewer_limit: DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
            custom_selected: false,
            display_limit: DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT,
            retention_days: DEFAULT_EVENT_LOG_RETENTION_DAYS,
            history_retention_days: default_history_retention,
            transfers,
            schedule: None,
            safety_copies: None,
            transfer_watch: None,
            per_viewer_input,
            display_input,
            retention_input,
            history_retention_input,
            _subs: subs,
        };
        view.load(cx);
        view
    }

    pub fn attach_schedule(
        &mut self,
        schedule: Entity<BackupScheduleView>,
        cx: &mut Context<Self>,
    ) {
        self._subs
            .push(cx.observe(&schedule, |_, _, cx| cx.notify()));
        self.schedule = Some(schedule);
        cx.notify();
    }

    pub fn attach_safety_copies(
        &mut self,
        safety_copies: Entity<SafetyCopiesView>,
        cx: &mut Context<Self>,
    ) {
        self.safety_copies = Some(safety_copies);
        cx.notify();
    }

    pub fn follow_transfer_lock(
        &mut self,
        exclusivity: &TransferExclusivity,
        cx: &mut Context<Self>,
    ) {
        let mut held = exclusivity.watch_held();
        self.transfer_watch = Some(cx.spawn(async move |this, cx| {
            while held.changed().await.is_some() {
                if this
                    .update(cx, |this, cx| this.transfer_lock_changed(cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn transfer_lock_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(schedule) = &self.schedule {
            schedule.update(cx, |_, cx| cx.notify());
        }
        if let Some(safety_copies) = &self.safety_copies {
            safety_copies.update(cx, |safety_copies, cx| {
                safety_copies.transfer_lock_changed(cx)
            });
        }
        cx.notify();
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            load_limits(repo),
            |this, result, cx| this.apply_loaded(result, cx),
            cx,
        );
    }

    fn apply_loaded(&mut self, result: Result<StoredLimits, String>, cx: &mut Context<Self>) {
        match result {
            Ok(limits) => {
                self.per_viewer_limit = limits.per_viewer;
                self.custom_selected = false;
                self.display_limit = limits.display;
                self.retention_days = limits.event_log_retention;
                self.per_viewer_input
                    .update(cx, |i, cx| i.set_content(limits.per_viewer.to_string(), cx));
                self.display_input
                    .update(cx, |i, cx| i.set_content(limits.display.to_string(), cx));
                self.retention_input.update(cx, |i, cx| {
                    i.set_content(limits.event_log_retention.to_string(), cx)
                });
                self.show_history_retention(limits.action_history_retention, cx);
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to load storage settings");
            }
        }
        cx.notify();
    }

    fn select_per_viewer_preset(&mut self, limit: u32, cx: &mut Context<Self>) {
        self.custom_selected = false;
        self.per_viewer_input
            .update(cx, |i, cx| i.set_content(limit.to_string(), cx));
        self.apply_per_viewer_limit(limit);
        cx.notify();
    }

    fn select_custom_per_viewer(&mut self, cx: &mut Context<Self>) {
        self.custom_selected = true;
        cx.notify();
    }

    fn commit_custom_per_viewer(&mut self, cx: &mut Context<Self>) {
        match parse_per_viewer_limit(self.per_viewer_input.read(cx).content()) {
            Some(value) => {
                if value == UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT {
                    self.custom_selected = false;
                }
                self.per_viewer_input
                    .update(cx, |i, cx| i.set_content(value.to_string(), cx));
                self.apply_per_viewer_limit(value);
            }
            None => {
                let restore = self.per_viewer_limit.to_string();
                self.per_viewer_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn apply_per_viewer_limit(&mut self, limit: u32) {
        if self.per_viewer_limit == limit {
            return;
        }
        self.per_viewer_limit = limit;
        self.retention.set_per_viewer_limit(limit);
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.rt_handle.spawn(async move {
            if let Err(e) = set_chat_history_per_viewer_limit(repo.as_ref(), limit).await {
                tracing::warn!(error = %e, "failed to persist chat history per-viewer limit");
            }
        });
    }

    fn per_viewer_field(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let choice = per_viewer_choice(self.per_viewer_limit, self.custom_selected);
        let mut segments: Vec<_> = PER_VIEWER_PRESETS
            .iter()
            .map(|&limit| {
                segment(
                    SharedString::from(format!("settings-storage-per-viewer-{limit}")),
                    limit.to_string(),
                    choice == PerViewerChoice::Preset(limit),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.select_per_viewer_preset(limit, cx)
                    }),
                )
            })
            .collect();
        segments.push(segment(
            "settings-storage-per-viewer-unlimited",
            tr!("settings_storage_per_viewer_limit_unlimited"),
            choice == PerViewerChoice::Unlimited,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.select_per_viewer_preset(UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT, cx)
            }),
        ));
        segments.push(segment(
            "settings-storage-per-viewer-custom",
            tr!("settings_storage_per_viewer_limit_custom"),
            choice == PerViewerChoice::Custom,
            cx.listener(|this, _: &ClickEvent, _, cx| this.select_custom_per_viewer(cx)),
        ));

        let mut controls = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(segmented(segments, palette));
        if choice == PerViewerChoice::Custom {
            controls = controls.child(
                div()
                    .w(LIMIT_INPUT_W)
                    .min_w(LIMIT_INPUT_W)
                    .max_w(LIMIT_INPUT_W)
                    .child(self.per_viewer_input.clone()),
            );
        }

        let hint = if choice == PerViewerChoice::Unlimited {
            tr!("settings_storage_per_viewer_limit_unlimited_hint")
        } else {
            tr!("settings_storage_per_viewer_limit_hint")
        };
        setting_row(
            tr!("settings_storage_per_viewer_limit_label"),
            Some(hint.into()),
            controls,
            palette,
            density,
        )
    }

    fn commit_display(&mut self, cx: &mut Context<Self>) {
        match parse_limit(self.display_input.read(cx).content()) {
            Some(value) => {
                self.display_limit = value;
                self.display_input
                    .update(cx, |i, cx| i.set_content(value.to_string(), cx));
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.rt_handle.spawn(async move {
                    if let Err(e) = set_chat_history_display_limit(repo.as_ref(), value).await {
                        tracing::warn!(error = %e, "failed to persist chat history display limit");
                    }
                });
            }
            None => {
                let restore = self.display_limit.to_string();
                self.display_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn commit_retention(&mut self, cx: &mut Context<Self>) {
        match parse_retention_days(self.retention_input.read(cx).content()) {
            Some(value) => {
                self.retention_days = value;
                self.retention_input
                    .update(cx, |i, cx| i.set_content(value.to_string(), cx));
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                async_bridge::run_async(
                    &self.rt_handle,
                    persist_event_log_retention(repo, value),
                    |this, result, cx| match result {
                        Ok(history_days) => {
                            this.show_history_retention(history_days, cx);
                            cx.notify();
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "failed to persist event log retention days");
                        }
                    },
                    cx,
                );
            }
            None => {
                let restore = self.retention_days.to_string();
                self.retention_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn commit_history_retention(&mut self, cx: &mut Context<Self>) {
        match parse_history_retention_days(self.history_retention_input.read(cx).content()) {
            Some(value) => {
                self.show_history_retention(value, cx);
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.rt_handle.spawn(async move {
                    if let Err(e) = set_action_history_retention_days(repo.as_ref(), value).await {
                        tracing::warn!(error = %e, "failed to persist action history retention days");
                    }
                });
            }
            None => {
                let restore = self.history_retention_days.to_string();
                self.history_retention_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn show_history_retention(&mut self, days: u32, cx: &mut Context<Self>) {
        self.history_retention_days = days;
        self.history_retention_input
            .update(cx, |i, cx| i.set_content(days.to_string(), cx));
    }

    fn start_restore(&mut self, cx: &mut Context<Self>) {
        if let Some(transfers) = self.transfers.clone() {
            transfers.update(cx, |transfers, cx| transfers.start_restore(cx));
        }
        cx.notify();
    }

    fn start_backup(&mut self, cx: &mut Context<Self>) {
        if let Some(schedule) = &self.schedule
            && schedule.read(cx).backs_up_unattended()
        {
            schedule.update(cx, |schedule, cx| schedule.run_now(cx));
        } else if let Some(transfers) = self.transfers.clone() {
            transfers.update(cx, |transfers, cx| transfers.backup_to_backups_dir(cx));
        }
        cx.notify();
    }

    fn limit_row(
        &self,
        label: impl Into<SharedString>,
        hint: impl Into<SharedString>,
        input: Entity<TextInput>,
        palette: &ForgePalette,
        density: Density,
    ) -> SettingRow {
        setting_row(
            label,
            Some(hint.into()),
            div().w(LIMIT_INPUT_W).child(input),
            palette,
            density,
        )
    }
}

impl Render for SettingsStorageView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let db_path = forge_storage::paths::data_dir().join("forge.db");
        let (backup_flow, restore_flow, flow_views, transfer_busy) = match &self.transfers {
            Some(transfers) => {
                let flows = transfers.read(cx);
                (
                    Some(flows.backup.clone()),
                    Some(flows.restore.clone()),
                    flows.views().to_vec(),
                    flows.is_busy(cx),
                )
            }
            None => (None, None, Vec::new(), true),
        };
        let schedule = self.schedule.as_ref().map(|schedule| schedule.read(cx));
        let unattended = schedule.is_some_and(BackupScheduleView::backs_up_unattended);
        let scheduled_running = schedule.is_some_and(BackupScheduleView::is_running);
        let transfer_held = schedule.is_some_and(BackupScheduleView::transfer_held);
        let schedule_prompt = schedule.and_then(BackupScheduleView::prompt_view);
        let backing_up = scheduled_running
            || backup_flow
                .as_ref()
                .is_some_and(|backup| backup.read(cx).is_writing());
        let restore_preparing = restore_flow
            .as_ref()
            .is_some_and(|restore| restore.read(cx).is_preparing());
        let backup_label = if backing_up {
            tr!("settings_storage_backup_btn_busy")
        } else {
            tr!("settings_storage_backup_btn")
        };
        let backup_btn = primary_button(backup_label, &palette)
            .busy(backing_up)
            .disabled(transfer_busy || transfer_held)
            .on_click(
                "settings-db-backup",
                cx.listener(|this, _: &ClickEvent, _, cx| this.start_backup(cx)),
            );
        let restore_label = if restore_preparing {
            tr!("settings_storage_restore_btn_busy")
        } else {
            tr!("settings_storage_restore_btn")
        };
        let restore_btn = secondary_button(restore_label, &palette)
            .busy(restore_preparing)
            .disabled(transfer_busy)
            .on_click(
                "settings-db-restore",
                cx.listener(|this, _: &ClickEvent, _, cx| this.start_restore(cx)),
            );
        let backup_hint = if (transfer_busy || transfer_held) && !backing_up && !restore_preparing {
            tr!("settings_storage_transfer_busy_hint")
        } else if unattended {
            tr!("settings_storage_backup_scheduled_hint")
        } else {
            tr!("settings_storage_backup_hint")
        };
        let backup_controls = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(restore_btn)
            .child(backup_btn);

        let backup_rows = setting_rows(&palette)
            .row(setting_value_row(
                tr!("settings_storage_db_path_label"),
                db_path.display().to_string(),
                &palette,
                density,
            ))
            .row(setting_row(
                tr!("settings_storage_backup_label"),
                Some(backup_hint.into()),
                backup_controls,
                &palette,
                density,
            ))
            .in_card(&palette);

        let limit_rows = setting_rows(&palette)
            .row(self.per_viewer_field(&palette, density, cx))
            .row(self.limit_row(
                tr!("settings_storage_display_limit_label"),
                tr!("settings_storage_display_limit_hint"),
                self.display_input.clone(),
                &palette,
                density,
            ))
            .row(self.limit_row(
                tr!("settings_storage_retention_label"),
                tr!("settings_storage_retention_hint"),
                self.retention_input.clone(),
                &palette,
                density,
            ))
            .row(self.limit_row(
                tr!("settings_storage_history_retention_label"),
                tr!(
                    "settings_storage_history_retention_hint",
                    min = MIN_ACTION_HISTORY_RETENTION_DAYS.to_string()
                ),
                self.history_retention_input.clone(),
                &palette,
                density,
            ))
            .in_card(&palette);

        div()
            .min_h_full()
            .flex()
            .flex_col()
            .child(SettingsPaneHeader::new(SettingsSection::Storage))
            .child(setting_groups(
                [
                    Some(backup_rows.into_any_element()),
                    self.schedule.clone().map(IntoElement::into_any_element),
                    self.safety_copies
                        .clone()
                        .map(IntoElement::into_any_element),
                    Some(limit_rows.into_any_element()),
                ]
                .into_iter()
                .flatten(),
            ))
            .children(flow_views)
            .children(schedule_prompt)
    }
}

async fn load_limits(repo: Arc<dyn SettingsRepo>) -> Result<StoredLimits, String> {
    let per_viewer = chat_history_per_viewer_limit(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    let display = chat_history_display_limit(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    let event_log_retention = event_log_retention_days(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    let action_history_retention = action_history_retention_days(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(StoredLimits {
        per_viewer,
        display,
        event_log_retention,
        action_history_retention,
    })
}

async fn persist_event_log_retention(
    repo: Arc<dyn SettingsRepo>,
    days: u32,
) -> Result<u32, StorageError> {
    set_event_log_retention_days(repo.as_ref(), days).await?;
    action_history_retention_days(repo.as_ref()).await
}

fn parse_limit(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok().filter(|v| *v >= 1)
}

fn parse_per_viewer_limit(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok().filter(|v| {
        *v == UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT
            || (MIN_CHAT_HISTORY_PER_VIEWER_LIMIT..=MAX_CHAT_HISTORY_PER_VIEWER_LIMIT).contains(v)
    })
}

fn parse_retention_days(raw: &str) -> Option<u32> {
    raw.trim()
        .parse::<u32>()
        .ok()
        .filter(|v| (MIN_EVENT_LOG_RETENTION_DAYS..=MAX_EVENT_LOG_RETENTION_DAYS).contains(v))
}

fn parse_history_retention_days(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok().filter(|v| {
        (MIN_ACTION_HISTORY_RETENTION_DAYS..=MAX_ACTION_HISTORY_RETENTION_DAYS).contains(v)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use forge_events::{Event, EventSource};
    use forge_runtime::{EventBus, spawn_chat_history_persistence};
    use forge_storage::chat_history::MockChatHistoryRepo;
    use forge_storage::{
        DataProvider, SettingsRepo, chat_history_per_viewer_limit, reserved_keys,
        set_action_history_retention_days,
    };
    use forge_types::{ChatPayload, ModerationMarks};
    use gpui::{AppContext, Entity, TestAppContext};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
    use tokio::time::{Duration, timeout};

    use super::{
        PerViewerChoice, SettingsStorageView, parse_history_retention_days, parse_limit,
        parse_per_viewer_limit, parse_retention_days, per_viewer_choice,
    };
    use crate::backup_schedule::tests::{ScheduleRig, key_for, schedule_rig, unattended_seeds};
    use crate::home_stats::HomeStats;
    use crate::test_support::{
        SettingWrite, TestBackend, install_presentation, pump, quiet_bus, runtime, test_backend,
    };
    use crate::transfer_flows::{TransferFlows, TransferRoute};

    const RECV_TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn per_viewer_choice_is_unlimited_for_zero_a_preset_on_match_and_custom_when_chosen() {
        for (limit, custom_selected, expected) in [
            (0, false, PerViewerChoice::Unlimited),
            (0, true, PerViewerChoice::Custom),
            (25, false, PerViewerChoice::Preset(25)),
            (50, false, PerViewerChoice::Preset(50)),
            (100, false, PerViewerChoice::Preset(100)),
            (75, false, PerViewerChoice::Custom),
            (1, false, PerViewerChoice::Custom),
            (50, true, PerViewerChoice::Custom),
        ] {
            assert_eq!(
                per_viewer_choice(limit, custom_selected),
                expected,
                "limit={limit} custom_selected={custom_selected}"
            );
        }
    }

    #[test]
    fn parse_per_viewer_limit_accepts_exactly_the_range_storage_honours() {
        for (raw, expected) in [
            ("0", Some(0)),
            ("1", Some(1)),
            ("10000", Some(10_000)),
            ("10001", None),
            (" 250 ", Some(250)),
            ("abc", None),
            ("", None),
            ("-1", None),
            ("2.5", None),
        ] {
            assert_eq!(
                parse_per_viewer_limit(raw),
                expected,
                "parse_per_viewer_limit({raw:?})"
            );
        }
    }

    struct Rig {
        rt: tokio::runtime::Runtime,
        backend: Arc<TestBackend>,
        writes: UnboundedReceiver<SettingWrite>,
        bus: Arc<EventBus>,
        retained_at: UnboundedReceiver<usize>,
        view: Entity<SettingsStorageView>,
    }

    fn rig(cx: &mut TestAppContext) -> Rig {
        install_presentation(cx);
        let rt = runtime();
        let (backend, writes) = test_backend();
        let bus = quiet_bus(&rt);
        let (retained_tx, retained_at) = unbounded_channel();
        let mut repo = MockChatHistoryRepo::new();
        repo.expect_append_batch().returning(|_| Ok(()));
        repo.expect_apply_retention()
            .returning(move |_, per_viewer| {
                retained_tx.send(per_viewer).unwrap();
                Ok(0)
            });
        let retention = rt.block_on(async {
            spawn_chat_history_persistence(
                Arc::clone(&bus),
                Arc::new(repo),
                Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            )
        });
        pump(&rt);
        let view = cx.update(|cx| {
            cx.new(|cx| {
                SettingsStorageView::new(
                    Arc::clone(&backend) as Arc<dyn DataProvider>,
                    rt.handle().clone(),
                    retention,
                    cx,
                )
            })
        });
        pump(&rt);
        cx.run_until_parked();
        Rig {
            rt,
            backend,
            writes,
            bus,
            retained_at,
            view,
        }
    }

    fn chat_message() -> Event {
        let payload = ChatPayload {
            platform_msg_id: "m1".to_string(),
            author: "bob".to_string(),
            author_color: None,
            segments: vec![],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        };
        Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({ (ChatPayload::KEY): payload }),
        )
    }

    impl Rig {
        fn next_retention_limit(&mut self) -> usize {
            let bus = Arc::clone(&self.bus);
            let retained_at = &mut self.retained_at;
            self.rt.block_on(async move {
                bus.publish(chat_message());
                timeout(RECV_TIMEOUT, retained_at.recv())
                    .await
                    .unwrap()
                    .unwrap()
            })
        }

        fn stored_limit(&self) -> u32 {
            self.rt
                .block_on(chat_history_per_viewer_limit(self.backend.as_ref()))
                .unwrap()
        }

        fn shown_history_retention(&self, cx: &mut TestAppContext) -> String {
            self.view.read_with(cx, |view, cx| {
                view.history_retention_input.read(cx).content().to_owned()
            })
        }

        fn shown_input(&self, cx: &mut TestAppContext) -> String {
            self.view.read_with(cx, |view, cx| {
                view.per_viewer_input.read(cx).content().to_owned()
            })
        }
    }

    #[gpui::test]
    fn choosing_a_preset_persists_it_and_retains_the_next_written_batch_at_it(
        cx: &mut TestAppContext,
    ) {
        let mut rig = rig(cx);

        rig.view
            .update(cx, |view, cx| view.select_per_viewer_preset(25, cx));
        pump(&rig.rt);

        assert_eq!((rig.stored_limit(), rig.next_retention_limit()), (25, 25));
    }

    #[gpui::test]
    fn committing_a_custom_value_applies_a_valid_one_and_restores_an_invalid_one(
        cx: &mut TestAppContext,
    ) {
        for (typed, shown, written) in [
            (" 250 ", "250", Some("250")),
            ("10001", "50", None),
            ("abc", "50", None),
        ] {
            let mut rig = rig(cx);
            rig.view.update(cx, |view, cx| {
                view.select_custom_per_viewer(cx);
                view.per_viewer_input
                    .update(cx, |input, cx| input.set_content(typed.to_owned(), cx));
                view.commit_custom_per_viewer(cx);
            });
            pump(&rig.rt);

            assert_eq!(
                (
                    rig.shown_input(cx),
                    rig.writes.try_recv().ok().map(|(_, value)| value)
                ),
                (shown.to_owned(), written.map(str::to_owned)),
                "typed {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn committing_a_custom_zero_selects_unlimited_and_stops_per_viewer_trimming(
        cx: &mut TestAppContext,
    ) {
        let mut rig = rig(cx);

        rig.view.update(cx, |view, cx| {
            view.select_custom_per_viewer(cx);
            view.per_viewer_input
                .update(cx, |input, cx| input.set_content("0".to_owned(), cx));
            view.commit_custom_per_viewer(cx);
        });
        pump(&rig.rt);

        let choice = rig.view.read_with(cx, |view, _| {
            per_viewer_choice(view.per_viewer_limit, view.custom_selected)
        });
        assert_eq!(
            (choice, rig.stored_limit(), rig.next_retention_limit()),
            (PerViewerChoice::Unlimited, 0, 0)
        );
    }

    #[test]
    fn parse_limit_accepts_positive_integers_and_rejects_everything_else() {
        for (raw, expected) in [
            ("500", Some(500)),
            (" 42 ", Some(42)),
            ("1", Some(1)),
            ("0", None),
            ("", None),
            ("abc", None),
            ("-5", None),
            ("1.5", None),
        ] {
            assert_eq!(parse_limit(raw), expected, "parse_limit({raw:?})");
        }
    }

    #[test]
    fn parse_retention_days_accepts_exactly_the_range_storage_honours() {
        for (raw, expected) in [
            ("0", None),
            ("1", Some(1)),
            ("365", Some(365)),
            ("366", None),
            (" 30 ", Some(30)),
            ("", None),
            ("-1", None),
        ] {
            assert_eq!(
                parse_retention_days(raw),
                expected,
                "parse_retention_days({raw:?})"
            );
        }
    }

    #[test]
    fn parse_history_retention_days_accepts_exactly_the_range_storage_honours() {
        for (raw, expected) in [
            ("6", None),
            ("7", Some(7)),
            ("365", Some(365)),
            ("366", None),
            (" 30 ", Some(30)),
            ("0", None),
            ("", None),
            ("abc", None),
            ("-7", None),
        ] {
            assert_eq!(
                parse_history_retention_days(raw),
                expected,
                "parse_history_retention_days({raw:?})"
            );
        }
    }

    #[gpui::test]
    fn committing_the_history_retention_persists_a_valid_value_and_restores_an_invalid_one(
        cx: &mut TestAppContext,
    ) {
        for (typed, shown, written) in [
            (" 30 ", "30", Some("30")),
            ("6", "7", None),
            ("366", "7", None),
            ("abc", "7", None),
        ] {
            let mut rig = rig(cx);
            rig.view.update(cx, |view, cx| {
                view.history_retention_input
                    .update(cx, |input, cx| input.set_content(typed.to_owned(), cx));
                view.commit_history_retention(cx);
            });
            pump(&rig.rt);

            assert_eq!(
                (rig.shown_history_retention(cx), rig.writes.try_recv().ok()),
                (
                    shown.to_owned(),
                    written.map(|value| (
                        reserved_keys::ACTION_HISTORY_RETENTION_DAYS.to_owned(),
                        value.to_owned()
                    ))
                ),
                "typed {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn saving_the_event_log_retention_re_reads_the_history_retention_it_may_have_moved(
        cx: &mut TestAppContext,
    ) {
        for (stored_history, event_log, shown) in
            [(None, "30", "30"), (None, "3", "7"), (Some(10), "30", "10")]
        {
            let rig = rig(cx);
            if let Some(days) = stored_history {
                rig.rt
                    .block_on(set_action_history_retention_days(
                        rig.backend.as_ref(),
                        days,
                    ))
                    .unwrap();
            }
            rig.view.update(cx, |view, cx| {
                view.retention_input
                    .update(cx, |input, cx| input.set_content(event_log.to_owned(), cx));
                view.commit_retention(cx);
            });
            pump(&rig.rt);
            cx.run_until_parked();

            assert_eq!(
                rig.shown_history_retention(cx),
                shown,
                "stored history {stored_history:?}, event log {event_log:?}"
            );
        }
    }

    fn storage_with_schedule(
        cx: &mut TestAppContext,
        schedule: &ScheduleRig,
    ) -> (Entity<SettingsStorageView>, Entity<TransferFlows>) {
        let services = schedule.world.services.clone();
        let data_dir = schedule.world.data_dir();
        let flows = cx.update(|cx| {
            let stats = cx.new(|_| HomeStats::new());
            let flows = cx.new(|cx| TransferFlows::new(stats, services, data_dir, cx));
            cx.set_global(TransferRoute(flows.clone()));
            flows
        });
        let provider = Arc::clone(schedule.world.provider());
        let bus = quiet_bus(&schedule.rt);
        let mut repo = MockChatHistoryRepo::new();
        repo.expect_append_batch().returning(|_| Ok(()));
        repo.expect_apply_retention().returning(|_, _| Ok(0));
        let retention = schedule.rt.block_on(async {
            spawn_chat_history_persistence(
                bus,
                Arc::new(repo),
                Arc::clone(&provider) as Arc<dyn SettingsRepo>,
            )
        });
        let handle = schedule.rt.handle().clone();
        let schedule_view = schedule.view.clone();
        let view = cx.update(|cx| {
            cx.new(|cx| {
                let mut view = SettingsStorageView::new(provider, handle, retention, cx);
                view.attach_schedule(schedule_view, cx);
                view
            })
        });
        cx.run_until_parked();
        (view, flows)
    }

    #[gpui::test]
    fn backup_now_with_an_unattended_schedule_writes_a_scheduled_backup_without_asking(
        cx: &mut TestAppContext,
    ) {
        let schedule = schedule_rig(cx, &unattended_seeds(&key_for("the schedule passphrase")));
        let (view, flows) = storage_with_schedule(cx, &schedule);

        view.update(cx, |view, cx| view.start_backup(cx));
        let written = schedule.settle(cx, |_| !schedule.scheduled_bundles().is_empty());

        let asked = flows.read_with(cx, |flows, cx| flows.is_busy(cx));
        assert_eq!((written, asked), (true, false));
    }

    #[gpui::test]
    fn backup_now_without_a_schedule_key_opens_the_passphrase_flow(cx: &mut TestAppContext) {
        let schedule = schedule_rig(cx, &[]);
        let (view, flows) = storage_with_schedule(cx, &schedule);

        view.update(cx, |view, cx| view.start_backup(cx));
        let asked = schedule.settle(cx, |cx| flows.read_with(cx, |flows, cx| flows.is_busy(cx)));

        assert_eq!(
            (asked, schedule.scheduled_bundles()),
            (true, Vec::<String>::new())
        );
    }
}
