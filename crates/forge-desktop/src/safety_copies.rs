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
            row.hint_color(palette.danger)
        } else {
            row
        }
    }

    fn mode_select(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let trigger = choice_trigger(
            "safety-copies-mode-trigger",
            mode_label(self.settings.mode).into(),
            true,
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_menu(cx)),
            density,
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
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the view may close before the result arrives"
                    )]
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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use forge_components::Toasts;
    use forge_storage::{Language, reserved_keys};
    use forge_transfer::{
        SafetyCopyFailure, ScheduledBackupParts, SystemBackupClock, backup_file_stem, backups_dir,
        spawn_scheduled_backups,
    };
    use gpui::TestAppContext;
    use time::OffsetDateTime;

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::install_presentation;
    use crate::test_support::pack_flow::{PackWorld, pack_world};
    use crate::test_support::runtime;

    const SETTLE_GUARD: Duration = Duration::from_secs(30);
    const RUNTIME_TURN: Duration = Duration::from_millis(1);
    const STEM_PREFIX: &str = "forge-backup";
    const PRE_PACK: &str = "forge-pre-pack";
    const ARCHIVE: &str = "tar.gz";
    const COPY_BYTES: usize = 64;
    const SETTLE_ROUNDS: usize = 20;

    struct Rig {
        rt: tokio::runtime::Runtime,
        world: PackWorld,
        exclusivity: TransferExclusivity,
        scheduled: ScheduledBackupHandle,
        view: Entity<SafetyCopiesView>,
    }

    fn rig(cx: &mut TestAppContext, seeds: &[(&str, &str)], copies: usize) -> Rig {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let rt = runtime();
        let world = pack_world(&rt);
        let provider = Arc::clone(world.provider());
        let exclusivity = TransferExclusivity::new();
        let backups = backups_dir(&world.data_dir());
        std::fs::create_dir_all(&backups).unwrap();
        for n in 0..copies {
            let age = time::Duration::hours(i64::try_from(n).unwrap() + 1);
            let stem = backup_file_stem(OffsetDateTime::now_utc() - age, &format!("aaaa{n:02}"));
            let name = format!(
                "{PRE_PACK}{}.{ARCHIVE}",
                stem.strip_prefix(STEM_PREFIX).unwrap()
            );
            std::fs::write(backups.join(name), vec![0_u8; COPY_BYTES]).unwrap();
        }
        let scheduled = rt.block_on(async {
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
        let (handle, data_dir) = (rt.handle().clone(), world.data_dir());
        let (view_exclusivity, view_scheduled) = (exclusivity.clone(), scheduled.clone());
        let view = cx.update(|cx| {
            cx.new(|cx| {
                SafetyCopiesView::new(
                    provider as Arc<dyn DataProvider>,
                    handle,
                    view_exclusivity,
                    view_scheduled,
                    data_dir,
                    cx,
                )
            })
        });
        let rig = Rig {
            rt,
            world,
            exclusivity,
            scheduled,
            view,
        };
        let expected = u32::try_from(copies).unwrap();
        assert!(rig.settle(cx, |cx| rig.usage(cx).is_some_and(|u| u.count == expected)));
        rig
    }

    impl Rig {
        fn settle(
            &self,
            cx: &mut TestAppContext,
            mut done: impl FnMut(&mut TestAppContext) -> bool,
        ) -> bool {
            let started = Instant::now();
            loop {
                cx.run_until_parked();
                if done(cx) {
                    return true;
                }
                if started.elapsed() > SETTLE_GUARD {
                    return false;
                }
                self.rt
                    .block_on(async { tokio::time::sleep(RUNTIME_TURN).await });
            }
        }

        fn usage(&self, cx: &mut TestAppContext) -> Option<SafetyCopyUsage> {
            self.view.read_with(cx, |view, _| view.usage)
        }

        fn stored(&self, key: &str) -> Option<String> {
            self.rt
                .block_on(self.world.provider().get_string(key))
                .unwrap()
        }

        fn copies_on_disk(&self) -> usize {
            std::fs::read_dir(backups_dir(&self.world.data_dir()))
                .unwrap()
                .filter(|entry| {
                    entry
                        .as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(PRE_PACK)
                })
                .count()
        }

        fn type_and_commit(&self, cx: &mut TestAppContext, number: SafetyCopyNumber, text: &str) {
            self.view.update(cx, |view, cx| {
                let field = view.numbers.iter().find(|f| f.number == number).unwrap();
                field
                    .input
                    .update(cx, |input, cx| input.set_content(text.to_owned(), cx));
                view.commit_number(number, cx);
            });
        }

        fn shown(&self, cx: &mut TestAppContext, number: SafetyCopyNumber) -> (String, bool) {
            self.view.read_with(cx, |view, cx| {
                let field = view.numbers.iter().find(|f| f.number == number).unwrap();
                (field.input.read(cx).content().to_owned(), field.invalid)
            })
        }

        fn settings(&self, cx: &mut TestAppContext) -> SafetyCopySettings {
            self.view.read_with(cx, |view, _| view.settings)
        }

        fn toasts(&self, cx: &mut TestAppContext) -> usize {
            cx.update(|cx| cx.global::<Toasts>().items().len())
        }

        fn fence_with_a_real_change(&self, cx: &mut TestAppContext) -> usize {
            self.view
                .update(cx, |view, cx| view.pick_mode(SafetyCopyMode::Never, cx));
            assert!(self.settle(cx, |cx| {
                self.stored(reserved_keys::SAFETY_COPIES_MODE) == Some("never".to_owned())
                    && self.toasts(cx) > 0
            }));
            self.toasts(cx)
        }

        fn storage_backups_bytes(&self) -> Option<u64> {
            self.scheduled
                .status()
                .borrow()
                .storage_used
                .map(|used| used.backups)
        }
    }

    fn settings(
        mode: SafetyCopyMode,
        keep_newest: u32,
        delete_after_days: u32,
    ) -> SafetyCopySettings {
        SafetyCopySettings {
            mode,
            keep_newest,
            delete_after_days,
        }
    }

    fn report(removed: u32, failures: &[&str], deferred: bool) -> SafetyCopyReport {
        SafetyCopyReport {
            removed,
            freed_bytes: u64::from(removed),
            failures: failures
                .iter()
                .map(|name| SafetyCopyFailure {
                    path: Path::new("/data/backups").join(name),
                    reason: "denied".to_owned(),
                })
                .collect(),
            deferred_by_transfer: deferred,
        }
    }

    #[test]
    fn each_number_accepts_its_bounds_and_rejects_everything_outside_them() {
        for number in [
            SafetyCopyNumber::KeepNewest,
            SafetyCopyNumber::DeleteAfterDays,
        ] {
            let (min, max) = (*number.range().start(), *number.range().end());
            for (text, expected) in [
                (min.to_string(), Some(min)),
                (max.to_string(), Some(max)),
                (format!(" {max} "), Some(max)),
                ((min - 1).to_string(), None),
                ((max + 1).to_string(), None),
                (String::new(), None),
                ("   ".to_owned(), None),
                ("-1".to_owned(), None),
                ("five".to_owned(), None),
                ("1e3".to_owned(), None),
                ("99999999999".to_owned(), None),
            ] {
                assert_eq!(number.parse(&text), expected, "{number:?} {text:?}");
            }
        }
    }

    #[test]
    fn setting_one_number_leaves_the_mode_and_the_other_number_alone() {
        let before = settings(SafetyCopyMode::Never, 3, 40);
        for (number, value, expected) in [
            (
                SafetyCopyNumber::KeepNewest,
                9,
                settings(SafetyCopyMode::Never, 9, 40),
            ),
            (
                SafetyCopyNumber::DeleteAfterDays,
                9,
                settings(SafetyCopyMode::Never, 3, 9),
            ),
        ] {
            assert_eq!(number.with_value(before, value), expected, "{number:?}");
        }
    }

    #[test]
    fn only_the_numbered_modes_show_a_number_field() {
        assert_eq!(
            MODES.map(SafetyCopyNumber::for_mode),
            [
                Some(SafetyCopyNumber::KeepNewest),
                Some(SafetyCopyNumber::DeleteAfterDays),
                None
            ]
        );
    }

    #[test]
    fn each_sweep_outcome_maps_to_its_notices() {
        use SafetyCopyNotice as N;
        use SweepOrigin::{CleanUp, SettingChange};
        let not_deleted = |count| N::NotDeleted {
            count,
            name: "first.tar.gz".to_owned(),
            reason: "denied".to_owned(),
        };
        for (origin, result, expected) in [
            (
                CleanUp,
                Err("disk gone".to_owned()),
                vec![N::Failed(CleanUp, "disk gone".to_owned())],
            ),
            (
                SettingChange,
                Ok(report(0, &[], true)),
                vec![N::Deferred(SettingChange)],
            ),
            (
                CleanUp,
                Ok(report(0, &[], true)),
                vec![N::Deferred(CleanUp)],
            ),
            (
                CleanUp,
                Ok(report(2, &[], false)),
                vec![N::Freed {
                    removed: 2,
                    bytes: 2,
                }],
            ),
            (
                CleanUp,
                Ok(report(0, &["first.tar.gz", "second.tar.gz"], false)),
                vec![not_deleted(2)],
            ),
            (
                SettingChange,
                Ok(report(1, &["first.tar.gz"], false)),
                vec![
                    N::Freed {
                        removed: 1,
                        bytes: 1,
                    },
                    not_deleted(1),
                ],
            ),
            (SettingChange, Ok(report(0, &[], false)), vec![N::Saved]),
            (CleanUp, Ok(report(0, &[], false)), vec![N::NothingToRemove]),
        ] {
            assert_eq!(notices(origin, &result), expected, "{origin:?} {result:?}");
        }
    }

    #[gpui::test]
    fn the_card_shows_the_stored_policy_once_loaded(cx: &mut TestAppContext) {
        let r = rig(
            cx,
            &[
                (reserved_keys::SAFETY_COPIES_MODE, "delete_after_days"),
                (reserved_keys::SAFETY_COPIES_KEEP_NEWEST, "7"),
                (reserved_keys::SAFETY_COPIES_DELETE_AFTER_DAYS, "45"),
            ],
            0,
        );

        assert!(r.settle(cx, |cx| r.settings(cx)
            == settings(SafetyCopyMode::DeleteAfterDays, 7, 45)));
        assert_eq!(
            (
                r.shown(cx, SafetyCopyNumber::KeepNewest).0,
                r.shown(cx, SafetyCopyNumber::DeleteAfterDays).0
            ),
            ("7".to_owned(), "45".to_owned())
        );
    }

    #[gpui::test]
    fn picking_a_mode_saves_it_and_prunes_under_it(cx: &mut TestAppContext) {
        let r = rig(
            cx,
            &[
                (reserved_keys::SAFETY_COPIES_MODE, "never"),
                (reserved_keys::SAFETY_COPIES_KEEP_NEWEST, "1"),
            ],
            3,
        );
        assert!(r.settle(cx, |cx| r.settings(cx).mode == SafetyCopyMode::Never));

        r.view.update(cx, |view, cx| {
            view.pick_mode(SafetyCopyMode::KeepNewest, cx)
        });

        assert!(r.settle(cx, |cx| r.usage(cx).is_some_and(|u| u.count == 1)));
        assert_eq!(
            (
                r.stored(reserved_keys::SAFETY_COPIES_MODE),
                r.copies_on_disk()
            ),
            (Some("keep_newest".to_owned()), 1)
        );
    }

    #[gpui::test]
    fn picking_the_current_mode_issues_no_change(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 0);

        r.view.update(cx, |view, cx| {
            view.pick_mode(SafetyCopyMode::KeepNewest, cx)
        });

        assert_eq!(r.fence_with_a_real_change(cx), 1);
    }

    #[gpui::test]
    fn switching_mode_keeps_the_other_number_as_typed(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 0);
        r.type_and_commit(cx, SafetyCopyNumber::KeepNewest, "7");
        assert!(
            r.settle(cx, |_| r.stored(reserved_keys::SAFETY_COPIES_KEEP_NEWEST)
                == Some("7".to_owned()))
        );

        r.view.update(cx, |view, cx| {
            view.pick_mode(SafetyCopyMode::DeleteAfterDays, cx);
        });

        assert!(r.settle(cx, |_| r.stored(reserved_keys::SAFETY_COPIES_MODE)
            == Some("delete_after_days".to_owned())));
        assert_eq!(
            (
                r.stored(reserved_keys::SAFETY_COPIES_KEEP_NEWEST),
                r.settings(cx).keep_newest
            ),
            (Some("7".to_owned()), 7)
        );
    }

    #[gpui::test]
    fn a_committed_number_is_normalised_saved_and_pruned_under(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 3);

        r.type_and_commit(cx, SafetyCopyNumber::KeepNewest, " 1 ");

        assert!(r.settle(cx, |cx| r.usage(cx).is_some_and(|u| u.count == 1)));
        assert_eq!(
            (
                r.shown(cx, SafetyCopyNumber::KeepNewest),
                r.stored(reserved_keys::SAFETY_COPIES_KEEP_NEWEST)
            ),
            (("1".to_owned(), false), Some("1".to_owned()))
        );
    }

    #[gpui::test]
    fn an_invalid_number_is_flagged_and_never_saved_or_pruned(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 3);
        let max = *SafetyCopyNumber::KeepNewest.range().end();
        let before = r.settings(cx);
        let mut flagged = Vec::new();
        for typed in [
            String::new(),
            "0".to_owned(),
            (max + 1).to_string(),
            "lots".to_owned(),
        ] {
            r.type_and_commit(cx, SafetyCopyNumber::KeepNewest, &typed);
            flagged.push(r.shown(cx, SafetyCopyNumber::KeepNewest).1);
        }

        let toasts = r.fence_with_a_real_change(cx);

        assert_eq!(flagged, [true, true, true, true]);
        assert_eq!(
            (
                toasts,
                r.stored(reserved_keys::SAFETY_COPIES_KEEP_NEWEST),
                r.copies_on_disk()
            ),
            (1, Some(before.keep_newest.to_string()), 3)
        );
    }

    #[gpui::test]
    fn committing_the_current_value_issues_no_change_and_clears_the_invalid_mark(
        cx: &mut TestAppContext,
    ) {
        let r = rig(cx, &[], 0);
        r.type_and_commit(cx, SafetyCopyNumber::KeepNewest, "");
        let current = r.settings(cx).keep_newest.to_string();

        r.type_and_commit(cx, SafetyCopyNumber::KeepNewest, &current);
        let cleared = !r.shown(cx, SafetyCopyNumber::KeepNewest).1;

        assert_eq!((cleared, r.fence_with_a_real_change(cx)), (true, 1));
    }

    #[gpui::test]
    fn applying_loaded_settings_clears_a_stale_invalid_mark(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 0);
        r.type_and_commit(cx, SafetyCopyNumber::DeleteAfterDays, "never");

        r.view.update(cx, |view, cx| {
            view.apply_settings(settings(SafetyCopyMode::DeleteAfterDays, 4, 12), cx);
        });

        assert_eq!(
            r.shown(cx, SafetyCopyNumber::DeleteAfterDays),
            ("12".to_owned(), false)
        );
    }

    #[gpui::test]
    fn clean_up_deletes_every_safety_copy_and_leaves_the_bundles(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 3);
        let bundle = backups_dir(&r.world.data_dir()).join("forge-backup-manual.forgebundle");
        std::fs::write(&bundle, b"keep").unwrap();

        r.view.update(cx, |view, cx| view.clean_up(cx));

        assert!(r.settle(cx, |cx| r.usage(cx).is_some_and(|u| u.count == 0)));
        assert_eq!((r.copies_on_disk(), bundle.is_file()), (0, true));
    }

    #[gpui::test]
    fn clean_up_during_a_transfer_deletes_nothing(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 3);
        let _running = r.exclusivity.try_acquire().unwrap();

        r.view.update(cx, |view, cx| view.clean_up(cx));

        assert!(r.settle(cx, |cx| r.view.read_with(cx, |view, _| !view.cleaning)));
        assert_eq!(r.copies_on_disk(), 3);
    }

    #[gpui::test]
    fn clean_up_is_blocked_while_cleaning_while_a_transfer_runs_or_when_nothing_is_stored(
        cx: &mut TestAppContext,
    ) {
        let r = rig(cx, &[], 0);
        let blocked = |cx: &mut TestAppContext, cleaning, usage| {
            r.view.update(cx, |view, _| {
                view.cleaning = cleaning;
                view.usage = usage;
                view.clean_up_blocked()
            })
        };
        let some = Some(SafetyCopyUsage { count: 1, bytes: 1 });
        let none_stored = Some(SafetyCopyUsage::default());

        let free = [
            blocked(cx, false, some),
            blocked(cx, false, None),
            blocked(cx, true, some),
            blocked(cx, false, none_stored),
        ];
        let guard = r.exclusivity.try_acquire().unwrap();
        let while_held = blocked(cx, false, some);
        drop(guard);

        assert_eq!((free, while_held), ([false, false, true, true], true));
    }

    #[gpui::test]
    fn a_failed_measurement_shows_the_usage_as_unknown(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 1);

        r.view.update(cx, |view, cx| {
            view.apply_usage(Err("unreadable".to_owned()), cx)
        });

        assert_eq!(r.usage(cx), None);
    }

    #[gpui::test]
    fn a_sweep_that_removed_copies_makes_storage_used_measure_again(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 1);
        assert!(r.settle(cx, |_| r.storage_backups_bytes() == Some(COPY_BYTES as u64)));
        for entry in std::fs::read_dir(backups_dir(&r.world.data_dir())).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }

        r.view.update(cx, |view, cx| {
            view.sweep_finished(SweepOrigin::CleanUp, Ok(report(1, &[], false)), cx);
        });

        assert!(r.settle(cx, |_| r.storage_backups_bytes() == Some(0)));
    }

    #[gpui::test]
    fn a_sweep_that_removed_nothing_leaves_storage_used_alone(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 1);
        assert!(r.settle(cx, |_| r.storage_backups_bytes() == Some(COPY_BYTES as u64)));
        for entry in std::fs::read_dir(backups_dir(&r.world.data_dir())).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }

        r.view.update(cx, |view, cx| {
            view.sweep_finished(SweepOrigin::CleanUp, Ok(report(0, &[], false)), cx);
        });
        r.settle(cx, |cx| r.usage(cx).is_some_and(|u| u.count == 0));

        assert_eq!(r.storage_backups_bytes(), Some(COPY_BYTES as u64));
    }

    #[gpui::test]
    fn a_late_settings_load_does_not_overwrite_a_change_made_on_the_card(cx: &mut TestAppContext) {
        let r = rig(cx, &[], 0);
        r.type_and_commit(cx, SafetyCopyNumber::KeepNewest, "3");
        assert!(r.settle(cx, |_| {
            r.stored(reserved_keys::SAFETY_COPIES_KEEP_NEWEST) == Some("3".to_owned())
        }));
        r.rt.block_on(
            r.world
                .provider()
                .set_string(reserved_keys::SAFETY_COPIES_KEEP_NEWEST, "7"),
        )
        .unwrap();

        r.view.update(cx, |view, cx| view.load(cx));
        r.settle(cx, |_| true);
        for _ in 0..SETTLE_ROUNDS {
            r.rt.block_on(async { tokio::time::sleep(RUNTIME_TURN).await });
            cx.run_until_parked();
        }

        assert_eq!(
            (
                r.settings(cx).keep_newest,
                r.shown(cx, SafetyCopyNumber::KeepNewest).0
            ),
            (3, "3".to_owned())
        );
    }

    #[gpui::test]
    fn a_change_during_a_transfer_is_saved_deletes_nothing_and_says_it_trims_at_next_start(
        cx: &mut TestAppContext,
    ) {
        let r = rig(cx, &[], 3);
        let _running = r.exclusivity.try_acquire().unwrap();
        let next_start = tr!("safety_copies_change_deferred");
        let deferred_shown = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                cx.global::<Toasts>()
                    .items()
                    .iter()
                    .any(|toast| toast.message == next_start)
            })
        };

        r.type_and_commit(cx, SafetyCopyNumber::KeepNewest, "1");

        assert!(r.settle(cx, deferred_shown));
        assert_eq!(
            (
                r.stored(reserved_keys::SAFETY_COPIES_KEEP_NEWEST),
                r.copies_on_disk()
            ),
            (Some("1".to_owned()), 3)
        );
    }
}
