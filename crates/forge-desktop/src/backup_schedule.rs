use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SM, FONT_XS, ForgePalette, Icon, InputEvent,
    PushToast, Radius, SettingRow, Spacing, TextInput, ToastKind, body_family, checkbox,
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
    ScheduledBackupStatus, TransferError, TransferExclusivity, backups_dir,
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

fn failure_reason(category: FailureCategory) -> String {
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
        rows.in_card(&palette)
    }
}
