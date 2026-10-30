use std::sync::Arc;

use forge_components::{
    ActivePresentation, Icon, PushToast, ToastKind, setting_row, setting_rows, toggle, tr,
};
use forge_runtime::{NotificationPolicyHandle, NotificationPreferences};
use forge_storage::{Flag, Setting, SettingsRepo, StorageError, write_setting};
use gpui::{App, ClickEvent, Context, IntoElement, ParentElement, Render, Styled, Window, div};
use tokio::runtime::Handle;

use crate::async_bridge;
use crate::navigation::SettingsSection;
use crate::scheduled_backup_notices::BackupFailureNoticePreference;
use crate::settings_catalog::SCHEDULED_BACKUP_FAILURE_OS_NOTICE;
use crate::settings_pane_header::SettingsPaneHeader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationToggle {
    ActionFailureNotice,
    ConnectionLossCue,
}

impl NotificationToggle {
    pub const ALL: [NotificationToggle; 2] = [
        NotificationToggle::ActionFailureNotice,
        NotificationToggle::ConnectionLossCue,
    ];

    pub fn declaration(self) -> &'static Setting<Flag> {
        match self {
            NotificationToggle::ActionFailureNotice => {
                &forge_runtime::settings::ACTION_FAILURE_OS_NOTICE
            }
            NotificationToggle::ConnectionLossCue => {
                &forge_runtime::settings::CONNECTION_LOSS_SOUND
            }
        }
    }

    pub fn read(self, preferences: &NotificationPreferences) -> bool {
        match self {
            NotificationToggle::ActionFailureNotice => preferences.action_failure_notice,
            NotificationToggle::ConnectionLossCue => preferences.connection_loss_cue,
        }
    }

    pub fn write(self, preferences: &mut NotificationPreferences, value: bool) {
        match self {
            NotificationToggle::ActionFailureNotice => preferences.action_failure_notice = value,
            NotificationToggle::ConnectionLossCue => preferences.connection_loss_cue = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSetting {
    Policy(NotificationToggle),
    ScheduledBackupFailureNotice,
}

impl From<NotificationToggle> for NotificationSetting {
    fn from(toggle: NotificationToggle) -> Self {
        NotificationSetting::Policy(toggle)
    }
}

impl NotificationSetting {
    pub fn all() -> impl Iterator<Item = NotificationSetting> {
        NotificationToggle::ALL
            .into_iter()
            .map(NotificationSetting::from)
            .chain([NotificationSetting::ScheduledBackupFailureNotice])
    }

    pub fn declaration(self) -> &'static Setting<Flag> {
        match self {
            NotificationSetting::Policy(toggle) => toggle.declaration(),
            NotificationSetting::ScheduledBackupFailureNotice => {
                &SCHEDULED_BACKUP_FAILURE_OS_NOTICE
            }
        }
    }

    fn element_id(self) -> &'static str {
        match self {
            NotificationSetting::Policy(NotificationToggle::ActionFailureNotice) => {
                "settings-notifications-action-failure"
            }
            NotificationSetting::Policy(NotificationToggle::ConnectionLossCue) => {
                "settings-notifications-connection-loss"
            }
            NotificationSetting::ScheduledBackupFailureNotice => {
                "settings-notifications-scheduled-backup-failure"
            }
        }
    }

    fn icon(self) -> Icon {
        match self {
            NotificationSetting::Policy(NotificationToggle::ActionFailureNotice) => Icon::Bell,
            NotificationSetting::Policy(NotificationToggle::ConnectionLossCue) => {
                Icon::AlertTriangle
            }
            NotificationSetting::ScheduledBackupFailureNotice => Icon::Database,
        }
    }

    fn label(self) -> String {
        match self {
            NotificationSetting::Policy(NotificationToggle::ActionFailureNotice) => {
                tr!("settings_notifications_action_failure_label")
            }
            NotificationSetting::Policy(NotificationToggle::ConnectionLossCue) => {
                tr!("settings_notifications_connection_loss_label")
            }
            NotificationSetting::ScheduledBackupFailureNotice => {
                tr!("settings_notifications_scheduled_backup_failure_label")
            }
        }
    }

    fn hint(self) -> String {
        match self {
            NotificationSetting::Policy(NotificationToggle::ActionFailureNotice) => {
                tr!("settings_notifications_action_failure_hint")
            }
            NotificationSetting::Policy(NotificationToggle::ConnectionLossCue) => {
                tr!("settings_notifications_connection_loss_hint")
            }
            NotificationSetting::ScheduledBackupFailureNotice => {
                tr!("settings_notifications_scheduled_backup_failure_hint")
            }
        }
    }
}

pub struct SettingsNotificationsView {
    settings: Arc<dyn SettingsRepo>,
    rt_handle: Handle,
    policy: NotificationPolicyHandle,
    preferences: NotificationPreferences,
    stored: NotificationPreferences,
    backup_failure_notice: bool,
    stored_backup_failure_notice: bool,
    writing: Vec<NotificationSetting>,
}

impl SettingsNotificationsView {
    pub fn new(
        settings: Arc<dyn SettingsRepo>,
        rt_handle: Handle,
        policy: NotificationPolicyHandle,
    ) -> Self {
        let preferences = policy.preferences();
        let backup_failure_notice = BackupFailureNoticePreference::default().0;
        Self {
            settings,
            rt_handle,
            policy,
            preferences,
            stored: preferences,
            backup_failure_notice,
            stored_backup_failure_notice: backup_failure_notice,
            writing: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_backup_failure_notice(mut self, cx: &App) -> Self {
        let enabled = BackupFailureNoticePreference::current(cx);
        self.backup_failure_notice = enabled;
        self.stored_backup_failure_notice = enabled;
        self
    }

    pub fn shown(&self, setting: NotificationSetting) -> bool {
        match setting {
            NotificationSetting::Policy(toggle) => toggle.read(&self.preferences),
            NotificationSetting::ScheduledBackupFailureNotice => self.backup_failure_notice,
        }
    }

    fn stored_value(&self, setting: NotificationSetting) -> bool {
        match setting {
            NotificationSetting::Policy(toggle) => toggle.read(&self.stored),
            NotificationSetting::ScheduledBackupFailureNotice => self.stored_backup_failure_notice,
        }
    }

    fn show(&mut self, setting: NotificationSetting, value: bool) {
        match setting {
            NotificationSetting::Policy(toggle) => toggle.write(&mut self.preferences, value),
            NotificationSetting::ScheduledBackupFailureNotice => {
                self.backup_failure_notice = value;
            }
        }
    }

    fn record_stored(&mut self, setting: NotificationSetting, value: bool, cx: &mut App) {
        match setting {
            NotificationSetting::Policy(toggle) => {
                toggle.write(&mut self.stored, value);
                self.policy.set_preferences(self.stored);
            }
            NotificationSetting::ScheduledBackupFailureNotice => {
                self.stored_backup_failure_notice = value;
                cx.set_global(BackupFailureNoticePreference(value));
            }
        }
    }

    pub fn flip(&mut self, setting: impl Into<NotificationSetting>, cx: &mut Context<Self>) {
        let setting = setting.into();
        let value = !self.shown(setting);
        self.show(setting, value);
        if !self.writing.contains(&setting) {
            self.start_write(setting, value, cx);
        }
        cx.notify();
    }

    fn start_write(&mut self, setting: NotificationSetting, value: bool, cx: &mut Context<Self>) {
        self.writing.push(setting);
        let repo = Arc::clone(&self.settings);
        let declaration = setting.declaration();
        async_bridge::run_async(
            &self.rt_handle,
            async move { write_setting(repo.as_ref(), declaration, &value).await },
            move |this, result, cx| this.finish_write(setting, value, result, cx),
            cx,
        );
    }

    pub fn finish_write(
        &mut self,
        setting: NotificationSetting,
        written: bool,
        result: Result<(), StorageError>,
        cx: &mut Context<Self>,
    ) {
        self.writing.retain(|pending| *pending != setting);
        match result {
            Ok(()) => {
                self.record_stored(setting, written, cx);
                let wanted = self.shown(setting);
                if wanted != written {
                    self.start_write(setting, wanted, cx);
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, key = setting.declaration().name(), "notification preference not saved");
                self.show(setting, self.stored_value(setting));
                cx.push_toast(
                    ToastKind::Error,
                    tr!("settings_notifications_persist_failed"),
                );
            }
        }
        cx.notify();
    }
}

impl Render for SettingsNotificationsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let rows = NotificationSetting::all().map(|setting| {
            let glyph_color = match setting {
                NotificationSetting::Policy(NotificationToggle::ActionFailureNotice) => {
                    palette.brand
                }
                NotificationSetting::Policy(NotificationToggle::ConnectionLossCue) => {
                    palette.warning
                }
                NotificationSetting::ScheduledBackupFailureNotice => palette.info,
            };
            setting_row(
                setting.label(),
                Some(setting.hint().into()),
                toggle(self.shown(setting), &palette).on_click(
                    setting.element_id(),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.flip(setting, cx)),
                ),
                &palette,
                density,
            )
            .leading(setting.icon(), glyph_color)
            .into_any_element()
        });

        div()
            .flex()
            .flex_col()
            .child(
                SettingsPaneHeader::new(SettingsSection::Notifications)
                    .subtitle(tr!("settings_notifications_subtitle")),
            )
            .child(setting_rows(&palette).rows(rows))
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{Density, Presentation, ThemeId, Toasts};
    use forge_runtime::spawn_notification_policy;
    use forge_storage::Language;
    use forge_storage::settings::MockSettingsRepo;
    use gpui::{AppContext, Entity, TestAppContext};

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{pump, quiet_bus, runtime, test_backend};

    const STORED: NotificationPreferences = NotificationPreferences {
        action_failure_notice: false,
        connection_loss_cue: true,
    };

    fn policy(
        rt: &tokio::runtime::Runtime,
        initial: NotificationPreferences,
    ) -> NotificationPolicyHandle {
        let bus = quiet_bus(rt);
        rt.block_on(async { spawn_notification_policy(&bus, initial) })
    }

    fn mount(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        settings: Arc<dyn SettingsRepo>,
        policy: NotificationPolicyHandle,
    ) -> Entity<SettingsNotificationsView> {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        let handle = rt.handle().clone();
        cx.update(|cx| cx.new(|_| SettingsNotificationsView::new(settings, handle, policy)))
    }

    fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        for _ in 0..4 {
            pump(rt);
            cx.run_until_parked();
        }
    }

    fn shown(
        view: &Entity<SettingsNotificationsView>,
        cx: &mut TestAppContext,
    ) -> NotificationPreferences {
        view.read_with(cx, |view, _| view.preferences)
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<(ToastKind, String)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|t| (t.kind, t.message.to_string()))
                .collect()
        })
    }

    fn failing_settings() -> Arc<dyn SettingsRepo> {
        let mut repo = MockSettingsRepo::new();
        repo.expect_set_string().returning(|key, _| {
            Err(StorageError::NotFound {
                key: key.to_owned(),
            })
        });
        Arc::new(repo)
    }

    fn flipped(
        preferences: NotificationPreferences,
        setting: NotificationToggle,
    ) -> NotificationPreferences {
        let mut next = preferences;
        setting.write(&mut next, !setting.read(&preferences));
        next
    }

    #[gpui::test]
    fn toggles_open_showing_the_policy_preferences(cx: &mut TestAppContext) {
        let rt = runtime();
        for initial in [
            NotificationPreferences::default(),
            STORED,
            NotificationPreferences {
                action_failure_notice: true,
                connection_loss_cue: false,
            },
        ] {
            let (backend, _writes) = test_backend();
            let view = mount(cx, &rt, backend, policy(&rt, initial));
            assert_eq!(shown(&view, cx), initial);
        }
    }

    #[gpui::test]
    fn flipping_a_toggle_persists_its_key_and_pushes_the_new_preferences(cx: &mut TestAppContext) {
        let rt = runtime();
        for (setting, key, value) in [
            (
                NotificationToggle::ActionFailureNotice,
                reserved_keys::NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE,
                "true",
            ),
            (
                NotificationToggle::ConnectionLossCue,
                reserved_keys::NOTIFICATIONS_CONNECTION_LOSS_SOUND,
                "false",
            ),
        ] {
            let (backend, mut writes) = test_backend();
            let policy = policy(&rt, STORED);
            let view = mount(cx, &rt, backend, policy.clone());

            view.update(cx, |view, cx| view.flip(setting, cx));
            settle(cx, &rt);

            let expected = flipped(STORED, setting);
            assert_eq!(
                (
                    writes.try_recv().ok(),
                    writes.try_recv().ok(),
                    policy.preferences(),
                    shown(&view, cx),
                ),
                (
                    Some((key.to_owned(), value.to_owned())),
                    None,
                    expected,
                    expected,
                ),
                "{setting:?}"
            );
        }
    }

    #[gpui::test]
    fn a_failed_write_reverts_the_toggle_keeps_the_policy_and_shows_an_error_toast(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        for setting in NotificationToggle::ALL {
            let policy = policy(&rt, STORED);
            let view = mount(cx, &rt, failing_settings(), policy.clone());

            view.update(cx, |view, cx| view.flip(setting, cx));
            settle(cx, &rt);

            assert_eq!(
                (shown(&view, cx), policy.preferences(), toasts(cx)),
                (
                    STORED,
                    STORED,
                    vec![(
                        ToastKind::Error,
                        tr!("settings_notifications_persist_failed")
                    )]
                ),
                "{setting:?}"
            );
        }
    }

    #[gpui::test]
    fn the_flip_shows_before_the_write_lands(cx: &mut TestAppContext) {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let policy = policy(&rt, STORED);
        let view = mount(cx, &rt, backend, policy.clone());

        view.update(cx, |view, cx| {
            view.flip(NotificationToggle::ConnectionLossCue, cx)
        });

        assert_eq!(
            (shown(&view, cx), policy.preferences()),
            (
                flipped(STORED, NotificationToggle::ConnectionLossCue),
                STORED
            )
        );
    }

    fn mount_with_backup_notice(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        settings: Arc<dyn SettingsRepo>,
        enabled: bool,
    ) -> (Entity<SettingsNotificationsView>, NotificationPolicyHandle) {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
            cx.set_global(BackupFailureNoticePreference(enabled));
        });
        let policy = policy(rt, STORED);
        let handle = rt.handle().clone();
        let shared = policy.clone();
        let view = cx.update(|cx| {
            cx.new(|cx| {
                SettingsNotificationsView::new(settings, handle, shared)
                    .with_backup_failure_notice(cx)
            })
        });
        (view, policy)
    }

    fn backup_notice(cx: &mut TestAppContext) -> bool {
        cx.update(|cx| BackupFailureNoticePreference::current(cx))
    }

    #[gpui::test]
    fn the_backup_failure_toggle_opens_showing_the_global_preference(cx: &mut TestAppContext) {
        let rt = runtime();
        for enabled in [true, false] {
            let (backend, _writes) = test_backend();
            let (view, _policy) = mount_with_backup_notice(cx, &rt, backend, enabled);
            let opened = view.read_with(cx, |view, _| {
                view.shown(NotificationSetting::ScheduledBackupFailureNotice)
            });
            assert_eq!(opened, enabled);
        }
    }

    #[gpui::test]
    fn flipping_the_backup_failure_toggle_persists_its_key_and_updates_the_global_only(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        let (view, policy) = mount_with_backup_notice(cx, &rt, backend, true);

        view.update(cx, |view, cx| {
            view.flip(NotificationSetting::ScheduledBackupFailureNotice, cx)
        });
        settle(cx, &rt);

        assert_eq!(
            (
                writes.try_recv().ok(),
                writes.try_recv().ok(),
                backup_notice(cx),
                policy.preferences(),
            ),
            (
                Some((
                    reserved_keys::NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE.to_owned(),
                    "false".to_owned()
                )),
                None,
                false,
                STORED,
            )
        );
    }

    #[gpui::test]
    fn the_backup_failure_flip_shows_before_the_write_lands_and_the_global_waits(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let (view, _policy) = mount_with_backup_notice(cx, &rt, backend, true);

        view.update(cx, |view, cx| {
            view.flip(NotificationSetting::ScheduledBackupFailureNotice, cx)
        });

        let shown = view.read_with(cx, |view, _| {
            view.shown(NotificationSetting::ScheduledBackupFailureNotice)
        });
        assert_eq!((shown, backup_notice(cx)), (false, true));
    }

    #[gpui::test]
    fn a_failed_backup_failure_toggle_write_reverts_it_keeps_the_global_and_shows_an_error(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (view, _policy) = mount_with_backup_notice(cx, &rt, failing_settings(), true);

        view.update(cx, |view, cx| {
            view.flip(NotificationSetting::ScheduledBackupFailureNotice, cx)
        });
        settle(cx, &rt);

        let shown = view.read_with(cx, |view, _| {
            view.shown(NotificationSetting::ScheduledBackupFailureNotice)
        });
        assert_eq!(
            (shown, backup_notice(cx), toasts(cx)),
            (
                true,
                true,
                vec![(
                    ToastKind::Error,
                    tr!("settings_notifications_persist_failed")
                )]
            )
        );
    }

    #[gpui::test]
    fn a_double_flip_of_the_backup_failure_toggle_settles_on_the_original_value(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        let (view, _policy) = mount_with_backup_notice(cx, &rt, backend, true);

        view.update(cx, |view, cx| {
            view.flip(NotificationSetting::ScheduledBackupFailureNotice, cx);
            view.flip(NotificationSetting::ScheduledBackupFailureNotice, cx);
        });
        settle(cx, &rt);

        let mut written = Vec::new();
        while let Ok((_, value)) = writes.try_recv() {
            written.push(value);
        }
        assert_eq!(
            (written, backup_notice(cx)),
            (vec!["false".to_owned(), "true".to_owned()], true)
        );
    }

    #[gpui::test]
    fn two_failed_writes_of_a_double_flip_leave_the_toggle_at_the_stored_value(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let policy = policy(&rt, STORED);
        let view = mount(cx, &rt, failing_settings(), policy.clone());
        let setting = NotificationToggle::ActionFailureNotice;

        view.update(cx, |view, cx| {
            view.flip(setting, cx);
            view.flip(setting, cx);
        });
        settle(cx, &rt);

        assert_eq!((shown(&view, cx), policy.preferences()), (STORED, STORED));
    }
}
