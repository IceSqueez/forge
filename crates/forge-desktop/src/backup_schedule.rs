use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, BORDER_THIN, DISABLED_OPACITY, DateShape, Density, FONT_SM, FONT_XS,
    ForgePalette, Icon, InputEvent, PushToast, Radius, SettingRow, Spacing, TextInput, ToastKind,
    body_family, checkbox, error_row_with_retry, fmt_bytes, fmt_display, fmt_display_relative,
    ghost_button, ghost_button_with_icon, mono_family, radius, setting_row, setting_rows, spacing,
    toggle, tr,
};
use forge_storage::settings::core_settings::{
    BACKUP_SCHEDULE_DESTINATION, BACKUP_SCHEDULE_ENABLED, BACKUP_SCHEDULE_INCLUDE_ASSETS,
    BACKUP_SCHEDULE_INCLUDE_MEDIA, BACKUP_SCHEDULE_INCLUDE_OVERLAYS,
    BACKUP_SCHEDULE_INCLUDE_VOICES, BACKUP_SCHEDULE_INTERVAL_MINUTES,
    BACKUP_SCHEDULE_KEEP_DAILY_DAYS, BACKUP_SCHEDULE_KEEP_NEWEST, BACKUP_SCHEDULE_KEY,
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
    Count, DataProvider, Flag, OptionalText, Setting, SettingsRepo, StorageError, read_setting,
    read_stored_setting, write_setting,
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

use crate::async_bridge::{
    self, DIALOG_CANCELLED, ErrorSink, Generation, Loadable, SLIDER_PERSIST_DEBOUNCE,
    SnapshotWriter, WriteOutcome,
};
use crate::backup_passphrase_modal::{
    BackupPassphraseEvent, BackupPassphraseModal, PassphrasePurpose,
};

const NUMBER_INPUT_W: Pixels = px(120.0);
const UNKNOWN_VALUE: &str = "-";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScheduleNumber {
    Interval,
    KeepNewest,
    KeepDailyDays,
}

impl ScheduleNumber {
    pub(crate) const ALL: [Self; 3] = [Self::Interval, Self::KeepNewest, Self::KeepDailyDays];

    fn declaration(self) -> &'static Setting<Count> {
        match self {
            Self::Interval => &BACKUP_SCHEDULE_INTERVAL_MINUTES,
            Self::KeepNewest => &BACKUP_SCHEDULE_KEEP_NEWEST,
            Self::KeepDailyDays => &BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
        }
    }

    pub(crate) fn storage_key(self) -> &'static str {
        self.declaration().name()
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

pub(crate) fn include_setting(category: BackupCategory) -> Option<&'static Setting<Flag>> {
    match category {
        BackupCategory::Database => None,
        BackupCategory::Media => Some(&BACKUP_SCHEDULE_INCLUDE_MEDIA),
        BackupCategory::Voices => Some(&BACKUP_SCHEDULE_INCLUDE_VOICES),
        BackupCategory::Overlays => Some(&BACKUP_SCHEDULE_INCLUDE_OVERLAYS),
        BackupCategory::Assets => Some(&BACKUP_SCHEDULE_INCLUDE_ASSETS),
    }
}

pub(crate) fn category_label(category: BackupCategory) -> String {
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
                last = fmt_display_relative(last_success_at)
            ),
            Self::Paused => tr!("backup_schedule_status_paused"),
            Self::Running => tr!("backup_schedule_status_running"),
            Self::Failing {
                category,
                since,
                attempts,
            } => tr!(
                "backup_schedule_status_failing",
                since = fmt_display(DateShape::DateTime, since),
                attempts = attempts,
                reason = failure_reason(category)
            ),
            Self::Scheduled {
                last_success_at,
                next_due: Some(next),
            } => tr!(
                "backup_schedule_status_scheduled",
                last = fmt_display_relative(last_success_at),
                next = fmt_display(DateShape::DateTime, next)
            ),
            Self::Scheduled {
                last_success_at,
                next_due: None,
            } => tr!(
                "backup_schedule_status_last",
                last = fmt_display_relative(last_success_at)
            ),
        }
    }
}

pub(crate) struct ScheduleSettings {
    pub enabled: bool,
    pub key: Option<ScheduleKey>,
    pub included: Vec<BackupCategory>,
    pub destination: Option<String>,
}

pub(crate) struct ScheduleSnapshot {
    pub settings: ScheduleSettings,
    pub numbers: Vec<(ScheduleNumber, u32)>,
}

async fn stored_text(
    repo: &dyn SettingsRepo,
    setting: &Setting<OptionalText>,
) -> Result<Option<String>, StorageError> {
    Ok(read_setting(repo, setting)
        .await?
        .filter(|value| !value.trim().is_empty()))
}

pub(crate) async fn load_schedule(
    repo: Arc<dyn SettingsRepo>,
) -> Result<ScheduleSnapshot, StorageError> {
    let repo = repo.as_ref();
    let mut numbers = Vec::with_capacity(ScheduleNumber::ALL.len());
    for number in ScheduleNumber::ALL {
        let stored = read_stored_setting(repo, number.declaration()).await?;
        numbers.push((number, stored.unwrap_or_else(|| number.default_value())));
    }
    let mut included = Vec::with_capacity(OPTIONAL_CATEGORIES.len());
    for category in OPTIONAL_CATEGORIES {
        if let Some(setting) = include_setting(category)
            && read_stored_setting(repo, setting)
                .await?
                .unwrap_or(DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY)
        {
            included.push(category);
        }
    }
    let key = stored_text(repo, &BACKUP_SCHEDULE_KEY)
        .await?
        .and_then(|stored| ScheduleKey::decode(&stored).ok());
    let enabled = read_stored_setting(repo, &BACKUP_SCHEDULE_ENABLED)
        .await?
        .unwrap_or(DEFAULT_BACKUP_SCHEDULE_ENABLED);
    Ok(ScheduleSnapshot {
        settings: ScheduleSettings {
            enabled,
            key,
            included,
            destination: stored_text(repo, &BACKUP_SCHEDULE_DESTINATION).await?,
        },
        numbers,
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
    write_setting(repo, &BACKUP_SCHEDULE_KEY, &Some(key.encode()?)).await?;
    if enable {
        write_setting(repo, &BACKUP_SCHEDULE_ENABLED, &true).await?;
    }
    Ok(())
}

struct NumberField {
    number: ScheduleNumber,
    input: Entity<TextInput>,
    pending_save: Generation,
    value: u32,
    invalid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManualRunProgress {
    Pending,
    Succeeded,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ManualRunWatch {
    baseline: Option<OffsetDateTime>,
    started: bool,
}

impl ManualRunWatch {
    pub(crate) fn since(last_success_at: Option<OffsetDateTime>) -> Self {
        Self {
            baseline: last_success_at,
            started: false,
        }
    }

    pub(crate) fn observe(&mut self, status: &ScheduledBackupStatus) -> ManualRunProgress {
        if status.last_success_at > self.baseline {
            ManualRunProgress::Succeeded
        } else if status.state == ScheduleState::Running {
            self.started = true;
            ManualRunProgress::Pending
        } else if self.started {
            ManualRunProgress::Ended
        } else {
            ManualRunProgress::Pending
        }
    }
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
    settings: Loadable<ScheduleSettings>,
    writers: HashMap<&'static str, SnapshotWriter>,
    status: ScheduledBackupStatus,
    manual_run: Option<ManualRunWatch>,
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
                TextInput::new(UNKNOWN_VALUE, cx)
                    .with_palette(palette)
                    .with_font_size(FONT_SM)
                    .read_only(true)
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
                pending_save: Generation::default(),
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
            settings: Loadable::Loading,
            writers: HashMap::new(),
            status,
            manual_run: None,
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
        self.settings = Loadable::Loading;
        self.lock_numbers(cx);
        async_bridge::run_async(
            &self.rt_handle,
            load_schedule(self.settings_repo()),
            |this, result, cx| match result {
                Ok(snapshot) => this.apply_snapshot(snapshot, cx),
                Err(error) => {
                    tracing::warn!(%error, "failed to load the backup schedule settings");
                    this.settings = Loadable::Failed(error.to_string());
                    cx.notify();
                }
            },
            cx,
        );
        cx.notify();
    }

    fn lock_numbers(&mut self, cx: &mut Context<Self>) {
        for field in &self.numbers {
            field.pending_save.next();
            field
                .input
                .update(cx, |input, cx| input.set_read_only(true, cx));
        }
    }

    pub(crate) fn apply_snapshot(&mut self, snapshot: ScheduleSnapshot, cx: &mut Context<Self>) {
        self.settings = Loadable::Loaded(snapshot.settings);
        for (number, value) in snapshot.numbers {
            if let Some(field) = self.numbers.iter_mut().find(|f| f.number == number) {
                field.value = value;
                field.invalid = false;
                field.input.update(cx, |input, cx| {
                    input.set_content(value.to_string(), cx);
                    input.set_invalid(false, cx);
                    input.set_read_only(false, cx);
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
        if let Some(watch) = self.manual_run.as_mut() {
            match watch.observe(&status) {
                ManualRunProgress::Pending => {}
                ManualRunProgress::Succeeded => {
                    self.manual_run = None;
                    cx.push_toast(ToastKind::Success, tr!("backup_schedule_run_done"));
                }
                ManualRunProgress::Ended => self.manual_run = None,
            }
        }
        self.status = status;
        cx.notify();
    }

    pub fn backs_up_unattended(&self) -> bool {
        self.settings
            .loaded()
            .is_some_and(|settings| settings.key.is_some() && settings.enabled)
            && self.status.state != ScheduleState::Suspended
    }

    pub fn is_running(&self) -> bool {
        self.status.state == ScheduleState::Running
    }

    pub fn transfer_held(&self) -> bool {
        self.exclusivity.is_held()
    }

    pub fn run_now(&mut self, cx: &mut Context<Self>) {
        self.manual_run = Some(ManualRunWatch::since(self.status.last_success_at));
        self.service.run_now();
        cx.notify();
    }

    pub fn prompt_view(&self) -> Option<AnyView> {
        self.prompt
            .as_ref()
            .map(|prompt| AnyView::from(prompt.modal.clone()))
    }

    fn persist<F>(&mut self, key: &'static str, write: F, cx: &mut Context<Self>)
    where
        F: Future<Output = Result<(), StorageError>> + Send + 'static,
    {
        let service = self.service.clone();
        let handle = self.rt_handle.clone();
        self.writers
            .entry(key)
            .or_insert_with(|| SnapshotWriter::new(&handle))
            .write(
                async move {
                    write.await?;
                    service.settings_changed();
                    Ok::<(), StorageError>(())
                },
                Self::persist_settled,
                cx,
            );
    }

    fn persist_settled(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        if let WriteOutcome::Failed(error) = outcome {
            ErrorSink::Toast.report(
                tr!(
                    "backup_schedule_persist_failed_reason",
                    reason = error.as_str()
                ),
                cx,
            );
            self.load(cx);
        }
    }

    pub(crate) fn commit_number(
        &mut self,
        number: ScheduleNumber,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.is_loaded() {
            return;
        }
        let parsed = number.parse(text);
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
            let ticket = field.pending_save.next();
            async_bridge::run_async(
                &self.rt_handle,
                async move { tokio::time::sleep(SLIDER_PERSIST_DEBOUNCE).await },
                move |this, (), cx| this.save_number(number, ticket, cx),
                cx,
            );
        }
        cx.notify();
    }

    fn save_number(&mut self, number: ScheduleNumber, ticket: u64, cx: &mut Context<Self>) {
        if !self.settings.is_loaded() {
            return;
        }
        let Some(field) = self.numbers.iter().find(|f| f.number == number) else {
            return;
        };
        if !field.pending_save.is_current(ticket) || field.invalid {
            return;
        }
        let value = field.value;
        let repo = self.settings_repo();
        self.persist(
            number.storage_key(),
            async move { write_setting(repo.as_ref(), number.declaration(), &value).await },
            cx,
        );
    }

    pub(crate) fn toggle_enabled(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.loaded() else {
            return;
        };
        if settings.enabled {
            self.set_enabled(false, cx);
        } else if settings.key.is_some() {
            self.set_enabled(true, cx);
        } else {
            self.open_prompt(PassphrasePurpose::ScheduleSetup, cx);
        }
        cx.notify();
    }

    fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let Some(settings) = self.settings.loaded_mut() else {
            return;
        };
        settings.enabled = enabled;
        let repo = self.settings_repo();
        self.persist(
            BACKUP_SCHEDULE_ENABLED.name(),
            async move { write_setting(repo.as_ref(), &BACKUP_SCHEDULE_ENABLED, &enabled).await },
            cx,
        );
    }

    pub(crate) fn toggle_category(&mut self, category: BackupCategory, cx: &mut Context<Self>) {
        let Some(setting) = include_setting(category) else {
            return;
        };
        let Some(settings) = self.settings.loaded_mut() else {
            return;
        };
        let include = !settings.included.contains(&category);
        if include {
            settings.included.push(category);
        } else {
            settings.included.retain(|kept| *kept != category);
        }
        let repo = self.settings_repo();
        self.persist(
            setting.name(),
            async move { write_setting(repo.as_ref(), setting, &include).await },
            cx,
        );
        cx.notify();
    }

    fn choose_folder(&mut self, cx: &mut Context<Self>) {
        if !self.settings.is_loaded() {
            return;
        }
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
                let Some(settings) = self.settings.loaded_mut() else {
                    return;
                };
                let shown = path.to_string_lossy().into_owned();
                settings.destination = Some(shown.clone());
                let repo = self.settings_repo();
                self.persist(
                    BACKUP_SCHEDULE_DESTINATION.name(),
                    async move {
                        write_setting(repo.as_ref(), &BACKUP_SCHEDULE_DESTINATION, &Some(shown))
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
        let Some(settings) = self.settings.loaded_mut() else {
            return;
        };
        settings.destination = None;
        let repo = self.settings_repo();
        self.persist(
            BACKUP_SCHEDULE_DESTINATION.name(),
            async move { write_setting(repo.as_ref(), &BACKUP_SCHEDULE_DESTINATION, &None).await },
            cx,
        );
        cx.notify();
    }

    fn open_prompt(&mut self, purpose: PassphrasePurpose, cx: &mut Context<Self>) {
        if self.prompt.is_some() || !self.settings.is_loaded() {
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
        if self
            .settings
            .loaded()
            .is_some_and(|settings| settings.key.is_some())
        {
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
        let Some(settings) = self.settings.loaded() else {
            return;
        };
        let wrapped_identity = settings
            .key
            .as_ref()
            .map(|key| key.wrapped_identity.clone());
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
                if let Some(settings) = self.settings.loaded_mut() {
                    settings.key = Some(key);
                    if purpose == PassphrasePurpose::ScheduleSetup {
                        settings.enabled = true;
                    }
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
        let settings = self.settings.loaded();
        let change = settings.and_then(|s| s.key.as_ref()).map(|_| {
            ghost_button(tr!("backup_schedule_change_btn"), palette).on_click(
                "backup-schedule-change-passphrase",
                cx.listener(|this, _: &ClickEvent, _, cx| this.change_passphrase(cx)),
            )
        });
        let enabled = settings.is_some_and(|s| s.enabled);
        let switch = toggle(enabled, palette);
        let switch = if settings.is_some() {
            div().child(switch.on_click(
                "backup-schedule-enabled",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enabled(cx)),
            ))
        } else {
            div().opacity(DISABLED_OPACITY).child(switch)
        };
        let controls = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .children(change)
            .child(switch);
        let row = setting_row(
            tr!("backup_schedule_label"),
            Some(line.text().into()),
            controls,
            palette,
            density,
        );
        if line.is_failure() {
            row.hint_color(palette.danger)
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
        let mut control = div()
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
        if !self.settings.is_loaded() {
            control = control.opacity(DISABLED_OPACITY);
        }
        let row = setting_row(
            number.label(),
            Some(SharedString::from(hint)),
            control,
            palette,
            density,
        );
        Some(if field.invalid {
            row.hint_color(palette.danger)
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
        let fixed = include_setting(category).is_none();
        let settings = self.settings.loaded();
        let checked = fixed || settings.is_some_and(|s| s.included.contains(&category));
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
        } else if settings.is_none() {
            item = item.opacity(DISABLED_OPACITY);
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
        let settings = self.settings.loaded();
        let destination = settings.and_then(|s| s.destination.clone());
        let has_destination = destination.is_some();
        let shown = match settings {
            Some(_) => destination
                .unwrap_or_else(|| backups_dir(&self.data_dir).to_string_lossy().into_owned()),
            None => UNKNOWN_VALUE.to_owned(),
        };
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
        .disabled(settings.is_none())
        .on_click(
            "backup-schedule-folder-choose",
            cx.listener(|this, _: &ClickEvent, _, cx| this.choose_folder(cx)),
        );
        let reset = has_destination.then(|| {
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
        let failure = self.settings.failure().map(|reason| {
            div()
                .w_full()
                .p(spacing(Spacing::Sm, density))
                .child(error_row_with_retry(
                    "backup-schedule-load-retry",
                    tr!("backup_schedule_load_failed", reason = reason),
                    &palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.load(cx)),
                    density,
                ))
                .into_any_element()
        });
        let mut rows = setting_rows(&palette)
            .rows(failure)
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
pub(crate) mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use forge_components::{DateShape, Toasts, fmt_display, fmt_display_relative, tr};
    use forge_storage::SettingPortability;
    use forge_storage::{
        DataProvider, Language, SettingsRepo, StorageError, read_stored_setting, reserved_keys,
    };
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
        BackupScheduleView, OPTIONAL_CATEGORIES, ScheduleNumber, ScheduleSettings,
        ScheduleSnapshot, StatusLine, failure_reason, load_schedule,
    };
    use crate::backup_passphrase_modal::PassphrasePurpose;
    use crate::i18n::install_language;
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
                view.settings
                    .loaded()
                    .and_then(|settings| settings.destination.as_deref())
                    == Some(shown_destination.as_str())
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
            self.view.read_with(cx, |view, _| {
                view.settings
                    .loaded()
                    .and_then(|settings| settings.key.clone())
            })
        }

        fn view_enabled(&self, cx: &mut TestAppContext) -> bool {
            self.view.read_with(cx, |view, _| {
                view.settings
                    .loaded()
                    .is_some_and(|settings| settings.enabled)
            })
        }

        fn view_included(&self, cx: &mut TestAppContext) -> Vec<BackupCategory> {
            self.view.read_with(cx, |view, _| {
                view.settings.loaded().unwrap().included.clone()
            })
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
    fn stored_number_defaults_when_absent_clamps_when_out_of_range_and_rejects_non_numbers() {
        let rt = crate::test_support::runtime();
        for number in ScheduleNumber::ALL {
            let (min, max) = (*number.range().start(), *number.range().end());
            let mut cases = vec![
                (None, Some(number.default_value())),
                (Some((max + 1).to_string()), Some(max)),
                (Some(format!(" {max} ")), Some(max)),
                (Some(min.to_string()), Some(min)),
                (Some("abc".to_owned()), None),
                (Some("1.5".to_owned()), None),
                (Some("-1".to_owned()), None),
                (Some(u64::MAX.to_string()), None),
            ];
            if min > 0 {
                cases.push((Some((min - 1).to_string()), Some(min)));
            }
            for (stored, expected) in cases {
                let (backend, _writes) = crate::test_support::test_backend();
                if let Some(raw) = &stored {
                    backend.seed(number.storage_key(), raw);
                }
                let loaded = rt
                    .block_on(read_stored_setting(backend.as_ref(), number.declaration()))
                    .ok()
                    .map(|value| value.unwrap_or_else(|| number.default_value()));
                assert_eq!(loaded, expected, "{number:?} stored {stored:?}");
            }
        }
    }

    #[test]
    fn a_blank_stored_number_is_a_load_failure_not_the_default() {
        let rt = crate::test_support::runtime();
        for number in ScheduleNumber::ALL {
            for blank in ["", "   "] {
                let (backend, _writes) = crate::test_support::test_backend();
                backend.seed(number.storage_key(), blank);

                let loaded = rt.block_on(load_schedule(backend));

                assert!(
                    matches!(
                        loaded,
                        Err(StorageError::UndecodableSetting { ref key }) if key == number.storage_key()
                    ),
                    "{number:?} blank {blank:?} loaded as a value"
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
            let relative = unmarked(fmt_display_relative(Some(last)));
            let next_stamp = fmt_display(DateShape::DateTime, next);
            let since_stamp = fmt_display(DateShape::DateTime, last);
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
                        settings: ScheduleSettings {
                            enabled,
                            key: has_key.then(|| key.clone()),
                            included: OPTIONAL_CATEGORIES.to_vec(),
                            destination: None,
                        },
                        numbers: Vec::new(),
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

        let enabled = rig.view_enabled(cx);
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
        let enabled = rig.view_enabled(cx);
        assert_eq!(
            (
                rig.stored_key(),
                rig.stored(reserved_keys::BACKUP_SCHEDULE_ENABLED),
                enabled,
                crate::settings_catalog::registered_settings_catalog()
                    .portability_of(reserved_keys::BACKUP_SCHEDULE_KEY),
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
        let _runtime = rig.rt.enter();
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
        let _runtime = rig.rt.enter();

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
        let _runtime = rig.rt.enter();

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
            .settings
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

        let included = rig.view_included(cx);
        assert!(persisted && included.contains(&BackupCategory::Voices));
    }

    #[gpui::test]
    fn the_database_category_cannot_be_unchecked(cx: &mut TestAppContext) {
        let rig = schedule_rig(cx, &[]);

        rig.view.update(cx, |view, cx| {
            view.toggle_category(BackupCategory::Database, cx)
        });
        cx.run_until_parked();

        let included = rig.view_included(cx);
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

    fn status_at(
        state: ScheduleState,
        last_success_at: Option<OffsetDateTime>,
    ) -> ScheduledBackupStatus {
        ScheduledBackupStatus {
            state,
            last_success_at,
            failure: None,
            next_due: None,
            prune_failures: 0,
            storage_used: None,
        }
    }

    #[test]
    fn manual_run_watch_follows_the_clicked_run_from_pickup_to_its_outcome() {
        use super::{ManualRunProgress as Progress, ManualRunWatch};
        let clicked = just_now();
        let later = Some(clicked + time::Duration::seconds(1));
        let earlier = Some(clicked - time::Duration::seconds(1));
        let base = Some(clicked);
        let cases: [(&str, Option<OffsetDateTime>, Vec<_>, Vec<_>); 6] = [
            (
                "first ever backup succeeds",
                None,
                vec![(ScheduleState::Running, None), (ScheduleState::Idle, later)],
                vec![Progress::Pending, Progress::Succeeded],
            ),
            (
                "run starts then fails",
                base,
                vec![(ScheduleState::Running, base), (ScheduleState::Idle, base)],
                vec![Progress::Pending, Progress::Ended],
            ),
            (
                "not picked up yet",
                base,
                vec![(ScheduleState::Idle, base), (ScheduleState::Idle, base)],
                vec![Progress::Pending, Progress::Pending],
            ),
            (
                "success lands before Running was ever seen",
                base,
                vec![(ScheduleState::Idle, later)],
                vec![Progress::Succeeded],
            ),
            (
                "an older stamp after the run is no success",
                base,
                vec![
                    (ScheduleState::Running, base),
                    (ScheduleState::Idle, earlier),
                ],
                vec![Progress::Pending, Progress::Ended],
            ),
            (
                "run ends suspended without a success",
                base,
                vec![
                    (ScheduleState::Running, base),
                    (ScheduleState::Suspended, base),
                ],
                vec![Progress::Pending, Progress::Ended],
            ),
        ];
        for (label, baseline, steps, expected) in cases {
            let mut watch = ManualRunWatch::since(baseline);

            let observed: Vec<Progress> = steps
                .into_iter()
                .map(|(state, last)| watch.observe(&status_at(state, last)))
                .collect();

            assert_eq!(observed, expected, "{label}");
        }
    }

    fn success_toasts(cx: &mut TestAppContext) -> Vec<String> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .filter(|toast| toast.kind == forge_components::ToastKind::Success)
                .map(|toast| unmarked(toast.message.to_string()))
                .collect()
        })
    }

    #[gpui::test]
    fn a_clicked_backup_now_toasts_once_its_run_saves_a_backup(cx: &mut TestAppContext) {
        let rig = schedule_rig(cx, &unattended_seeds(&key_for(OLD_PASSPHRASE)));
        let progress_loaded = rig.settle(cx, |cx| {
            rig.view
                .read_with(cx, |view, _| view.status.last_success_at.is_some())
        });
        assert!(
            progress_loaded,
            "the seeded last backup never reached the view"
        );

        rig.view.update(cx, |view, cx| view.run_now(cx));
        let toasted = rig.settle(cx, |cx| !success_toasts(cx).is_empty());

        assert_eq!(
            (toasted, success_toasts(cx), rig.scheduled_bundles().len()),
            (true, vec![tr!("backup_schedule_run_done")], 1)
        );
    }

    #[gpui::test]
    fn an_automatic_run_saves_a_backup_without_a_success_toast(cx: &mut TestAppContext) {
        let overdue = just_now() - time::Duration::days(2);
        let mut seeds = unattended_seeds(&key_for(OLD_PASSPHRASE));
        seeds[2].1 = overdue.format(&Rfc3339).unwrap();
        let rig = schedule_rig(cx, &seeds);

        let saved = rig.settle(cx, |cx| {
            rig.view.read_with(cx, |view, _| {
                view.status.last_success_at > Some(overdue) && !view.is_running()
            })
        });

        assert_eq!(
            (saved, success_toasts(cx), rig.scheduled_bundles().len()),
            (true, Vec::<String>::new(), 1)
        );
    }

    type Edit = fn(&mut BackupScheduleView, &mut gpui::Context<BackupScheduleView>);

    const STORED_DESTINATION: &str = "/stored/backups";
    const STORED_INTERVAL: &str = "120";
    const CHOSEN_DESTINATION: &str = "/chosen/backups";

    struct FakeSchedule {
        rt: tokio::runtime::Runtime,
        backend: Arc<crate::test_support::TestBackend>,
        writes: std::cell::RefCell<
            tokio::sync::mpsc::UnboundedReceiver<crate::test_support::SettingWrite>,
        >,
        service: ScheduledBackupHandle,
        _service_rt: tokio::runtime::Runtime,
        data_dir: TempDir,
    }

    impl FakeSchedule {
        fn new(cx: &mut TestAppContext, key: &ScheduleKey) -> Self {
            install_language(Language::En);
            install_presentation(cx);
            cx.update(|cx| cx.set_global(Toasts::new()));
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .start_paused(true)
                .build()
                .unwrap();
            let service_rt = runtime();
            let (backend, writes) = crate::test_support::test_backend();
            for (stored_key, value) in [
                (reserved_keys::BACKUP_SCHEDULE_ENABLED, "true".to_owned()),
                (reserved_keys::BACKUP_SCHEDULE_KEY, key.encode().unwrap()),
                (
                    reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES,
                    STORED_INTERVAL.to_owned(),
                ),
                (
                    reserved_keys::BACKUP_SCHEDULE_INCLUDE_MEDIA,
                    "false".to_owned(),
                ),
                (
                    reserved_keys::BACKUP_SCHEDULE_DESTINATION,
                    STORED_DESTINATION.to_owned(),
                ),
            ] {
                backend.seed(stored_key, &value);
            }
            let (service_backend, _) = crate::test_support::test_backend();
            let data_dir = tempfile::tempdir().unwrap();
            let service = {
                let _entered = service_rt.enter();
                spawn_scheduled_backups(ScheduledBackupParts {
                    provider: service_backend,
                    data_dir: data_dir.path().to_path_buf(),
                    exclusivity: TransferExclusivity::new(),
                    clock: Arc::new(SystemBackupClock),
                })
            };
            Self {
                rt,
                backend,
                writes: std::cell::RefCell::new(writes),
                service,
                _service_rt: service_rt,
                data_dir,
            }
        }

        fn open(&self, cx: &mut TestAppContext) -> Entity<BackupScheduleView> {
            let backend = Arc::clone(&self.backend) as Arc<dyn DataProvider>;
            let handle = self.rt.handle().clone();
            let service = self.service.clone();
            let data_dir = self.data_dir.path().to_path_buf();
            let view = cx.update(|cx| {
                cx.new(|cx| {
                    BackupScheduleView::new(
                        backend,
                        handle,
                        service,
                        TransferExclusivity::new(),
                        data_dir,
                        cx,
                    )
                })
            });
            self.settle(cx);
            view
        }

        fn settle(&self, cx: &mut TestAppContext) {
            for _ in 0..3 {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }

        fn past_the_save_debounce(&self, cx: &mut TestAppContext) {
            self.settle(cx);
            self.rt
                .block_on(tokio::time::advance(super::SLIDER_PERSIST_DEBOUNCE * 2));
            self.settle(cx);
        }

        fn stored(&self) -> std::collections::HashMap<String, String> {
            self.rt.block_on(self.backend.load_all()).unwrap()
        }

        fn untouched_since(&self, before: &std::collections::HashMap<String, String>) -> bool {
            let mut writes = self.writes.borrow_mut();
            let mut written = Vec::new();
            while let Ok(write) = writes.try_recv() {
                written.push(write);
            }
            written.is_empty() && self.stored() == *before
        }
    }

    fn edit_paths() -> [(&'static str, Edit); 11] {
        [
            ("type an interval", |v, cx| {
                v.commit_number(ScheduleNumber::Interval, "30", cx)
            }),
            ("type a newest-kept count", |v, cx| {
                v.commit_number(ScheduleNumber::KeepNewest, "30", cx)
            }),
            ("type a daily-kept count", |v, cx| {
                v.commit_number(ScheduleNumber::KeepDailyDays, "30", cx)
            }),
            ("flip the schedule switch", |v, cx| v.toggle_enabled(cx)),
            ("flip media", |v, cx| {
                v.toggle_category(BackupCategory::Media, cx)
            }),
            ("flip voices", |v, cx| {
                v.toggle_category(BackupCategory::Voices, cx)
            }),
            ("flip overlays", |v, cx| {
                v.toggle_category(BackupCategory::Overlays, cx)
            }),
            ("flip assets", |v, cx| {
                v.toggle_category(BackupCategory::Assets, cx)
            }),
            ("choose a folder", |v, cx| {
                v.folder_chosen(Ok(std::path::PathBuf::from(CHOSEN_DESTINATION)), cx)
            }),
            ("reset the folder", |v, cx| v.reset_folder(cx)),
            ("change the passphrase", |v, cx| v.change_passphrase(cx)),
        ]
    }

    fn view_failure(view: &Entity<BackupScheduleView>, cx: &mut TestAppContext) -> Option<String> {
        view.read_with(cx, |view, _| view.settings.failure().map(str::to_owned))
    }

    fn view_prompt(
        view: &Entity<BackupScheduleView>,
        cx: &mut TestAppContext,
    ) -> Option<PassphrasePurpose> {
        view.read_with(cx, |view, _| view.prompt.as_ref().map(|p| p.purpose))
    }

    #[gpui::test]
    fn no_edit_after_a_failed_load_writes_over_the_stored_schedule(cx: &mut TestAppContext) {
        let key = key_for(OLD_PASSPHRASE);
        let unreachable = forge_storage::StorageError::Connection {
            reason: crate::test_support::SETTINGS_READ_FAILED.to_owned(),
        }
        .to_string();
        let bad_number = forge_storage::StorageError::UndecodableSetting {
            key: ScheduleNumber::Interval.storage_key().to_owned(),
        }
        .to_string();
        let faults: [(&str, &str, Option<&str>); 4] = [
            (
                "unreachable store",
                reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES,
                None,
            ),
            (
                "undecodable number",
                reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES,
                Some("abc"),
            ),
            (
                "undecodable flag",
                reserved_keys::BACKUP_SCHEDULE_INCLUDE_MEDIA,
                Some("maybe"),
            ),
            (
                "blank flag",
                reserved_keys::BACKUP_SCHEDULE_ENABLED,
                Some(""),
            ),
        ];
        for (fault, faulty_key, stored) in faults {
            for (case, edit) in edit_paths() {
                let schedule = FakeSchedule::new(cx, &key);
                let reason = match stored {
                    Some(raw) => {
                        schedule.backend.seed(faulty_key, raw);
                        if faulty_key == reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES {
                            bad_number.clone()
                        } else {
                            format!("stored setting '{faulty_key}' cannot be decoded")
                        }
                    }
                    None => {
                        schedule
                            .backend
                            .fault_reads(faulty_key, crate::test_support::ReadFault::Fails);
                        unreachable.clone()
                    }
                };
                let before = schedule.stored();
                let view = schedule.open(cx);

                view.update(cx, edit);
                schedule.past_the_save_debounce(cx);

                assert_eq!(
                    (
                        schedule.untouched_since(&before),
                        view_failure(&view, cx),
                        view_prompt(&view, cx),
                    ),
                    (true, Some(reason), None),
                    "{fault}: {case}"
                );
            }
        }
    }

    #[gpui::test]
    fn no_edit_while_the_load_is_pending_writes_and_the_load_still_lands_intact(
        cx: &mut TestAppContext,
    ) {
        let key = key_for(OLD_PASSPHRASE);
        for (case, edit) in edit_paths() {
            let schedule = FakeSchedule::new(cx, &key);
            let gate = Arc::new(tokio::sync::Notify::new());
            schedule.backend.fault_reads(
                reserved_keys::BACKUP_SCHEDULE_DESTINATION,
                crate::test_support::ReadFault::Held(Arc::clone(&gate)),
            );
            let before = schedule.stored();
            let view = schedule.open(cx);

            view.update(cx, edit);
            schedule.past_the_save_debounce(cx);
            let untouched_while_pending = schedule.untouched_since(&before);
            schedule
                .backend
                .heal_reads(reserved_keys::BACKUP_SCHEDULE_DESTINATION);
            gate.notify_one();
            schedule.settle(cx);

            let landed = view.read_with(cx, |view, _| {
                view.settings.loaded().map(|settings| {
                    (
                        settings.enabled,
                        settings.included.contains(&BackupCategory::Media),
                        settings.destination.clone(),
                    )
                })
            });
            assert_eq!(
                (untouched_while_pending, landed),
                (
                    true,
                    Some((true, false, Some(STORED_DESTINATION.to_owned())))
                ),
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn retry_after_a_failed_load_restores_the_schedule_and_edits_persist(cx: &mut TestAppContext) {
        let schedule = FakeSchedule::new(cx, &key_for(OLD_PASSPHRASE));
        let _runtime = schedule.rt.enter();
        schedule.backend.fault_reads(
            reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES,
            crate::test_support::ReadFault::Fails,
        );
        let view = schedule.open(cx);
        schedule
            .backend
            .heal_reads(reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES);

        view.update(cx, |view, cx| view.load(cx));
        schedule.settle(cx);
        view.update(cx, |view, cx| {
            view.toggle_category(BackupCategory::Media, cx);
            view.commit_number(ScheduleNumber::KeepNewest, "30", cx);
        });
        schedule.past_the_save_debounce(cx);

        let stored = schedule.stored();
        let read = |key: &str| stored.get(key).cloned();
        assert_eq!(
            (
                read(reserved_keys::BACKUP_SCHEDULE_INCLUDE_MEDIA),
                read(reserved_keys::BACKUP_SCHEDULE_KEEP_NEWEST),
                read(reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES),
                read(reserved_keys::BACKUP_SCHEDULE_DESTINATION),
            ),
            (
                Some("true".to_owned()),
                Some("30".to_owned()),
                Some(STORED_INTERVAL.to_owned()),
                Some(STORED_DESTINATION.to_owned()),
            )
        );
    }

    #[gpui::test]
    fn a_failed_save_reports_the_error_and_reloads_the_stored_schedule(cx: &mut TestAppContext) {
        let key = key_for(OLD_PASSPHRASE);
        let cases: [(&str, &str, Edit); 3] = [
            (
                "flip media",
                reserved_keys::BACKUP_SCHEDULE_INCLUDE_MEDIA,
                |v, cx| v.toggle_category(BackupCategory::Media, cx),
            ),
            (
                "flip the schedule switch",
                reserved_keys::BACKUP_SCHEDULE_ENABLED,
                |v, cx| v.toggle_enabled(cx),
            ),
            (
                "choose a folder",
                reserved_keys::BACKUP_SCHEDULE_DESTINATION,
                |v, cx| v.folder_chosen(Ok(std::path::PathBuf::from(CHOSEN_DESTINATION)), cx),
            ),
        ];
        for (case, failing_key, edit) in cases {
            let schedule = FakeSchedule::new(cx, &key);
            schedule.backend.fail_writes(failing_key);
            let view = schedule.open(cx);

            view.update(cx, edit);
            schedule.settle(cx);

            let shown = view.read_with(cx, |view, _| {
                view.settings.loaded().map(|settings| {
                    (
                        settings.enabled,
                        settings.included.contains(&BackupCategory::Media),
                        settings.destination.clone(),
                    )
                })
            });
            assert_eq!(
                (cx.update(|cx| crate::test_support::error_toasts(cx)), shown),
                (1, Some((true, false, Some(STORED_DESTINATION.to_owned())))),
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn a_failed_number_save_reports_the_error_and_reloads_the_stored_number(
        cx: &mut TestAppContext,
    ) {
        let schedule = FakeSchedule::new(cx, &key_for(OLD_PASSPHRASE));
        let _runtime = schedule.rt.enter();
        schedule
            .backend
            .fail_writes(reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES);
        let view = schedule.open(cx);

        view.update(cx, |view, cx| {
            view.commit_number(ScheduleNumber::Interval, "90", cx)
        });
        schedule.past_the_save_debounce(cx);

        let shown = view.read_with(cx, |view, cx| {
            view.numbers
                .iter()
                .find(|field| field.number == ScheduleNumber::Interval)
                .map(|field| (field.value, field.input.read(cx).content().to_owned()))
        });
        assert_eq!(
            (cx.update(|cx| crate::test_support::error_toasts(cx)), shown),
            (
                1,
                Some((
                    STORED_INTERVAL.parse::<u32>().unwrap(),
                    STORED_INTERVAL.to_owned()
                ))
            )
        );
    }
}
