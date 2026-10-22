use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, ConfirmTone, Density, FONT_SM, FONT_XS, ForgePalette, InputEvent,
    OverlayPosition, PushToast, SettingRow, Spacing, TextInput, ToastKind, body_family,
    choice_trigger, confirm_modal, drive_overlay_focus, dropdown, dropdown_list, dropdown_row,
    fmt_bytes, overlay, secondary_button, setting_row, setting_rows, spacing, tr,
};
use forge_storage::settings::{
    MAX_SAFETY_COPIES_DELETE_AFTER_DAYS, MAX_SAFETY_COPIES_KEEP_NEWEST,
    MIN_SAFETY_COPIES_DELETE_AFTER_DAYS, MIN_SAFETY_COPIES_KEEP_NEWEST,
};
use forge_storage::{DataProvider, SettingsRepo};
use forge_transfer::{
    SafetyCopyMode, SafetyCopyReport, SafetyCopySettings, SafetyCopyUsage, ScheduledBackupHandle,
    TransferExclusivity, change_safety_copy_settings, clean_up_safety_copies, safety_copy_usage,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FocusHandle, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::async_bridge;

const MODE_SELECT_W: Pixels = px(200.0);
const NUMBER_INPUT_W: Pixels = px(96.0);

const MODES: [SafetyCopyMode; 3] = [
    SafetyCopyMode::KeepNewest,
    SafetyCopyMode::DeleteAfterDays,
    SafetyCopyMode::Never,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SafetyCopyNumber {
    KeepNewest,
    DeleteAfterDays,
}

impl SafetyCopyNumber {
    pub(crate) fn for_mode(mode: SafetyCopyMode) -> Option<Self> {
        match mode {
            SafetyCopyMode::KeepNewest => Some(Self::KeepNewest),
            SafetyCopyMode::DeleteAfterDays => Some(Self::DeleteAfterDays),
            SafetyCopyMode::Never => None,
        }
    }

    pub(crate) fn range(self) -> RangeInclusive<u32> {
        match self {
            Self::KeepNewest => MIN_SAFETY_COPIES_KEEP_NEWEST..=MAX_SAFETY_COPIES_KEEP_NEWEST,
            Self::DeleteAfterDays => {
                MIN_SAFETY_COPIES_DELETE_AFTER_DAYS..=MAX_SAFETY_COPIES_DELETE_AFTER_DAYS
            }
        }
    }

    pub(crate) fn parse(self, text: &str) -> Option<u32> {
        text.trim()
            .parse::<u32>()
            .ok()
            .filter(|value| self.range().contains(value))
    }

    pub(crate) fn value_in(self, settings: &SafetyCopySettings) -> u32 {
        match self {
            Self::KeepNewest => settings.keep_newest,
            Self::DeleteAfterDays => settings.delete_after_days,
        }
    }

    pub(crate) fn with_value(self, settings: SafetyCopySettings, value: u32) -> SafetyCopySettings {
        match self {
            Self::KeepNewest => SafetyCopySettings {
                keep_newest: value,
                ..settings
            },
            Self::DeleteAfterDays => SafetyCopySettings {
                delete_after_days: value,
                ..settings
            },
        }
    }

    fn unit(self, count: u32) -> String {
        match self {
            Self::KeepNewest => tr!("safety_copies_unit_copies", count = count),
            Self::DeleteAfterDays => tr!("safety_copies_unit_days", count = count),
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
}

fn mode_label(mode: SafetyCopyMode) -> String {
    match mode {
        SafetyCopyMode::KeepNewest => tr!("safety_copies_mode_keep_newest"),
        SafetyCopyMode::DeleteAfterDays => tr!("safety_copies_mode_delete_after_days"),
        SafetyCopyMode::Never => tr!("safety_copies_mode_never"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SweepOrigin {
    SettingChange,
    CleanUp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SafetyCopyNotice {
    Deferred(SweepOrigin),
    Failed(SweepOrigin, String),
    NotDeleted {
        count: usize,
        name: String,
        reason: String,
    },
    Freed {
        removed: u32,
        bytes: u64,
    },
    Saved,
    NothingToRemove,
}

pub(crate) fn notices(
    origin: SweepOrigin,
    result: &Result<SafetyCopyReport, String>,
) -> Vec<SafetyCopyNotice> {
    let report = match result {
        Ok(report) => report,
        Err(error) => return vec![SafetyCopyNotice::Failed(origin, error.clone())],
    };
    if report.deferred_by_transfer {
        return vec![SafetyCopyNotice::Deferred(origin)];
    }
    let mut shown = Vec::with_capacity(2);
    if report.removed > 0 {
        shown.push(SafetyCopyNotice::Freed {
            removed: report.removed,
            bytes: report.freed_bytes,
        });
    }
    if let Some(first) = report.failures.first() {
        shown.push(SafetyCopyNotice::NotDeleted {
            count: report.failures.len(),
            name: first.path.file_name().map_or_else(
                || first.path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            reason: first.reason.clone(),
        });
    }
    if shown.is_empty() {
        shown.push(match origin {
            SweepOrigin::SettingChange => SafetyCopyNotice::Saved,
            SweepOrigin::CleanUp => SafetyCopyNotice::NothingToRemove,
        });
    }
    shown
}

fn notice_toast(notice: &SafetyCopyNotice) -> (ToastKind, String) {
    match notice {
        SafetyCopyNotice::Deferred(SweepOrigin::SettingChange) => {
            (ToastKind::Warn, tr!("safety_copies_change_deferred"))
        }
        SafetyCopyNotice::Deferred(SweepOrigin::CleanUp) => {
            (ToastKind::Warn, tr!("safety_copies_clean_up_deferred"))
        }
        SafetyCopyNotice::Failed(SweepOrigin::SettingChange, error) => (
            ToastKind::Error,
            tr!("safety_copies_change_failed", error = error.as_str()),
        ),
        SafetyCopyNotice::Failed(SweepOrigin::CleanUp, error) => (
            ToastKind::Error,
            tr!("safety_copies_clean_up_failed", error = error.as_str()),
        ),
        SafetyCopyNotice::NotDeleted {
            count,
            name,
            reason,
        } => (
            ToastKind::Error,
            tr!(
                "safety_copies_not_deleted",
                count = *count,
                name = name.as_str(),
                reason = reason.as_str()
            ),
        ),
        SafetyCopyNotice::Freed { removed, bytes } => (
            ToastKind::Success,
            tr!(
                "safety_copies_freed",
                count = *removed,
                size = fmt_bytes(*bytes)
            ),
        ),
        SafetyCopyNotice::Saved => (ToastKind::Success, tr!("safety_copies_saved")),
        SafetyCopyNotice::NothingToRemove => {
            (ToastKind::Info, tr!("safety_copies_nothing_to_remove"))
        }
    }
}

struct NumberField {
    number: SafetyCopyNumber,
    input: Entity<TextInput>,
    invalid: bool,
}

pub struct SafetyCopiesView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    exclusivity: TransferExclusivity,
    scheduled: ScheduledBackupHandle,
    data_dir: PathBuf,
    settings: SafetyCopySettings,
    numbers: [NumberField; 2],
    usage: Option<SafetyCopyUsage>,
    menu_open: bool,
    clean_up_pending: bool,
    cleaning: bool,
    changed_locally: bool,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
    _subs: Vec<Subscription>,
}

impl SafetyCopiesView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        exclusivity: TransferExclusivity,
        scheduled: ScheduledBackupHandle,
        data_dir: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = SafetyCopySettings::default();
        let palette = cx.palette();
        let mut subs = Vec::with_capacity(2);
        let numbers = [
            SafetyCopyNumber::KeepNewest,
            SafetyCopyNumber::DeleteAfterDays,
        ]
        .map(|number| {
            let input = cx.new(|cx| {
                TextInput::new(number.value_in(&settings).to_string(), cx)
                    .with_palette(palette)
                    .with_font_size(FONT_SM)
            });
            subs.push(
                cx.subscribe(&input, move |this, _input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                        this.commit_number(number, cx);
                    }
                }),
            );
            NumberField {
                number,
                input,
                invalid: false,
            }
        });
        let mut view = Self {
            backend,
            rt_handle,
            exclusivity,
            scheduled,
            data_dir,
            settings,
            numbers,
            usage: None,
            menu_open: false,
            clean_up_pending: false,
            cleaning: false,
            changed_locally: false,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
            _subs: subs,
        };
        view.load(cx);
        view.refresh_usage(cx);
        view
    }

    fn settings_repo(&self) -> Arc<dyn SettingsRepo> {
        Arc::clone(&self.backend) as Arc<dyn SettingsRepo>
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let repo = self.settings_repo();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                SafetyCopySettings::load(repo.as_ref())
                    .await
                    .map_err(|error| error.to_string())
            },
            |this, result, cx| match result {
                Ok(_) if this.changed_locally => {}
                Ok(settings) => this.apply_settings(settings, cx),
                Err(error) => {
                    tracing::warn!(%error, "failed to load the safety copy settings");
                }
            },
            cx,
        );
    }

    pub(crate) fn apply_settings(&mut self, settings: SafetyCopySettings, cx: &mut Context<Self>) {
        self.settings = settings;
        for field in &mut self.numbers {
            field.invalid = false;
            let shown = field.number.value_in(&settings).to_string();
            field.input.update(cx, |input, cx| {
                input.set_content(shown, cx);
                input.set_invalid(false, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn refresh_usage(&mut self, cx: &mut Context<Self>) {
        let data_dir = self.data_dir.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                safety_copy_usage(&data_dir)
                    .await
                    .map_err(|error| error.to_string())
            },
            |this, result, cx| this.apply_usage(result, cx),
            cx,
        );
    }

    pub(crate) fn apply_usage(
        &mut self,
        result: Result<SafetyCopyUsage, String>,
        cx: &mut Context<Self>,
    ) {
        self.usage = match result {
            Ok(usage) => Some(usage),
            Err(error) => {
                tracing::warn!(%error, "failed to measure the safety copies");
                None
            }
        };
        cx.notify();
    }

    fn toggle_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = !self.menu_open;
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu_open {
            self.menu_open = false;
            cx.notify();
        }
    }

    pub(crate) fn pick_mode(&mut self, mode: SafetyCopyMode, cx: &mut Context<Self>) {
        self.menu_open = false;
        if mode != self.settings.mode {
            self.change_settings(
                SafetyCopySettings {
                    mode,
                    ..self.settings
                },
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn commit_number(&mut self, number: SafetyCopyNumber, cx: &mut Context<Self>) {
        let Some(index) = self.numbers.iter().position(|f| f.number == number) else {
            return;
        };
        let typed = self.numbers[index].input.read(cx).content().to_owned();
        let parsed = number.parse(&typed);
        let field = &mut self.numbers[index];
        field.invalid = parsed.is_none();
        let invalid = field.invalid;
        field
            .input
            .update(cx, |input, cx| input.set_invalid(invalid, cx));
        if let Some(value) = parsed {
            field
                .input
                .update(cx, |input, cx| input.set_content(value.to_string(), cx));
            if value != number.value_in(&self.settings) {
                self.change_settings(number.with_value(self.settings, value), cx);
            }
        }
        cx.notify();
    }

    fn change_settings(&mut self, changed: SafetyCopySettings, cx: &mut Context<Self>) {
        self.settings = changed;
        self.changed_locally = true;
        let repo = self.settings_repo();
        let data_dir = self.data_dir.clone();
        let exclusivity = self.exclusivity.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                change_safety_copy_settings(repo.as_ref(), &data_dir, &exclusivity, changed)
                    .await
                    .map_err(|error| error.to_string())
            },
            |this, result, cx| this.sweep_finished(SweepOrigin::SettingChange, result, cx),
            cx,
        );
        cx.notify();
    }

    fn request_clean_up(&mut self, cx: &mut Context<Self>) {
        if !self.clean_up_blocked() {
            self.clean_up_pending = true;
        }
        cx.notify();
    }

    fn cancel_clean_up(&mut self, cx: &mut Context<Self>) {
        self.clean_up_pending = false;
        cx.notify();
    }

    pub(crate) fn clean_up(&mut self, cx: &mut Context<Self>) {
        self.clean_up_pending = false;
        if self.cleaning {
            cx.notify();
            return;
        }
        self.cleaning = true;
        let data_dir = self.data_dir.clone();
        let exclusivity = self.exclusivity.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                clean_up_safety_copies(&data_dir, &exclusivity)
                    .await
                    .map_err(|error| error.to_string())
            },
            |this, result, cx| {
                this.cleaning = false;
                this.sweep_finished(SweepOrigin::CleanUp, result, cx);
            },
            cx,
        );
        cx.notify();
    }

    pub(crate) fn sweep_finished(
        &mut self,
        origin: SweepOrigin,
        result: Result<SafetyCopyReport, String>,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = &result {
            tracing::warn!(%error, ?origin, "safety copy sweep failed");
        }
        if result
            .as_ref()
            .is_ok_and(|report| report.removed > 0 || !report.failures.is_empty())
        {
            self.scheduled.storage_changed();
        }
        for notice in notices(origin, &result) {
            let (kind, message) = notice_toast(&notice);
            cx.push_toast(kind, message);
        }
        self.refresh_usage(cx);
        cx.notify();
    }

    pub(crate) fn transfer_lock_changed(&mut self, cx: &mut Context<Self>) {
        if !self.exclusivity.is_held() {
            self.refresh_usage(cx);
        }
        cx.notify();
    }

    pub(crate) fn clean_up_blocked(&self) -> bool {
        self.cleaning
            || self.exclusivity.is_held()
            || self.usage.is_some_and(|usage| usage.count == 0)
    }

    fn policy_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let active = SafetyCopyNumber::for_mode(self.settings.mode)
            .and_then(|number| self.numbers.iter().find(|f| f.number == number));
        let mut control = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(self.mode_select(palette, cx));
        if let Some(field) = active {
            control = control
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
                        .child(field.number.unit(field.number.value_in(&self.settings))),
                );
        }
        let invalid = active.filter(|field| field.invalid);
        let hint = invalid.map_or_else(
            || tr!("safety_copies_hint"),
            |field| field.number.range_error(),
        );
        let row = setting_row(
            tr!("safety_copies_label"),
            Some(SharedString::from(hint)),
            control,
            palette,
            density,
        );
        if invalid.is_some() {
            row.hint_color(palette.random)
        } else {
            row
        }
    }

    fn mode_select(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let trigger = choice_trigger(
            "safety-copies-mode-trigger",
            mode_label(self.settings.mode).into(),
            true,
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_menu(cx)),
        );
        let mut field = div()
            .relative()
            .w(MODE_SELECT_W)
            .min_w(MODE_SELECT_W)
            .max_w(MODE_SELECT_W)
            .child(trigger);
        if self.menu_open {
            field = field.child(self.mode_menu(palette, cx));
        }
        field.into_any_element()
    }

    fn mode_menu(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = dropdown_list("safety-copies-mode-list", palette);
        for (idx, mode) in MODES.into_iter().enumerate() {
            panel = panel.child(
                dropdown_row(("safety-copies-mode", idx), mode_label(mode), palette)
                    .current(mode == self.settings.mode)
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_mode(mode, cx)),
                    ),
            );
        }
        let view = cx.entity();
        dropdown(panel)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    fn usage_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let usage = self.usage.map_or_else(
            || tr!("safety_copies_usage_unknown"),
            |usage| {
                tr!(
                    "safety_copies_usage",
                    count = usage.count,
                    size = fmt_bytes(usage.bytes)
                )
            },
        );
        let hint = if self.exclusivity.is_held() && !self.cleaning {
            tr!("settings_storage_transfer_busy_hint")
        } else {
            usage
        };
        let label = if self.cleaning {
            tr!("safety_copies_clean_up_btn_busy")
        } else {
            tr!("safety_copies_clean_up_btn")
        };
        let button = secondary_button(label, palette)
            .busy(self.cleaning)
            .disabled(self.clean_up_blocked())
            .on_click(
                "safety-copies-clean-up",
                cx.listener(|this, _: &ClickEvent, _, cx| this.request_clean_up(cx)),
            );
        setting_row(
            tr!("safety_copies_usage_label"),
            Some(hint.into()),
            button,
            palette,
            density,
        )
    }

    fn clean_up_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.clean_up_pending {
            return None;
        }
        let modal = confirm_modal(
            tr!("safety_copies_clean_up_confirm_title"),
            tr!("safety_copies_clean_up_confirm_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .on_cancel(
            "safety-copies-clean-up-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_clean_up(cx)),
        )
        .on_confirm(
            "safety-copies-clean-up-confirm",
            tr!("safety_copies_clean_up_confirm_action"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.clean_up(cx)),
        );
        let weak = cx.entity().downgrade();
        Some(
            overlay(modal, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("safety-copies-clean-up-dismiss", move |_window, cx| {
                    let _ = weak.update(cx, |this, cx| this.cancel_clean_up(cx));
                })
                .into_any_element(),
        )
    }
}

impl Render for SafetyCopiesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.menu_open,
            &self.overlay_focus,
            &mut self.focus_restore,
            window,
            cx,
        );
        let palette = cx.palette();
        let density = cx.density();
        let rows = setting_rows(&palette)
            .row(self.policy_row(&palette, density, cx))
            .row(self.usage_row(&palette, density, cx))
            .in_card(&palette);
        div()
            .child(rows)
            .children(self.clean_up_overlay(&palette, cx))
    }
}
