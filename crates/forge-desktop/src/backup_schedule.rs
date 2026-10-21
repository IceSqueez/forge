use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SM, FONT_XS, ForgePalette, Icon, InputEvent,
    PushToast, Radius, SettingRow, Spacing, TextInput, ToastKind, body_family, checkbox, fmt_bytes,
    fmt_relative_time, ghost_button, ghost_button_with_icon, mono_family, radius, setting_row,
    setting_rows, spacing, toggle, tr,
};
use forge_storage::settings::{
    DEFAULT_BACKUP_SCHEDULE_ENABLED, DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY,
    DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES, DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
    DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST, MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES,
    MAX_BACKUP_SCHEDULE_KEEP_DAILY_DAYS, MAX_BACKUP_SCHEDULE_KEEP_NEWEST,
    MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES, MIN_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
    MIN_BACKUP_SCHEDULE_KEEP_NEWEST,
};
use forge_storage::{
    DataProvider, SettingsRepo, StorageError, get_bool_setting, reserved_keys, set_bool_setting,
};
use forge_transfer::{
    BackupCategory, FailureCategory, Passphrase, ScheduleKey, ScheduleState, ScheduledBackupHandle,
    ScheduledBackupStatus, StorageUsed, TransferError, TransferExclusivity, backups_dir,
    change_schedule_passphrase, create_schedule_key,
};
use gpui::{
    AnyView, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};
use time::OffsetDateTime;

use crate::async_bridge::{self, DIALOG_CANCELLED, Debounced, ErrorSink, SLIDER_PERSIST_DEBOUNCE};
use crate::backup_passphrase_modal::{
    BackupPassphraseEvent, BackupPassphraseModal, PassphrasePurpose,
};
use crate::scheduled_run_labels::{local_stamp, system_offset_at};

const NUMBER_INPUT_W: Pixels = px(120.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScheduleNumber {
    Interval,
    KeepNewest,
    KeepDailyDays,
}

impl ScheduleNumber {
    pub(crate) const ALL: [Self; 3] = [Self::Interval, Self::KeepNewest, Self::KeepDailyDays];

    pub(crate) fn storage_key(self) -> &'static str {
        match self {
            Self::Interval => reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES,
            Self::KeepNewest => reserved_keys::BACKUP_SCHEDULE_KEEP_NEWEST,
            Self::KeepDailyDays => reserved_keys::BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
        }
    }

    pub(crate) fn range(self) -> RangeInclusive<u32> {
        match self {
            Self::Interval => {
                MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES..=MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES
            }
            Self::KeepNewest => MIN_BACKUP_SCHEDULE_KEEP_NEWEST..=MAX_BACKUP_SCHEDULE_KEEP_NEWEST,
            Self::KeepDailyDays => {
                MIN_BACKUP_SCHEDULE_KEEP_DAILY_DAYS..=MAX_BACKUP_SCHEDULE_KEEP_DAILY_DAYS
            }
        }
    }

    pub(crate) fn default_value(self) -> u32 {
        match self {
            Self::Interval => DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES,
            Self::KeepNewest => DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST,
            Self::KeepDailyDays => DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
        }
    }

    pub(crate) fn parse(self, text: &str) -> Option<u32> {
        text.trim()
            .parse::<u32>()
            .ok()
            .filter(|value| self.range().contains(value))
    }

    pub(crate) fn value_from_stored(self, stored: Option<&str>) -> u32 {
        let range = self.range();
        stored
            .and_then(|raw| raw.trim().parse::<u32>().ok())
            .map_or(self.default_value(), |value| {
                value.clamp(*range.start(), *range.end())
            })
    }

    fn label(self) -> String {
        match self {
            Self::Interval => tr!("backup_schedule_interval_label"),
            Self::KeepNewest => tr!("backup_schedule_keep_newest_label"),
            Self::KeepDailyDays => tr!("backup_schedule_keep_daily_label"),
        }
    }

    fn hint(self) -> String {
        let range = self.range();
        let (min, max) = (*range.start(), *range.end());
        match self {
            Self::Interval => tr!("backup_schedule_interval_hint", min = min, max = max),
            Self::KeepNewest => tr!("backup_schedule_keep_newest_hint", min = min, max = max),
            Self::KeepDailyDays => tr!("backup_schedule_keep_daily_hint", min = min, max = max),
        }
    }

    fn range_error(self) -> String {
        let range = self.range();
        tr!(
            "backup_schedule_range_error",
            min = *range.start(),
            max = *range.end()
        )
    }

    fn unit(self, count: u32) -> String {
        match self {
            Self::Interval => tr!("backup_schedule_unit_minutes", count = count),
            Self::KeepNewest => tr!("backup_schedule_unit_backups", count = count),
            Self::KeepDailyDays => tr!("backup_schedule_unit_days", count = count),
        }
    }
}

pub(crate) const OPTIONAL_CATEGORIES: [BackupCategory; 4] = [
    BackupCategory::Media,
    BackupCategory::Voices,
    BackupCategory::Overlays,
    BackupCategory::Assets,
];

pub(crate) fn include_key(category: BackupCategory) -> Option<&'static str> {
    match category {
        BackupCategory::Database => None,
        BackupCategory::Media => Some(reserved_keys::BACKUP_SCHEDULE_INCLUDE_MEDIA),
        BackupCategory::Voices => Some(reserved_keys::BACKUP_SCHEDULE_INCLUDE_VOICES),
        BackupCategory::Overlays => Some(reserved_keys::BACKUP_SCHEDULE_INCLUDE_OVERLAYS),
        BackupCategory::Assets => Some(reserved_keys::BACKUP_SCHEDULE_INCLUDE_ASSETS),
    }
}

fn category_label(category: BackupCategory) -> String {
    match category {
        BackupCategory::Database => tr!("backup_schedule_category_database"),
        BackupCategory::Media => tr!("backup_schedule_category_media"),
        BackupCategory::Voices => tr!("backup_schedule_category_voices"),
        BackupCategory::Overlays => tr!("backup_schedule_category_overlays"),
        BackupCategory::Assets => tr!("backup_schedule_category_assets"),
    }
}

pub(crate) fn failure_reason(category: FailureCategory) -> String {
    match category {
        FailureCategory::DestinationMissing => tr!("backup_schedule_failure_destination_missing"),
        FailureCategory::DestinationNotWritable => {
            tr!("backup_schedule_failure_destination_not_writable")
        }
        FailureCategory::DiskFull => tr!("backup_schedule_failure_disk_full"),
        FailureCategory::DestinationInsideBackupScope => {
            tr!("backup_schedule_failure_inside_scope")
        }
        FailureCategory::KeyMissing => tr!("backup_schedule_failure_key_missing"),
        FailureCategory::KeyUnreadable => tr!("backup_schedule_failure_key_unreadable"),
        FailureCategory::SnapshotFailed => tr!("backup_schedule_failure_snapshot_failed"),
        FailureCategory::StateNotSaved => tr!("backup_schedule_failure_state_not_saved"),
        FailureCategory::Interrupted => tr!("backup_schedule_failure_interrupted"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoragePart {
    Database,
    Media,
    Voices,
    Overlays,
    Assets,
    Logs,
    Backups,
    Destination,
}

impl StoragePart {
    fn label(self) -> String {
        match self {
            Self::Database => tr!("backup_schedule_storage_used_database"),
            Self::Media => tr!("backup_schedule_storage_used_media"),
            Self::Voices => tr!("backup_schedule_storage_used_voices"),
            Self::Overlays => tr!("backup_schedule_storage_used_overlays"),
            Self::Assets => tr!("backup_schedule_storage_used_assets"),
            Self::Logs => tr!("backup_schedule_storage_used_logs"),
            Self::Backups => tr!("backup_schedule_storage_used_backups"),
            Self::Destination => tr!("backup_schedule_storage_used_destination"),
        }
    }
}

pub(crate) fn storage_parts(used: &StorageUsed) -> Vec<(StoragePart, u64)> {
    let mut parts = vec![
        (StoragePart::Database, used.database),
        (StoragePart::Media, used.media),
        (StoragePart::Voices, used.voices),
        (StoragePart::Overlays, used.overlays),
        (StoragePart::Assets, used.assets),
        (StoragePart::Logs, used.logs),
        (StoragePart::Backups, used.backups),
    ];
    if used.destination_outside_data_dir > 0 {
        parts.push((StoragePart::Destination, used.destination_outside_data_dir));
    }
    parts
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLine {
    Off {
        last_success_at: Option<OffsetDateTime>,
    },
    Paused,
    Running,
    Failing {
        category: FailureCategory,
        since: OffsetDateTime,
        attempts: u32,
    },
    Scheduled {
        last_success_at: Option<OffsetDateTime>,
        next_due: Option<OffsetDateTime>,
    },
}

impl StatusLine {
    pub(crate) fn of(status: &ScheduledBackupStatus) -> Self {
        match (status.state, status.failure) {
            (ScheduleState::Disabled, _) => Self::Off {
                last_success_at: status.last_success_at,
            },
            (ScheduleState::Suspended, _) => Self::Paused,
            (ScheduleState::Running, _) => Self::Running,
            (ScheduleState::Idle, Some(streak)) => Self::Failing {
                category: streak.category,
                since: streak.since,
                attempts: streak.attempts,
            },
            (ScheduleState::Idle, None) => Self::Scheduled {
                last_success_at: status.last_success_at,
                next_due: status.next_due,
            },
        }
    }

    pub(crate) fn is_failure(self) -> bool {
        matches!(self, Self::Failing { .. })
    }

    fn text(self) -> String {
        match self {
            Self::Off { last_success_at } => tr!(
                "backup_schedule_status_off",
                last = fmt_relative_time(last_success_at)
            ),
            Self::Paused => tr!("backup_schedule_status_paused"),
            Self::Running => tr!("backup_schedule_status_running"),
            Self::Failing {
                category,
                since,
                attempts,
            } => tr!(
                "backup_schedule_status_failing",
                since = local_stamp(since, system_offset_at(since)),
                attempts = attempts,
                reason = failure_reason(category)
            ),
            Self::Scheduled {
                last_success_at,
                next_due: Some(next),
            } => tr!(
                "backup_schedule_status_scheduled",
                last = fmt_relative_time(last_success_at),
                next = local_stamp(next, system_offset_at(next))
            ),
            Self::Scheduled {
                last_success_at,
                next_due: None,
            } => tr!(
                "backup_schedule_status_last",
                last = fmt_relative_time(last_success_at)
            ),
        }
    }
}

pub(crate) struct ScheduleSnapshot {
    pub enabled: bool,
    pub key: Option<ScheduleKey>,
    pub numbers: Vec<(ScheduleNumber, u32)>,
    pub included: Vec<BackupCategory>,
    pub destination: Option<String>,
}

async fn stored_text(repo: &dyn SettingsRepo, key: &str) -> Result<Option<String>, StorageError> {
    Ok(repo
        .get_string(key)
        .await?
        .filter(|value| !value.trim().is_empty()))
}

pub(crate) async fn load_schedule(
    repo: Arc<dyn SettingsRepo>,
) -> Result<ScheduleSnapshot, StorageError> {
    let repo = repo.as_ref();
    let mut numbers = Vec::with_capacity(ScheduleNumber::ALL.len());
    for number in ScheduleNumber::ALL {
        let stored = stored_text(repo, number.storage_key()).await?;
        numbers.push((number, number.value_from_stored(stored.as_deref())));
    }
    let mut included = Vec::with_capacity(OPTIONAL_CATEGORIES.len());
    for category in OPTIONAL_CATEGORIES {
        if let Some(key) = include_key(category)
            && get_bool_setting(repo, key, DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY).await
        {
            included.push(category);
        }
    }
    let key = stored_text(repo, reserved_keys::BACKUP_SCHEDULE_KEY)
        .await?
        .and_then(|stored| ScheduleKey::decode(&stored).ok());
    Ok(ScheduleSnapshot {
        enabled: get_bool_setting(
            repo,
            reserved_keys::BACKUP_SCHEDULE_ENABLED,
            DEFAULT_BACKUP_SCHEDULE_ENABLED,
        )
        .await,
        key,
        numbers,
        included,
        destination: stored_text(repo, reserved_keys::BACKUP_SCHEDULE_DESTINATION).await?,
    })
}

#[derive(Debug)]
pub(crate) enum KeyFailure {
    WrongCurrent,
    Other(String),
}

impl From<TransferError> for KeyFailure {
    fn from(error: TransferError) -> Self {
        match error {
            TransferError::WrongPassphrase => Self::WrongCurrent,
            other => Self::Other(other.to_string()),
        }
    }
}

impl From<StorageError> for KeyFailure {
    fn from(error: StorageError) -> Self {
        Self::Other(error.to_string())
    }
}

async fn store_key(
    repo: &dyn SettingsRepo,
    key: &ScheduleKey,
    enable: bool,
) -> Result<(), KeyFailure> {
    repo.set_string(reserved_keys::BACKUP_SCHEDULE_KEY, &key.encode()?)
        .await?;
    if enable {
        set_bool_setting(repo, reserved_keys::BACKUP_SCHEDULE_ENABLED, true).await?;
    }
    Ok(())
}

struct NumberField {
    number: ScheduleNumber,
    input: Entity<TextInput>,
    debounce: Debounced,
    value: u32,
    invalid: bool,
}

struct PassphrasePrompt {
    purpose: PassphrasePurpose,
    modal: Entity<BackupPassphraseModal>,
    _sub: Subscription,
}

pub struct BackupScheduleView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    service: ScheduledBackupHandle,
    exclusivity: TransferExclusivity,
    data_dir: PathBuf,
    enabled: bool,
    key: Option<ScheduleKey>,
    included: Vec<BackupCategory>,
    destination: Option<String>,
    status: ScheduledBackupStatus,
    numbers: Vec<NumberField>,
    prompt: Option<PassphrasePrompt>,
    _subs: Vec<Subscription>,
}

impl BackupScheduleView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        service: ScheduledBackupHandle,
        exclusivity: TransferExclusivity,
        data_dir: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let mut subs = Vec::with_capacity(ScheduleNumber::ALL.len());
        let mut numbers = Vec::with_capacity(ScheduleNumber::ALL.len());
        for number in ScheduleNumber::ALL {
            let value = number.default_value();
            let input = cx.new(|cx| {
                TextInput::new(value.to_string(), cx)
                    .with_palette(palette)
                    .with_font_size(FONT_SM)
            });
            subs.push(
                cx.subscribe(&input, move |this, _input, event: &InputEvent, cx| {
                    if let InputEvent::Changed(text) | InputEvent::Submitted(text) = event {
                        this.commit_number(number, text.as_ref(), cx);
                    }
                }),
            );
            numbers.push(NumberField {
                number,
                input,
                debounce: Debounced::new(SLIDER_PERSIST_DEBOUNCE),
                value,
                invalid: false,
            });
        }
        let status = service.status().borrow().clone();
        let mut view = Self {
            backend,
            rt_handle,
            service,
            exclusivity,
            data_dir,
            enabled: DEFAULT_BACKUP_SCHEDULE_ENABLED,
            key: None,
            included: OPTIONAL_CATEGORIES.to_vec(),
            destination: None,
            status,
            numbers,
            prompt: None,
            _subs: subs,
        };
        view.load(cx);
        view.start_status_bridge(cx);
        view.service.storage_changed();
        view
    }

    fn settings_repo(&self) -> Arc<dyn SettingsRepo> {
        Arc::clone(&self.backend) as Arc<dyn SettingsRepo>
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        async_bridge::run_async(
            &self.rt_handle,
            load_schedule(self.settings_repo()),
            |this, result, cx| match result {
                Ok(snapshot) => this.apply_snapshot(snapshot, cx),
                Err(error) => {
                    tracing::warn!(%error, "failed to load the backup schedule settings");
                }
            },
            cx,
        );
    }

    pub(crate) fn apply_snapshot(&mut self, snapshot: ScheduleSnapshot, cx: &mut Context<Self>) {
        self.enabled = snapshot.enabled;
        self.key = snapshot.key;
        self.included = snapshot.included;
        self.destination = snapshot.destination;
        for (number, value) in snapshot.numbers {
            if let Some(field) = self.numbers.iter_mut().find(|f| f.number == number) {
                field.value = value;
                field.invalid = false;
                field.input.update(cx, |input, cx| {
                    input.set_content(value.to_string(), cx);
                    input.set_invalid(false, cx);
                });
            }
        }
        cx.notify();
    }

    fn start_status_bridge(&self, cx: &mut Context<Self>) {
        let mut status = self.service.status();
        cx.spawn(async move |this, cx| {
            loop {
                let current = status.borrow_and_update().clone();
                if this
                    .update(cx, |this, cx| this.apply_status(current, cx))
                    .is_err()
                    || status.changed().await.is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn apply_status(&mut self, status: ScheduledBackupStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    pub fn backs_up_unattended(&self) -> bool {
        self.key.is_some() && self.enabled && self.status.state != ScheduleState::Suspended
    }

    pub fn is_running(&self) -> bool {
        self.status.state == ScheduleState::Running
    }

    pub fn transfer_held(&self) -> bool {
        self.exclusivity.is_held()
    }

    pub fn run_now(&self) {
        self.service.run_now();
    }

    pub fn prompt_view(&self) -> Option<AnyView> {
        self.prompt
            .as_ref()
            .map(|prompt| AnyView::from(prompt.modal.clone()))
    }

    fn persist<F>(&self, write: F, cx: &mut Context<Self>)
    where
        F: Future<Output = Result<(), StorageError>> + Send + 'static,
    {
        let service = self.service.clone();
        async_bridge::report_failure(
            &self.rt_handle,
            async move {
                write.await?;
                service.settings_changed();
                Ok::<(), StorageError>(())
            },
            ErrorSink::Toast,
            tr!("backup_schedule_persist_failed"),
            cx,
        );
    }

    pub(crate) fn commit_number(
        &mut self,
        number: ScheduleNumber,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let parsed = number.parse(text);
        let repo = self.settings_repo();
        let service = self.service.clone();
        let handle = self.rt_handle.clone();
        let Some(field) = self.numbers.iter_mut().find(|f| f.number == number) else {
            return;
        };
        field.invalid = parsed.is_none();
        let invalid = field.invalid;
        field
            .input
            .update(cx, |input, cx| input.set_invalid(invalid, cx));
        if let Some(value) = parsed {
            field.value = value;
            field.debounce.schedule_reporting(
                &handle,
                tr!("backup_schedule_persist_failed"),
                async move {
                    repo.set_string(number.storage_key(), &value.to_string())
                        .await?;
                    service.settings_changed();
                    Ok::<(), StorageError>(())
                },
                ErrorSink::Toast,
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn toggle_enabled(&mut self, cx: &mut Context<Self>) {
        if self.enabled {
            self.set_enabled(false, cx);
        } else if self.key.is_some() {
            self.set_enabled(true, cx);
        } else {
            self.open_prompt(PassphrasePurpose::ScheduleSetup, cx);
        }
        cx.notify();
    }

    fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        let repo = self.settings_repo();
        self.persist(
            async move {
                set_bool_setting(
                    repo.as_ref(),
                    reserved_keys::BACKUP_SCHEDULE_ENABLED,
                    enabled,
                )
                .await
            },
            cx,
        );
    }

    pub(crate) fn toggle_category(&mut self, category: BackupCategory, cx: &mut Context<Self>) {
        let Some(key) = include_key(category) else {
            return;
        };
        let include = !self.included.contains(&category);
        if include {
            self.included.push(category);
        } else {
            self.included.retain(|kept| *kept != category);
        }
        let repo = self.settings_repo();
        self.persist(
            async move { set_bool_setting(repo.as_ref(), key, include).await },
            cx,
        );
        cx.notify();
    }

    fn choose_folder(&mut self, cx: &mut Context<Self>) {
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::pick_folder(),
            |this, result, cx| this.folder_chosen(result, cx),
            cx,
        );
    }

    pub(crate) fn folder_chosen(
        &mut self,
        result: Result<PathBuf, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(path) => {
                let shown = path.to_string_lossy().into_owned();
                self.destination = Some(shown.clone());
                let repo = self.settings_repo();
                self.persist(
                    async move {
                        repo.set_string(reserved_keys::BACKUP_SCHEDULE_DESTINATION, &shown)
                            .await
                    },
                    cx,
                );
            }
            Err(message) if message == DIALOG_CANCELLED => {}
            Err(message) => {
                tracing::warn!(error = %message, "backup folder picker failed");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("backup_schedule_folder_failed", error = message.as_str()),
                );
            }
        }
        cx.notify();
    }

    pub(crate) fn reset_folder(&mut self, cx: &mut Context<Self>) {
        self.destination = None;
        let repo = self.settings_repo();
        self.persist(
            async move {
                repo.delete(reserved_keys::BACKUP_SCHEDULE_DESTINATION)
                    .await
                    .map(|_| ())
            },
            cx,
        );
        cx.notify();
    }

    fn open_prompt(&mut self, purpose: PassphrasePurpose, cx: &mut Context<Self>) {
        if self.prompt.is_some() {
            return;
        }
        let modal = cx.new(|cx| BackupPassphraseModal::with_purpose(purpose, cx));
        let sub = cx.subscribe(&modal, |this, _modal, event: &BackupPassphraseEvent, cx| {
            match event {
                BackupPassphraseEvent::Confirmed => this.prompt_confirmed(cx),
                BackupPassphraseEvent::Cancelled => this.prompt = None,
            }
            cx.notify();
        });
        self.prompt = Some(PassphrasePrompt {
            purpose,
            modal,
            _sub: sub,
        });
        cx.notify();
    }

    fn change_passphrase(&mut self, cx: &mut Context<Self>) {
        if self.key.is_some() {
            self.open_prompt(PassphrasePurpose::ScheduleChange, cx);
        }
    }

    fn prompt_confirmed(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = &self.prompt else {
            return;
        };
        let purpose = prompt.purpose;
        let modal = prompt.modal.clone();
        let (replacement, current) = modal.update(cx, |modal, cx| {
            (modal.entered_passphrase(cx), modal.entered_current(cx))
        });
        let Some(replacement) = replacement else {
            return;
        };
        let wrapped_identity = self.key.as_ref().map(|key| key.wrapped_identity.clone());
        let work = match (purpose, current, wrapped_identity) {
            (PassphrasePurpose::ScheduleChange, Some(current), Some(wrapped)) => KeyWork::Change {
                wrapped,
                current,
                replacement,
            },
            (PassphrasePurpose::ScheduleSetup, _, _) => KeyWork::Create(replacement),
            _ => return,
        };
        modal.update(cx, |modal, cx| modal.set_working(true, cx));
        let repo = self.settings_repo();
        let service = self.service.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let enable = matches!(work, KeyWork::Create(_));
                let key = tokio::task::spawn_blocking(move || work.run())
                    .await
                    .map_err(|error| KeyFailure::Other(error.to_string()))??;
                store_key(repo.as_ref(), &key, enable).await?;
                service.settings_changed();
                Ok::<ScheduleKey, KeyFailure>(key)
            },
            move |this, result, cx| this.key_finished(purpose, result, cx),
            cx,
        );
    }

    pub(crate) fn key_finished(
        &mut self,
        purpose: PassphrasePurpose,
        result: Result<ScheduleKey, KeyFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(key) => {
                self.key = Some(key);
                if purpose == PassphrasePurpose::ScheduleSetup {
                    self.enabled = true;
                }
                self.prompt = None;
                if purpose == PassphrasePurpose::ScheduleChange {
                    cx.push_toast(ToastKind::Success, tr!("backup_schedule_change_done"));
                }
            }
            Err(KeyFailure::WrongCurrent) => {
                if let Some(prompt) = &self.prompt {
                    prompt
                        .modal
                        .update(cx, |modal, cx| modal.reject_current(cx));
                }
            }
            Err(KeyFailure::Other(message)) => {
                tracing::warn!(error = %message, "backup schedule key setup failed");
                self.prompt = None;
                cx.push_toast(
                    ToastKind::Error,
                    tr!("backup_schedule_key_failed", error = message.as_str()),
                );
            }
        }
        cx.notify();
    }

    fn schedule_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let line = StatusLine::of(&self.status);
        let change = self.key.is_some().then(|| {
            ghost_button(tr!("backup_schedule_change_btn"), palette).on_click(
                "backup-schedule-change-passphrase",
                cx.listener(|this, _: &ClickEvent, _, cx| this.change_passphrase(cx)),
            )
        });
        let controls = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .children(change)
            .child(toggle(self.enabled, palette).on_click(
                "backup-schedule-enabled",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enabled(cx)),
            ));
        let row = setting_row(
            tr!("backup_schedule_label"),
            Some(line.text().into()),
            controls,
            palette,
            density,
        );
        if line.is_failure() {
            row.hint_color(palette.random)
        } else {
            row
        }
    }

    fn number_row(
        &self,
        number: ScheduleNumber,
        palette: &ForgePalette,
        density: Density,
    ) -> Option<SettingRow> {
        let field = self.numbers.iter().find(|f| f.number == number)?;
        let hint = if field.invalid {
            number.range_error()
        } else {
            number.hint()
        };
        let control = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .w(NUMBER_INPUT_W)
                    .min_w(NUMBER_INPUT_W)
                    .max_w(NUMBER_INPUT_W)
                    .child(field.input.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(number.unit(field.value)),
            );
        let row = setting_row(
            number.label(),
            Some(SharedString::from(hint)),
            control,
            palette,
            density,
        );
        Some(if field.invalid {
            row.hint_color(palette.random)
        } else {
            row
        })
    }

    fn category_item(
        &self,
        category: BackupCategory,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fixed = include_key(category).is_none();
        let checked = fixed || self.included.contains(&category);
        let label_color = if fixed {
            palette.text_muted
        } else {
            palette.text_primary
        };
        let mut item = div()
            .id(SharedString::from(format!(
                "backup-schedule-category-{category:?}"
            )))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .child(checkbox(checked, palette).disabled(fixed))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(label_color)
                    .child(category_label(category)),
            );
        if fixed {
            item = item.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("backup_schedule_category_database_reason")),
            );
        } else {
            let hover_bg = palette.base;
            item = item
                .cursor_pointer()
                .hover(move |style| style.bg(hover_bg))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.toggle_category(category, cx)
                }));
        }
        item
    }

    fn contents_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let mut items = div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        for category in BackupCategory::ALL {
            items = items.child(self.category_item(category, palette, density, cx));
        }
        setting_row(
            tr!("backup_schedule_contents_label"),
            Some(tr!("backup_schedule_contents_hint").into()),
            items,
            palette,
            density,
        )
        .stacked()
    }

    fn folder_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let shown = self
            .destination
            .clone()
            .unwrap_or_else(|| backups_dir(&self.data_dir).to_string_lossy().into_owned());
        let path_box = div()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .text_ellipsis()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(shown);
        let choose = ghost_button_with_icon(
            Icon::FolderOpen,
            tr!("backup_schedule_folder_choose"),
            palette,
        )
        .on_click(
            "backup-schedule-folder-choose",
            cx.listener(|this, _: &ClickEvent, _, cx| this.choose_folder(cx)),
        );
        let reset = self.destination.is_some().then(|| {
            ghost_button(tr!("backup_schedule_folder_reset"), palette).on_click(
                "backup-schedule-folder-reset",
                cx.listener(|this, _: &ClickEvent, _, cx| this.reset_folder(cx)),
            )
        });
        let control = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(path_box)
            .child(choose)
            .children(reset);
        setting_row(
            tr!("backup_schedule_folder_label"),
            Some(tr!("backup_schedule_folder_hint").into()),
            control,
            palette,
            density,
        )
        .stacked()
    }

    fn storage_used_row(&self, palette: &ForgePalette, density: Density) -> SettingRow {
        let mut items = div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Sm, density));
        match &self.status.storage_used {
            Some(used) => {
                for (part, bytes) in storage_parts(used) {
                    items = items.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Xs, density))
                            .child(
                                div()
                                    .font_family(body_family())
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_muted)
                                    .child(part.label()),
                            )
                            .child(
                                div()
                                    .font_family(mono_family())
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_primary)
                                    .child(fmt_bytes(bytes)),
                            ),
                    );
                }
            }
            None => {
                items = items.child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(tr!("backup_schedule_storage_used_unknown")),
                );
            }
        }
        setting_row(
            tr!("backup_schedule_storage_used_label"),
            Some(tr!("backup_schedule_storage_used_hint").into()),
            items,
            palette,
            density,
        )
        .stacked()
    }
}

enum KeyWork {
    Create(Passphrase),
    Change {
        wrapped: String,
        current: Passphrase,
        replacement: Passphrase,
    },
}

impl KeyWork {
    fn run(self) -> Result<ScheduleKey, KeyFailure> {
        match self {
            Self::Create(passphrase) => Ok(create_schedule_key(&passphrase)?),
            Self::Change {
                wrapped,
                current,
                replacement,
            } => Ok(change_schedule_passphrase(
                &wrapped,
                &current,
                &replacement,
            )?),
        }
    }
}

impl Render for BackupScheduleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let mut rows = setting_rows(&palette)
            .row(self.schedule_row(&palette, density, cx))
            .rows(
                self.number_row(ScheduleNumber::Interval, &palette, density)
                    .map(IntoElement::into_any_element),
            )
            .row(self.contents_row(&palette, density, cx))
            .row(self.folder_row(&palette, density, cx));
        for number in [ScheduleNumber::KeepNewest, ScheduleNumber::KeepDailyDays] {
            rows = rows.rows(
                self.number_row(number, &palette, density)
                    .map(IntoElement::into_any_element),
            );
        }
        rows.row(self.storage_used_row(&palette, density))
            .in_card(&palette)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
pub(crate) mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use forge_components::{Toasts, fmt_relative_time, tr};
    use forge_storage::settings::portability::{SettingPortability, portability_of};
    use forge_storage::{DataProvider, Language, reserved_keys};
    use forge_transfer::{
        BackupCategory, FailureCategory, FailureStreak, Passphrase, ScheduleKey, ScheduleState,
        ScheduledBackupHandle, ScheduledBackupParts, ScheduledBackupStatus, SystemBackupClock,
        TransferError, TransferExclusivity, change_schedule_passphrase, create_schedule_key,
        spawn_scheduled_backups,
    };
    use gpui::{AppContext as _, Entity, TestAppContext};
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::{
        BackupScheduleView, OPTIONAL_CATEGORIES, ScheduleNumber, ScheduleSnapshot, StatusLine,
        failure_reason, load_schedule,
    };
    use crate::backup_passphrase_modal::PassphrasePurpose;
    use crate::i18n::install_language;
    use crate::scheduled_run_labels::{local_stamp, system_offset_at};
    use crate::test_support::install_presentation;
    use crate::test_support::pack_flow::{PackWorld, pack_world};
    use crate::test_support::{pump, runtime};

    const SETTLE_DEADLINE: Duration = Duration::from_secs(30);
    const OLD_PASSPHRASE: &str = "the old schedule passphrase";
    const NEW_PASSPHRASE: &str = "a brand new schedule passphrase";
    const SCHEDULED_PREFIX: &str = "forge-scheduled-";

    pub(crate) struct ScheduleRig {
        pub(crate) rt: tokio::runtime::Runtime,
        pub(crate) world: PackWorld,
        pub(crate) destination: TempDir,
        pub(crate) view: Entity<BackupScheduleView>,
        _service: ScheduledBackupHandle,
    }

    pub(crate) fn key_for(passphrase: &str) -> ScheduleKey {
        create_schedule_key(&Passphrase::new(passphrase.to_owned())).unwrap()
    }

    pub(crate) fn unattended_seeds(key: &ScheduleKey) -> Vec<(&'static str, String)> {
        vec![
            (reserved_keys::BACKUP_SCHEDULE_ENABLED, "true".to_owned()),
            (reserved_keys::BACKUP_SCHEDULE_KEY, key.encode().unwrap()),
            (
                reserved_keys::BACKUP_SCHEDULE_LAST_SUCCESS_AT,
                just_now().format(&Rfc3339).unwrap(),
            ),
        ]
    }

    fn just_now() -> OffsetDateTime {
        OffsetDateTime::now_utc().replace_nanosecond(0).unwrap()
    }

    pub(crate) fn schedule_rig(
        cx: &mut TestAppContext,
        seeds: &[(&'static str, String)],
    ) -> ScheduleRig {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let rt = runtime();
        let world = pack_world(&rt);
        let destination = tempfile::tempdir().unwrap();
        let shown_destination = destination.path().to_string_lossy().into_owned();
        let exclusivity = TransferExclusivity::new();
        let provider = Arc::clone(world.provider());
        let service = rt.block_on(async {
            provider
                .set_string(
                    reserved_keys::BACKUP_SCHEDULE_DESTINATION,
                    &shown_destination,
                )
                .await
                .unwrap();
            for (key, value) in seeds {
                provider.set_string(key, value).await.unwrap();
            }
            spawn_scheduled_backups(ScheduledBackupParts {
                provider: Arc::clone(&provider),
                data_dir: world.data_dir(),
                exclusivity: exclusivity.clone(),
                clock: Arc::new(SystemBackupClock),
            })
        });
        let handle = rt.handle().clone();
        let data_dir = world.data_dir();
        let (view_service, view_exclusivity) = (service.clone(), exclusivity.clone());
        let view = cx.update(|cx| {
            cx.new(|cx| {
                BackupScheduleView::new(
                    provider as Arc<dyn DataProvider>,
                    handle,
                    view_service,
                    view_exclusivity,
                    data_dir,
                    cx,
                )
            })
        });
        let rig = ScheduleRig {
            rt,
            world,
            destination,
            view,
            _service: service,
        };
        let loaded = rig.settle(cx, |cx| {
            rig.view.read_with(cx, |view, _| {
                view.destination.as_deref() == Some(shown_destination.as_str())
            })
        });
        assert!(loaded, "the schedule settings never loaded");
        rig
    }

    impl ScheduleRig {
        pub(crate) fn settle(
            &self,
            cx: &mut TestAppContext,
            mut done: impl FnMut(&mut TestAppContext) -> bool,
        ) -> bool {
            let deadline = Instant::now() + SETTLE_DEADLINE;
            loop {
                cx.run_until_parked();
                if done(cx) {
                    return true;
                }
                if Instant::now() >= deadline {
                    return false;
                }
                self.rt
                    .block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            }
        }

        pub(crate) fn stored(&self, key: &str) -> Option<String> {
            self.rt
                .block_on(self.world.provider().get_string(key))
                .unwrap()
        }

        fn stored_key(&self) -> Option<ScheduleKey> {
            self.stored(reserved_keys::BACKUP_SCHEDULE_KEY)
                .map(|stored| ScheduleKey::decode(&stored).unwrap())
        }

        pub(crate) fn scheduled_bundles(&self) -> Vec<String> {
            bundles_in(self.destination.path())
        }

        fn prompt_purpose(&self, cx: &mut TestAppContext) -> Option<PassphrasePurpose> {
            self.view
                .read_with(cx, |view, _| view.prompt.as_ref().map(|p| p.purpose))
        }

        fn view_key(&self, cx: &mut TestAppContext) -> Option<ScheduleKey> {
            self.view.read_with(cx, |view, _| view.key.clone())
        }

        fn answer_prompt(
            &self,
            cx: &mut TestAppContext,
            current: Option<&str>,
            passphrase: &str,
            confirmation: &str,
        ) {
            let modal = self
                .view
                .read_with(cx, |view, _| view.prompt.as_ref().unwrap().modal.clone());
            modal.update(cx, |modal, cx| {
                modal.fill(current, passphrase, confirmation, cx);
                modal.submit(cx);
            });
        }

        fn settle_until_prompt_closes(&self, cx: &mut TestAppContext) {
            let closed = self.settle(cx, |cx| {
                self.view.read_with(cx, |view, _| view.prompt.is_none())
            });
            assert!(closed, "the passphrase prompt never closed");
        }
    }

    pub(crate) fn bundles_in(folder: &Path) -> Vec<String> {
        std::fs::read_dir(folder)
            .map(|entries| {
                entries
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .filter(|name| name.starts_with(SCHEDULED_PREFIX))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn opens_with(wrapped: &str, passphrase: &str) -> Result<(), TransferError> {
        change_schedule_passphrase(
            wrapped,
            &Passphrase::new(passphrase.to_owned()),
            &Passphrase::new(NEW_PASSPHRASE.to_owned()),
        )
        .map(|_| ())
    }

    fn unmarked(text: String) -> String {
        text.replace(['\u{2068}', '\u{2069}'], "")
    }

    #[test]
    fn number_input_accepts_exactly_the_settings_bounds() {
        for number in ScheduleNumber::ALL {
            let (min, max) = (*number.range().start(), *number.range().end());
            let mut cases = vec![
                (min.to_string(), Some(min)),
                (max.to_string(), Some(max)),
                (format!(" {max} "), Some(max)),
                ((max + 1).to_string(), None),
                ("abc".to_owned(), None),
                (String::new(), None),
                ("1.5".to_owned(), None),
                ("-1".to_owned(), None),
            ];
            if min > 0 {
                cases.push(((min - 1).to_string(), None));
            }
            for (typed, expected) in cases {
                assert_eq!(number.parse(&typed), expected, "{number:?} typed {typed:?}");
            }
        }
    }

    #[test]
    fn stored_number_falls_back_to_the_default_when_unreadable_and_clamps_when_out_of_range() {
        for number in ScheduleNumber::ALL {
            let (min, max) = (*number.range().start(), *number.range().end());
            let mut cases = vec![
                (None, number.default_value()),
                (Some("abc".to_owned()), number.default_value()),
                (Some((max + 1).to_string()), max),
                (Some(min.to_string()), min),
            ];
            if min > 0 {
                cases.push((Some((min - 1).to_string()), min));
            }
            for (stored, expected) in cases {
                assert_eq!(
                    number.value_from_stored(stored.as_deref()),
                    expected,
                    "{number:?} stored {stored:?}"
                );
            }
        }
    }

    #[test]
    fn status_line_lets_off_and_paused_win_over_a_failure_streak() {
        let at = just_now();
        let streak = FailureStreak {
            category: FailureCategory::DiskFull,
            since: at,
            attempts: 2,
        };
        let status = |state, failure| ScheduledBackupStatus {
            state,
            last_success_at: Some(at),
            failure,
            next_due: Some(at),
            prune_failures: 0,
            storage_used: None,
        };
        for (state, failure, expected) in [
            (
                ScheduleState::Disabled,
                Some(streak),
                StatusLine::Off {
                    last_success_at: Some(at),
                },
            ),
            (ScheduleState::Suspended, Some(streak), StatusLine::Paused),
            (ScheduleState::Running, Some(streak), StatusLine::Running),
            (
                ScheduleState::Idle,
                Some(streak),
                StatusLine::Failing {
                    category: FailureCategory::DiskFull,
                    since: at,
                    attempts: 2,
                },
            ),
            (
                ScheduleState::Idle,
                None,
                StatusLine::Scheduled {
                    last_success_at: Some(at),
                    next_due: Some(at),
                },
            ),
        ] {
            assert_eq!(
                StatusLine::of(&status(state, failure)),
                expected,
                "{state:?} {failure:?}"
            );
        }
    }

    #[test]
    fn status_text_shows_never_relative_last_next_and_failure_reason_in_each_locale() {
        let last = OffsetDateTime::now_utc() - time::Duration::hours(2);
        let next = OffsetDateTime::now_utc() + time::Duration::hours(1);
        for lang in [Language::En, Language::Uk] {
            install_language(lang);
            let never = unmarked(tr!("fmt_relative_never"));
            let relative = unmarked(fmt_relative_time(Some(last)));
            let next_stamp = local_stamp(next, system_offset_at(next));
            let since_stamp = local_stamp(last, system_offset_at(last));
            let reason = failure_reason(FailureCategory::DiskFull);
            let text = |line: StatusLine| unmarked(line.text());

            let off = text(StatusLine::Off {
                last_success_at: None,
            });
            let scheduled = text(StatusLine::Scheduled {
                last_success_at: Some(last),
                next_due: Some(next),
            });
            let last_only = text(StatusLine::Scheduled {
                last_success_at: Some(last),
                next_due: None,
            });
            let failing_once = text(StatusLine::Failing {
                category: FailureCategory::DiskFull,
                since: last,
                attempts: 1,
            });
            let failing_thrice = text(StatusLine::Failing {
                category: FailureCategory::DiskFull,
                since: last,
                attempts: 3,
            });

            assert!(off.contains(&never), "{lang:?} off: {off}");
            assert!(
                scheduled.contains(&relative) && scheduled.contains(&next_stamp),
                "{lang:?} scheduled: {scheduled}"
            );
            assert!(
                last_only.contains(&relative) && !last_only.contains(&next_stamp),
                "{lang:?} last only: {last_only}"
            );
            assert!(
                failing_once.contains(&since_stamp)
                    && failing_once.contains(&reason)
                    && failing_once != failing_thrice,
                "{lang:?} failing once: {failing_once}"
            );
            assert!(
                failing_thrice.contains(&reason) && failing_thrice.contains('3'),
                "{lang:?} failing thrice: {failing_thrice}"
            );
        }
    }

    #[test]
    fn every_failure_category_has_its_own_translated_reason_in_each_locale() {
        let categories = [
            FailureCategory::DestinationMissing,
            FailureCategory::DestinationNotWritable,
            FailureCategory::DiskFull,
            FailureCategory::DestinationInsideBackupScope,
            FailureCategory::KeyMissing,
            FailureCategory::KeyUnreadable,
            FailureCategory::SnapshotFailed,
            FailureCategory::StateNotSaved,
            FailureCategory::Interrupted,
        ];
        for lang in [Language::En, Language::Uk] {
            install_language(lang);
            let reasons: std::collections::BTreeSet<String> =
                categories.iter().map(|c| failure_reason(*c)).collect();

            assert!(
                reasons.len() == categories.len()
                    && reasons.iter().all(|r| !r.starts_with("backup_schedule_")),
                "{lang:?}: {reasons:?}"
            );
        }
    }

    #[test]
    fn number_hints_and_range_errors_carry_the_bounds_in_each_locale() {
        for lang in [Language::En, Language::Uk] {
            install_language(lang);
            for number in ScheduleNumber::ALL {
                let (min, max) = (*number.range().start(), *number.range().end());
                for text in [unmarked(number.hint()), unmarked(number.range_error())] {
                    assert!(
                        text.contains(&min.to_string()) && text.contains(&max.to_string()),
                        "{lang:?} {number:?}: {text}"
                    );
                }
            }
        }
    }

    #[gpui::test]
    fn unattended_backup_needs_a_key_an_enabled_schedule_and_no_pending_restore(
        cx: &mut TestAppContext,
    ) {
        let rig = schedule_rig(cx, &[]);
        let key = ScheduleKey {
            recipient: "recipient".to_owned(),
            wrapped_identity: "wrapped".to_owned(),
        };
        for (has_key, enabled, state, expected) in [
            (true, true, ScheduleState::Idle, true),
            (true, true, ScheduleState::Running, true),
            (true, true, ScheduleState::Suspended, false),
            (false, true, ScheduleState::Idle, false),
            (true, false, ScheduleState::Disabled, false),
        ] {
            let unattended = rig.view.update(cx, |view, cx| {
                view.apply_snapshot(
                    ScheduleSnapshot {
                        enabled,
                        key: has_key.then(|| key.clone()),
                        numbers: Vec::new(),
                        included: OPTIONAL_CATEGORIES.to_vec(),
                        destination: None,
                    },
                    cx,
                );
                let mut status = view.status.clone();
                status.state = state;
                view.apply_status(status, cx);
                view.backs_up_unattended()
            });

            assert_eq!(
                unattended, expected,
                "key={has_key} enabled={enabled} {state:?}"
            );
        }
    }

    #[gpui::test]
    fn turning_on_without_a_key_asks_for_a_passphrase_and_leaves_the_schedule_off(
        cx: &mut TestAppContext,
    ) {
        let rig = schedule_rig(cx, &[]);

        rig.view.update(cx, |view, cx| view.toggle_enabled(cx));
        cx.run_until_parked();

        let enabled = rig.view.read_with(cx, |view, _| view.enabled);
        assert_eq!(
            (
                rig.prompt_purpose(cx),
                enabled,
                rig.stored(reserved_keys::BACKUP_SCHEDULE_ENABLED)
            ),
            (Some(PassphrasePurpose::ScheduleSetup), false, None)
        );
    }

    #[gpui::test]
    fn setup_with_a_short_or_mismatched_passphrase_stores_no_key(cx: &mut TestAppContext) {
        let short: String = "s".repeat(forge_transfer::MIN_PASSPHRASE_CHARS - 1);
        for (passphrase, confirmation) in [
            (short.as_str(), short.as_str()),
            (NEW_PASSPHRASE, OLD_PASSPHRASE),
        ] {
            let rig = schedule_rig(cx, &[]);
            rig.view.update(cx, |view, cx| view.toggle_enabled(cx));
            cx.run_until_parked();

            rig.answer_prompt(cx, None, passphrase, confirmation);
            pump(&rig.rt);
            cx.run_until_parked();

            assert_eq!(
                (rig.prompt_purpose(cx), rig.view_key(cx), rig.stored_key()),
                (Some(PassphrasePurpose::ScheduleSetup), None, None),
                "{passphrase:?} / {confirmation:?}"
            );
        }
    }

    #[gpui::test]
    fn confirmed_setup_stores_the_key_pair_machine_locally_and_turns_the_schedule_on(
        cx: &mut TestAppContext,
    ) {
        let rig = schedule_rig(cx, &[]);
        rig.view.update(cx, |view, cx| view.toggle_enabled(cx));
        cx.run_until_parked();

        rig.answer_prompt(cx, None, NEW_PASSPHRASE, NEW_PASSPHRASE);
        rig.settle_until_prompt_closes(cx);

        let key = rig.view_key(cx).expect("a schedule key");
        let enabled = rig.view.read_with(cx, |view, _| view.enabled);
        assert_eq!(
            (
                rig.stored_key(),
                rig.stored(reserved_keys::BACKUP_SCHEDULE_ENABLED),
                enabled,
                portability_of(reserved_keys::BACKUP_SCHEDULE_KEY),
            ),
            (
                Some(key),
                Some("true".to_owned()),
                true,
                SettingPortability::MachineLocal,
            )
        );
    }

    #[gpui::test]
    fn the_stored_wrapped_identity_opens_only_with_the_setup_passphrase(cx: &mut TestAppContext) {
        let rig = schedule_rig(cx, &[]);
        rig.view.update(cx, |view, cx| view.toggle_enabled(cx));
        cx.run_until_parked();

        rig.answer_prompt(cx, None, NEW_PASSPHRASE, NEW_PASSPHRASE);
        rig.settle_until_prompt_closes(cx);

        let wrapped = rig
            .stored_key()
            .expect("a stored schedule key")
            .wrapped_identity;
        assert!(
            opens_with(&wrapped, NEW_PASSPHRASE).is_ok()
                && matches!(
                    opens_with(&wrapped, OLD_PASSPHRASE),
                    Err(TransferError::WrongPassphrase)
                )
        );
    }

    #[gpui::test]
    fn turning_the_schedule_off_keeps_its_key(cx: &mut TestAppContext) {
        let key = key_for(OLD_PASSPHRASE);
        let rig = schedule_rig(cx, &unattended_seeds(&key));

        rig.view.update(cx, |view, cx| view.toggle_enabled(cx));
        let persisted = rig.settle(cx, |_| {
            rig.stored(reserved_keys::BACKUP_SCHEDULE_ENABLED)
                .as_deref()
                == Some("false")
        });

        assert_eq!(
            (persisted, rig.view_key(cx), rig.stored_key()),
            (true, Some(key.clone()), Some(key))
        );
    }

    #[gpui::test]
    fn turning_back_on_with_a_stored_key_skips_the_passphrase_prompt(cx: &mut TestAppContext) {
        let key = key_for(OLD_PASSPHRASE);
        let mut seeds = unattended_seeds(&key);
        seeds[0].1 = "false".to_owned();
        let rig = schedule_rig(cx, &seeds);

        rig.view.update(cx, |view, cx| view.toggle_enabled(cx));
        let persisted = rig.settle(cx, |_| {
            rig.stored(reserved_keys::BACKUP_SCHEDULE_ENABLED)
                .as_deref()
                == Some("true")
        });

        assert_eq!((persisted, rig.prompt_purpose(cx)), (true, None));
    }

    #[gpui::test]
    fn an_unreadable_stored_key_asks_for_a_new_passphrase_when_turned_on(cx: &mut TestAppContext) {
        let rig = schedule_rig(
            cx,
            &[(
                reserved_keys::BACKUP_SCHEDULE_KEY,
                "{\"recipient\":".to_owned(),
            )],
        );

        rig.view.update(cx, |view, cx| view.toggle_enabled(cx));
        cx.run_until_parked();

        assert_eq!(
            rig.prompt_purpose(cx),
            Some(PassphrasePurpose::ScheduleSetup)
        );
    }

    #[gpui::test]
    fn a_wrong_current_passphrase_is_rejected_inline_and_keeps_the_old_key(
        cx: &mut TestAppContext,
    ) {
        let key = key_for(OLD_PASSPHRASE);
        let rig = schedule_rig(cx, &unattended_seeds(&key));
        rig.view.update(cx, |view, cx| view.change_passphrase(cx));
        cx.run_until_parked();

        rig.answer_prompt(
            cx,
            Some("not the old passphrase"),
            NEW_PASSPHRASE,
            NEW_PASSPHRASE,
        );
        let rejected = rig.settle(cx, |cx| {
            rig.view.read_with(cx, |view, cx| {
                view.prompt
                    .as_ref()
                    .is_some_and(|p| p.modal.read(cx).shown_current_error().is_some())
            })
        });

        assert_eq!(
            (
                rejected,
                rig.prompt_purpose(cx),
                rig.stored_key().map(|stored| stored.recipient)
            ),
            (
                true,
                Some(PassphrasePurpose::ScheduleChange),
                Some(key.recipient)
            )
        );
    }

    #[gpui::test]
    fn a_confirmed_change_replaces_the_key_with_one_that_opens_only_with_the_new_passphrase(
        cx: &mut TestAppContext,
    ) {
        let key = key_for(OLD_PASSPHRASE);
        let rig = schedule_rig(cx, &unattended_seeds(&key));
        rig.view.update(cx, |view, cx| view.change_passphrase(cx));
        cx.run_until_parked();

        rig.answer_prompt(cx, Some(OLD_PASSPHRASE), NEW_PASSPHRASE, NEW_PASSPHRASE);
        rig.settle_until_prompt_closes(cx);

        let ScheduleKey {
            recipient,
            wrapped_identity: wrapped,
        } = rig.stored_key().unwrap();
        assert!(
            recipient != key.recipient
                && rig.view_key(cx).map(|k| k.recipient) == Some(recipient)
                && opens_with(&wrapped, NEW_PASSPHRASE).is_ok()
                && matches!(
                    opens_with(&wrapped, OLD_PASSPHRASE),
                    Err(TransferError::WrongPassphrase)
                )
        );
    }

    #[gpui::test]
    fn an_out_of_range_number_is_flagged_and_never_saved(cx: &mut TestAppContext) {
        let rig = schedule_rig(cx, &[]);
        let below = (MIN_INTERVAL - 1).to_string();

        let flagged = rig.view.update(cx, |view, cx| {
            let mut flagged = Vec::new();
            for typed in [below.as_str(), "abc", ""] {
                view.commit_number(ScheduleNumber::Interval, typed, cx);
                let field = view
                    .numbers
                    .iter()
                    .find(|f| f.number == ScheduleNumber::Interval)
                    .unwrap();
                flagged.push((field.invalid, field.value));
            }
            view.commit_number(ScheduleNumber::KeepNewest, "30", cx);
            flagged
        });
        let later_saved = rig.settle(cx, |_| {
            rig.stored(reserved_keys::BACKUP_SCHEDULE_KEEP_NEWEST)
                .as_deref()
                == Some("30")
        });

        let default = ScheduleNumber::Interval.default_value();
        assert_eq!(
            (
                flagged,
                later_saved,
                rig.stored(reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES)
            ),
            (vec![(true, default); 3], true, None)
        );
    }

    const MIN_INTERVAL: u32 = forge_storage::settings::MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES;

    #[gpui::test]
    fn each_number_saves_its_boundary_values(cx: &mut TestAppContext) {
        let rig = schedule_rig(cx, &[]);

        let accepted = rig.view.update(cx, |view, cx| {
            let mut accepted = Vec::new();
            for number in ScheduleNumber::ALL {
                for bound in [*number.range().start(), *number.range().end()] {
                    view.commit_number(number, &bound.to_string(), cx);
                    let field = view.numbers.iter().find(|f| f.number == number).unwrap();
                    accepted.push((field.invalid, field.value == bound));
                }
            }
            accepted
        });
        let saved = rig.settle(cx, |_| {
            ScheduleNumber::ALL.iter().all(|number| {
                rig.stored(number.storage_key()) == Some(number.range().end().to_string())
            })
        });

        assert_eq!(
            (accepted, saved),
            (vec![(false, true); ScheduleNumber::ALL.len() * 2], true)
        );
    }

    #[gpui::test]
    fn saving_a_new_interval_moves_the_shown_next_due_time(cx: &mut TestAppContext) {
        let key = key_for(OLD_PASSPHRASE);
        let seeds = unattended_seeds(&key);
        let (_, stored_last) = seeds
            .iter()
            .find(|(key, _)| *key == reserved_keys::BACKUP_SCHEDULE_LAST_SUCCESS_AT)
            .unwrap();
        let last = OffsetDateTime::parse(stored_last, &Rfc3339).unwrap();
        let rig = schedule_rig(cx, &seeds);

        rig.view.update(cx, |view, cx| {
            view.commit_number(ScheduleNumber::Interval, "30", cx)
        });
        let moved = rig.settle(cx, |cx| {
            rig.view.read_with(cx, |view, _| {
                view.status.next_due == Some(last + time::Duration::minutes(30))
            })
        });

        assert!(moved);
    }

    #[gpui::test]
    fn an_unchecked_category_persists_and_is_left_out_on_reload(cx: &mut TestAppContext) {
        let rig = schedule_rig(cx, &[]);

        rig.view.update(cx, |view, cx| {
            view.toggle_category(BackupCategory::Media, cx)
        });
        let persisted = rig.settle(cx, |_| {
            rig.stored(reserved_keys::BACKUP_SCHEDULE_INCLUDE_MEDIA)
                .as_deref()
                == Some("false")
        });
        let reloaded = rig
            .rt
            .block_on(load_schedule(Arc::clone(rig.world.provider()) as _))
            .unwrap()
            .included;

        assert_eq!(
            (persisted, reloaded),
            (
                true,
                vec![
                    BackupCategory::Voices,
                    BackupCategory::Overlays,
                    BackupCategory::Assets
                ]
            )
        );
    }

    #[gpui::test]
    fn rechecking_a_category_includes_it_again(cx: &mut TestAppContext) {
        let rig = schedule_rig(cx, &[]);

        rig.view.update(cx, |view, cx| {
            view.toggle_category(BackupCategory::Voices, cx);
            view.toggle_category(BackupCategory::Voices, cx);
        });
        let persisted = rig.settle(cx, |_| {
            rig.stored(reserved_keys::BACKUP_SCHEDULE_INCLUDE_VOICES)
                .as_deref()
                == Some("true")
        });

        let included = rig.view.read_with(cx, |view, _| view.included.clone());
        assert!(persisted && included.contains(&BackupCategory::Voices));
    }

    #[gpui::test]
    fn the_database_category_cannot_be_unchecked(cx: &mut TestAppContext) {
        let rig = schedule_rig(cx, &[]);

        rig.view.update(cx, |view, cx| {
            view.toggle_category(BackupCategory::Database, cx)
        });
        cx.run_until_parked();

        let included = rig.view.read_with(cx, |view, _| view.included.clone());
        assert_eq!(included, OPTIONAL_CATEGORIES.to_vec());
    }

    fn measured(destination_outside_data_dir: u64) -> forge_transfer::StorageUsed {
        forge_transfer::StorageUsed {
            database: 1,
            media: 2,
            voices: 3,
            overlays: 4,
            assets: 5,
            logs: 6,
            backups: 7,
            destination_outside_data_dir,
        }
    }

    #[test]
    fn storage_parts_list_the_data_folder_parts_in_order_with_their_own_sizes() {
        let parts = super::storage_parts(&measured(0));

        assert_eq!(
            parts,
            vec![
                (super::StoragePart::Database, 1),
                (super::StoragePart::Media, 2),
                (super::StoragePart::Voices, 3),
                (super::StoragePart::Overlays, 4),
                (super::StoragePart::Assets, 5),
                (super::StoragePart::Logs, 6),
                (super::StoragePart::Backups, 7),
            ]
        );
    }

    #[test]
    fn the_outside_destination_part_appears_only_when_it_holds_bytes() {
        let shown: Vec<Option<(super::StoragePart, u64)>> = [0, 1, u64::MAX]
            .into_iter()
            .map(|outside| {
                super::storage_parts(&measured(outside))
                    .into_iter()
                    .find(|(part, _)| *part == super::StoragePart::Destination)
            })
            .collect();

        assert_eq!(
            shown,
            vec![
                None,
                Some((super::StoragePart::Destination, 1)),
                Some((super::StoragePart::Destination, u64::MAX)),
            ]
        );
    }

    #[test]
    fn an_empty_data_folder_still_lists_every_part_at_zero() {
        let parts = super::storage_parts(&forge_transfer::StorageUsed::default());

        assert_eq!(
            (parts.len(), parts.iter().all(|(_, bytes)| *bytes == 0)),
            (7, true)
        );
    }
}
