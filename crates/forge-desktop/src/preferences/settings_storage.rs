use std::sync::Arc;

use forge_components::{
    ActivePresentation, Density, FONT_SM, ForgePalette, InputEvent, PushToast, SettingRow, Spacing,
    TextInput, ToastKind, primary_button, secondary_button, segment, segmented, setting_groups,
    setting_row, setting_rows, setting_value_row, spacing, tr,
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
    AnyElement, App, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Task, Window,
    div, prelude::*, px,
};

use crate::app::async_bridge::{self, Loadable};
use crate::app::navigation::SettingsSection;
use crate::data_flows::backup_schedule::BackupScheduleView;
use crate::data_flows::safety_copies::SafetyCopiesView;
use crate::data_flows::transfer_flows::TransferFlows;
use crate::preferences::settings_pane_header::SettingsPaneHeader;
use crate::shared::load_notice::{LoadNotice, NoticeWidth, load_notice};
use crate::shared::repo_failure::RepoFailure;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Limit {
    PerViewer,
    Display,
    EventLogRetention,
    ActionHistoryRetention,
}

impl Limit {
    const ALL: [Limit; 4] = [
        Limit::PerViewer,
        Limit::Display,
        Limit::EventLogRetention,
        Limit::ActionHistoryRetention,
    ];

    fn default_value(self) -> u32 {
        match self {
            Limit::PerViewer => DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
            Limit::Display => DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT,
            Limit::EventLogRetention => DEFAULT_EVENT_LOG_RETENTION_DAYS,
            Limit::ActionHistoryRetention => {
                clamp_action_history_retention_days(DEFAULT_EVENT_LOG_RETENTION_DAYS)
            }
        }
    }

    fn parse(self, raw: &str) -> Option<u32> {
        match self {
            Limit::PerViewer => parse_per_viewer_limit(raw),
            Limit::Display => parse_limit(raw),
            Limit::EventLogRetention => parse_retention_days(raw),
            Limit::ActionHistoryRetention => parse_history_retention_days(raw),
        }
    }
}

struct LimitField {
    limit: Limit,
    value: u32,
    input: Entity<TextInput>,
}

struct StoredLimits {
    per_viewer: u32,
    display: u32,
    event_log_retention: u32,
    action_history_retention: u32,
}

impl StoredLimits {
    fn value(&self, limit: Limit) -> u32 {
        match limit {
            Limit::PerViewer => self.per_viewer,
            Limit::Display => self.display,
            Limit::EventLogRetention => self.event_log_retention,
            Limit::ActionHistoryRetention => self.action_history_retention,
        }
    }
}

pub(crate) struct SettingsStorageView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    retention: ChatHistoryRetentionHandle,

    limits_load: Loadable<()>,
    limits: Vec<LimitField>,
    custom_selected: bool,
    transfers: Option<Entity<TransferFlows>>,
    schedule: Option<Entity<BackupScheduleView>>,
    safety_copies: Option<Entity<SafetyCopiesView>>,
    transfer_watch: Option<Task<()>>,

    _subs: Vec<Subscription>,
}

impl SettingsStorageView {
    pub(crate) fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        retention: ChatHistoryRetentionHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let transfers = TransferFlows::route(cx);

        let mut subs = Vec::new();
        let limits = Limit::ALL
            .into_iter()
            .map(|limit| {
                let value = limit.default_value();
                let input =
                    cx.new(|cx| TextInput::new(value.to_string(), cx).with_font_size(FONT_SM));
                subs.push(
                    cx.subscribe(&input, move |this, _input, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                            this.commit_limit(limit, cx);
                        }
                    }),
                );
                LimitField {
                    limit,
                    value,
                    input,
                }
            })
            .collect();
        if let Some(transfers) = &transfers {
            subs.push(cx.observe(transfers, |_, _, cx| cx.notify()));
        }

        let mut view = Self {
            backend,
            rt_handle,
            retention,
            limits_load: Loadable::Loading,
            limits,
            custom_selected: false,
            transfers,
            schedule: None,
            safety_copies: None,
            transfer_watch: None,
            _subs: subs,
        };
        view.load(cx);
        view
    }

    pub(crate) fn attach_schedule(
        &mut self,
        schedule: Entity<BackupScheduleView>,
        cx: &mut Context<Self>,
    ) {
        self._subs
            .push(cx.observe(&schedule, |_, _, cx| cx.notify()));
        self.schedule = Some(schedule);
        cx.notify();
    }

    pub(crate) fn attach_safety_copies(
        &mut self,
        safety_copies: Entity<SafetyCopiesView>,
        cx: &mut Context<Self>,
    ) {
        self.safety_copies = Some(safety_copies);
        cx.notify();
    }

    pub(crate) fn follow_transfer_lock(
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
        self.limits_load = Loadable::Loading;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            load_limits(repo),
            |this, result, cx| this.apply_loaded(result, cx),
            cx,
        );
    }

    fn apply_loaded(&mut self, result: Result<StoredLimits, RepoFailure>, cx: &mut Context<Self>) {
        match result {
            Ok(limits) => {
                self.limits_load = Loadable::Loaded(());
                self.custom_selected = false;
                for limit in Limit::ALL {
                    self.show_limit(limit, limits.value(limit), cx);
                }
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to load storage settings");
                self.limits_load = Loadable::Failed(message);
            }
        }
        cx.notify();
    }

    fn limit_field(&self, limit: Limit) -> Option<&LimitField> {
        self.limits.iter().find(|field| field.limit == limit)
    }

    fn limit_value(&self, limit: Limit) -> u32 {
        self.limit_field(limit)
            .map_or_else(|| limit.default_value(), |field| field.value)
    }

    fn limit_input(&self, limit: Limit) -> Option<Entity<TextInput>> {
        self.limit_field(limit).map(|field| field.input.clone())
    }

    fn show_limit(&mut self, limit: Limit, value: u32, cx: &mut Context<Self>) {
        if let Some(field) = self.limits.iter_mut().find(|field| field.limit == limit) {
            field.value = value;
            field
                .input
                .update(cx, |input, cx| input.set_content(value.to_string(), cx));
        }
    }

    fn select_per_viewer_preset(&mut self, limit: u32, cx: &mut Context<Self>) {
        self.custom_selected = false;
        self.apply_limit(Limit::PerViewer, limit, cx);
        cx.notify();
    }

    fn select_custom_per_viewer(&mut self, cx: &mut Context<Self>) {
        self.custom_selected = true;
        cx.notify();
    }

    fn commit_limit(&mut self, limit: Limit, cx: &mut Context<Self>) {
        let Some(input) = self.limit_input(limit) else {
            return;
        };
        match limit.parse(input.read(cx).content()) {
            Some(value) => {
                if value == UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT && limit == Limit::PerViewer {
                    self.custom_selected = false;
                }
                self.apply_limit(limit, value, cx);
            }
            None => self.show_limit(limit, self.limit_value(limit), cx),
        }
        cx.notify();
    }

    fn apply_limit(&mut self, limit: Limit, value: u32, cx: &mut Context<Self>) {
        let previous = self.limit_value(limit);
        self.show_limit(limit, value, cx);
        if previous == value {
            return;
        }
        if limit == Limit::PerViewer {
            self.retention.set_per_viewer_limit(value);
        }
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            persist_limit(repo, limit, value),
            move |this, result, cx| this.apply_persisted(limit, previous, result, cx),
            cx,
        );
    }

    fn apply_persisted(
        &mut self,
        limit: Limit,
        previous: u32,
        result: Result<Option<u32>, StorageError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some(history_days)) => {
                self.show_limit(Limit::ActionHistoryRetention, history_days, cx);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(?limit, error = %RepoFailure::from(error), "failed to persist a storage limit");
                self.show_limit(limit, previous, cx);
                if limit == Limit::PerViewer {
                    self.retention.set_per_viewer_limit(previous);
                }
                cx.push_toast(ToastKind::Error, tr!("settings_persist_failed"));
            }
        }
        cx.notify();
    }

    fn per_viewer_field(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let choice = per_viewer_choice(self.limit_value(Limit::PerViewer), self.custom_selected);
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
                    .children(self.limit_input(Limit::PerViewer)),
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
        limit: Limit,
        palette: &ForgePalette,
        density: Density,
    ) -> SettingRow {
        setting_row(
            label,
            Some(hint.into()),
            div().w(LIMIT_INPUT_W).children(self.limit_input(limit)),
            palette,
            density,
        )
    }
}

struct TransferStatus {
    transfer_busy: bool,
    transfer_held: bool,
    unattended: bool,
    backing_up: bool,
    restore_preparing: bool,
}

impl SettingsStorageView {
    fn transfer_status(&self, cx: &App) -> TransferStatus {
        let (backup_flow, restore_flow, transfer_busy) = match &self.transfers {
            Some(transfers) => {
                let flows = transfers.read(cx);
                (
                    Some(flows.backup.clone()),
                    Some(flows.restore.clone()),
                    flows.is_busy(cx),
                )
            }
            None => (None, None, true),
        };
        let schedule = self.schedule.as_ref().map(|schedule| schedule.read(cx));
        let scheduled_running = schedule.is_some_and(BackupScheduleView::is_running);
        TransferStatus {
            transfer_busy,
            transfer_held: schedule.is_some_and(BackupScheduleView::transfer_held),
            unattended: schedule.is_some_and(BackupScheduleView::backs_up_unattended),
            backing_up: scheduled_running
                || backup_flow
                    .as_ref()
                    .is_some_and(|backup| backup.read(cx).is_writing()),
            restore_preparing: restore_flow
                .as_ref()
                .is_some_and(|restore| restore.read(cx).is_preparing()),
        }
    }

    fn backup_rows(
        &self,
        status: &TransferStatus,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let db_path = crate::app::app_paths::app_paths()
            .data_dir()
            .join(forge_storage::DATABASE_FILE_NAME);
        let backup_label = if status.backing_up {
            tr!("settings_storage_backup_btn_busy")
        } else {
            tr!("settings_storage_backup_btn")
        };
        let backup_btn = primary_button(backup_label, palette)
            .busy(status.backing_up)
            .disabled(status.transfer_busy || status.transfer_held)
            .on_click(
                "settings-db-backup",
                cx.listener(|this, _: &ClickEvent, _, cx| this.start_backup(cx)),
            );
        let restore_label = if status.restore_preparing {
            tr!("settings_storage_restore_btn_busy")
        } else {
            tr!("settings_storage_restore_btn")
        };
        let restore_btn = secondary_button(restore_label, palette)
            .busy(status.restore_preparing)
            .disabled(status.transfer_busy)
            .on_click(
                "settings-db-restore",
                cx.listener(|this, _: &ClickEvent, _, cx| this.start_restore(cx)),
            );
        let backup_hint = if (status.transfer_busy || status.transfer_held)
            && !status.backing_up
            && !status.restore_preparing
        {
            tr!("settings_storage_transfer_busy_hint")
        } else if status.unattended {
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

        setting_rows(palette)
            .row(setting_value_row(
                tr!("settings_storage_db_path_label"),
                db_path.display().to_string(),
                palette,
                density,
                cx,
            ))
            .row(setting_row(
                tr!("settings_storage_backup_label"),
                Some(backup_hint.into()),
                backup_controls,
                palette,
                density,
            ))
            .in_card(palette)
            .into_any_element()
    }

    fn limit_rows(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        setting_rows(palette)
            .row(self.per_viewer_field(palette, density, cx))
            .row(self.limit_row(
                tr!("settings_storage_display_limit_label"),
                tr!("settings_storage_display_limit_hint"),
                Limit::Display,
                palette,
                density,
            ))
            .row(self.limit_row(
                tr!("settings_storage_retention_label"),
                tr!("settings_storage_retention_hint"),
                Limit::EventLogRetention,
                palette,
                density,
            ))
            .row(self.limit_row(
                tr!("settings_storage_history_retention_label"),
                tr!(
                    "settings_storage_history_retention_hint",
                    min = MIN_ACTION_HISTORY_RETENTION_DAYS.to_string()
                ),
                Limit::ActionHistoryRetention,
                palette,
                density,
            ))
            .in_card(palette)
            .into_any_element()
    }

    fn limits_failure(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        matches!(self.limits_load, Loadable::Failed(_))
            .then(|| {
                load_notice(
                    &self.limits_load,
                    LoadNotice {
                        id: "settings-storage-limits-load",
                        loading: tr!("settings_storage_limits_loading"),
                        width: NoticeWidth::Pane,
                    },
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.load(cx);
                        cx.notify();
                    }),
                    palette,
                    density,
                    cx,
                )
            })
            .flatten()
    }
}

impl Render for SettingsStorageView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let status = self.transfer_status(cx);
        let flow_views = self
            .transfers
            .as_ref()
            .map(|transfers| transfers.read(cx).views().to_vec())
            .unwrap_or_default();
        let schedule_prompt = self
            .schedule
            .as_ref()
            .and_then(|schedule| schedule.read(cx).prompt_view());
        let backup_rows = self.backup_rows(&status, &palette, density, cx);
        let limit_rows = self.limit_rows(&palette, density, cx);
        let limits_failure = self.limits_failure(&palette, density, cx);

        div()
            .min_h_full()
            .flex()
            .flex_col()
            .child(SettingsPaneHeader::new(SettingsSection::Storage))
            .children(limits_failure)
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
                density,
            ))
            .children(flow_views)
            .children(schedule_prompt)
    }
}

async fn load_limits(repo: Arc<dyn SettingsRepo>) -> Result<StoredLimits, RepoFailure> {
    let per_viewer = chat_history_per_viewer_limit(repo.as_ref())
        .await
        .map_err(RepoFailure::from)?;
    let display = chat_history_display_limit(repo.as_ref())
        .await
        .map_err(RepoFailure::from)?;
    let event_log_retention = event_log_retention_days(repo.as_ref())
        .await
        .map_err(RepoFailure::from)?;
    let action_history_retention = action_history_retention_days(repo.as_ref())
        .await
        .map_err(RepoFailure::from)?;
    Ok(StoredLimits {
        per_viewer,
        display,
        event_log_retention,
        action_history_retention,
    })
}

async fn persist_limit(
    repo: Arc<dyn SettingsRepo>,
    limit: Limit,
    value: u32,
) -> Result<Option<u32>, StorageError> {
    let repo = repo.as_ref();
    match limit {
        Limit::PerViewer => set_chat_history_per_viewer_limit(repo, value).await?,
        Limit::Display => set_chat_history_display_limit(repo, value).await?,
        Limit::ActionHistoryRetention => set_action_history_retention_days(repo, value).await?,
        Limit::EventLogRetention => {
            set_event_log_retention_days(repo, value).await?;
            return action_history_retention_days(repo).await.map(Some);
        }
    }
    Ok(None)
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
mod tests {
    use std::sync::Arc;

    use forge_events::{Event, EventSource};
    use forge_runtime::{EventBus, spawn_chat_history_persistence};
    use forge_storage::MockChatHistoryRepo;
    use forge_storage::{
        DataProvider, SettingsRepo, chat_history_per_viewer_limit,
        set_action_history_retention_days,
    };
    use forge_types::{ChatPayload, ModerationMarks};
    use gpui::{AppContext, Entity, TestAppContext};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    use super::{
        Limit, PerViewerChoice, SettingsStorageView, parse_history_retention_days, parse_limit,
        parse_per_viewer_limit, parse_retention_days, per_viewer_choice,
    };
    use crate::audience::home_stats::HomeStats;
    use crate::data_flows::backup_schedule::tests::{
        ScheduleRig, key_for, schedule_rig, unattended_seeds,
    };
    use crate::data_flows::transfer_flows::{TransferFlows, TransferRoute};
    use crate::test_support::{
        SettingWrite, TestBackend, install_presentation, pump, quiet_bus, runtime, settle_until,
        test_backend, within_hang_guard,
    };

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
                within_hang_guard(retained_at.recv()).await.unwrap()
            })
        }

        fn stored_limit(&self) -> u32 {
            self.rt
                .block_on(chat_history_per_viewer_limit(self.backend.as_ref()))
                .unwrap()
        }

        fn shown_history_retention(&self, cx: &mut TestAppContext) -> String {
            self.view.read_with(cx, |view, cx| {
                view.limit_input(Limit::ActionHistoryRetention)
                    .unwrap()
                    .read(cx)
                    .content()
                    .to_owned()
            })
        }

        fn shown_input(&self, cx: &mut TestAppContext) -> String {
            self.view.read_with(cx, |view, cx| {
                view.limit_input(Limit::PerViewer)
                    .unwrap()
                    .read(cx)
                    .content()
                    .to_owned()
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
                view.limit_input(Limit::PerViewer)
                    .unwrap()
                    .update(cx, |input, cx| input.set_content(typed.to_owned(), cx));
                view.commit_limit(Limit::PerViewer, cx);
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

    fn refused_setting(
        key: &str,
        cx: &mut TestAppContext,
        write: impl FnOnce(&mut SettingsStorageView, &mut gpui::Context<SettingsStorageView>),
        shown: impl Fn(&SettingsStorageView, &gpui::App) -> String,
    ) -> (String, String, usize) {
        let rig = rig(cx);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        rig.backend.fail_writes(key);
        let before = rig.view.read_with(cx, |view, cx| shown(view, cx));

        rig.view.update(cx, write);
        pump(&rig.rt);
        cx.run_until_parked();

        let after = rig.view.read_with(cx, |view, cx| shown(view, cx));
        (
            before,
            after,
            cx.update(|cx| crate::test_support::error_toasts(cx)),
        )
    }

    fn shown_limit(view: &SettingsStorageView, limit: Limit, cx: &gpui::App) -> String {
        view.limit_input(limit)
            .expect("every limit has an input")
            .read(cx)
            .content()
            .to_owned()
    }

    fn enter_limit(
        view: &mut SettingsStorageView,
        limit: Limit,
        typed: &str,
        cx: &mut gpui::Context<SettingsStorageView>,
    ) {
        view.limit_input(limit)
            .expect("every limit has an input")
            .update(cx, |input, cx| input.set_content(typed.to_owned(), cx));
        view.commit_limit(limit, cx);
    }

    fn assert_rolled_back((before, after, toasts): (String, String, usize)) {
        assert_eq!((after, toasts), (before, 1));
    }

    #[gpui::test]
    fn a_refused_per_viewer_limit_is_taken_back_with_a_toast(cx: &mut TestAppContext) {
        assert_rolled_back(refused_setting(
            forge_storage::settings::core_settings::CHAT_HISTORY_PER_VIEWER_LIMIT.name(),
            cx,
            |view, cx| view.select_per_viewer_preset(25, cx),
            |view, cx| shown_limit(view, Limit::PerViewer, cx),
        ));
    }

    #[gpui::test]
    fn a_refused_display_limit_is_taken_back_with_a_toast(cx: &mut TestAppContext) {
        assert_rolled_back(refused_setting(
            forge_storage::settings::core_settings::CHAT_HISTORY_DISPLAY_LIMIT.name(),
            cx,
            |view, cx| {
                enter_limit(view, Limit::Display, "321", cx);
            },
            |view, cx| shown_limit(view, Limit::Display, cx),
        ));
    }

    #[gpui::test]
    fn a_refused_event_log_retention_is_taken_back_with_a_toast(cx: &mut TestAppContext) {
        assert_rolled_back(refused_setting(
            forge_storage::settings::core_settings::EVENT_LOG_RETENTION_DAYS.name(),
            cx,
            |view, cx| {
                enter_limit(view, Limit::EventLogRetention, "90", cx);
            },
            |view, cx| shown_limit(view, Limit::EventLogRetention, cx),
        ));
    }

    #[gpui::test]
    fn a_refused_history_retention_is_taken_back_with_a_toast(cx: &mut TestAppContext) {
        assert_rolled_back(refused_setting(
            forge_storage::settings::core_settings::ACTION_HISTORY_RETENTION_DAYS.name(),
            cx,
            |view, cx| {
                enter_limit(view, Limit::ActionHistoryRetention, "30", cx);
            },
            |view, cx| shown_limit(view, Limit::ActionHistoryRetention, cx),
        ));
    }

    #[gpui::test]
    fn committing_a_custom_zero_selects_unlimited_and_stops_per_viewer_trimming(
        cx: &mut TestAppContext,
    ) {
        let mut rig = rig(cx);

        rig.view.update(cx, |view, cx| {
            view.select_custom_per_viewer(cx);
            view.limit_input(Limit::PerViewer)
                .unwrap()
                .update(cx, |input, cx| input.set_content("0".to_owned(), cx));
            view.commit_limit(Limit::PerViewer, cx);
        });
        pump(&rig.rt);

        let choice = rig.view.read_with(cx, |view, _| {
            per_viewer_choice(view.limit_value(Limit::PerViewer), view.custom_selected)
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
                view.limit_input(Limit::ActionHistoryRetention)
                    .unwrap()
                    .update(cx, |input, cx| input.set_content(typed.to_owned(), cx));
                view.commit_limit(Limit::ActionHistoryRetention, cx);
            });
            pump(&rig.rt);

            assert_eq!(
                (rig.shown_history_retention(cx), rig.writes.try_recv().ok()),
                (
                    shown.to_owned(),
                    written.map(|value| (
                        forge_storage::settings::core_settings::ACTION_HISTORY_RETENTION_DAYS
                            .name()
                            .to_owned(),
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
                view.limit_input(Limit::EventLogRetention)
                    .unwrap()
                    .update(cx, |input, cx| input.set_content(event_log.to_owned(), cx));
                view.commit_limit(Limit::EventLogRetention, cx);
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
        settle_until(cx, &schedule.rt, |_| {
            !schedule.scheduled_bundles().is_empty()
        });

        let asked = flows.read_with(cx, |flows, cx| flows.is_busy(cx));
        assert!(!asked);
    }

    #[gpui::test]
    fn backup_now_without_a_schedule_key_opens_the_passphrase_flow(cx: &mut TestAppContext) {
        let schedule = schedule_rig(cx, &[]);
        let (view, flows) = storage_with_schedule(cx, &schedule);

        view.update(cx, |view, cx| view.start_backup(cx));
        settle_until(cx, &schedule.rt, |cx| {
            flows.read_with(cx, |flows, cx| flows.is_busy(cx))
        });

        assert_eq!(schedule.scheduled_bundles(), Vec::<String>::new());
    }

    #[gpui::test]
    fn every_take_and_release_of_the_transfer_lock_repaints_the_storage_cards(
        cx: &mut TestAppContext,
    ) {
        let schedule = schedule_rig(cx, &[]);
        let (view, _flows) = storage_with_schedule(cx, &schedule);
        let exclusivity = forge_transfer::TransferExclusivity::new();
        view.update(cx, |view, cx| view.follow_transfer_lock(&exclusivity, cx));
        let repaints = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counter = std::rc::Rc::clone(&repaints);
        cx.update(|cx| {
            cx.observe(&schedule.view, move |_, _| counter.set(counter.get() + 1))
                .detach();
        });
        cx.run_until_parked();
        let before = repaints.get();

        let guard = exclusivity.try_acquire().unwrap();
        cx.run_until_parked();
        let after_take = repaints.get();
        drop(guard);
        cx.run_until_parked();
        let after_release = repaints.get();

        assert_eq!(
            (after_take > before, after_release > after_take),
            (true, true)
        );
    }

    #[gpui::test]
    fn a_dropped_storage_view_stops_following_the_transfer_lock(cx: &mut TestAppContext) {
        let schedule = schedule_rig(cx, &[]);
        let (view, _flows) = storage_with_schedule(cx, &schedule);
        let exclusivity = forge_transfer::TransferExclusivity::new();
        view.update(cx, |view, cx| view.follow_transfer_lock(&exclusivity, cx));
        let repaints = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counter = std::rc::Rc::clone(&repaints);
        cx.update(|cx| {
            cx.observe(&schedule.view, move |_, _| counter.set(counter.get() + 1))
                .detach();
        });
        drop(view);
        cx.run_until_parked();
        let before = repaints.get();

        drop(exclusivity.try_acquire().unwrap());
        cx.run_until_parked();

        assert_eq!(repaints.get(), before);
    }
}
