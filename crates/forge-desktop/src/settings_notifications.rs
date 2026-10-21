use std::sync::Arc;

use forge_components::{
    ActivePresentation, Icon, PushToast, ToastKind, setting_row, setting_rows, toggle, tr,
};
use forge_runtime::{NotificationPolicyHandle, NotificationPreferences};
use forge_storage::{SettingsRepo, StorageError, reserved_keys, set_bool_setting};
use gpui::{ClickEvent, Context, IntoElement, ParentElement, Render, Styled, Window, div};
use tokio::runtime::Handle;

use crate::async_bridge;
use crate::settings::SettingsSection;
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

    pub fn storage_key(self) -> &'static str {
        match self {
            NotificationToggle::ActionFailureNotice => {
                reserved_keys::NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE
            }
            NotificationToggle::ConnectionLossCue => {
                reserved_keys::NOTIFICATIONS_CONNECTION_LOSS_SOUND
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

    fn element_id(self) -> &'static str {
        match self {
            NotificationToggle::ActionFailureNotice => "settings-notifications-action-failure",
            NotificationToggle::ConnectionLossCue => "settings-notifications-connection-loss",
        }
    }

    fn icon(self) -> Icon {
        match self {
            NotificationToggle::ActionFailureNotice => Icon::Bell,
            NotificationToggle::ConnectionLossCue => Icon::AlertTriangle,
        }
    }

    fn label(self) -> String {
        match self {
            NotificationToggle::ActionFailureNotice => {
                tr!("settings_notifications_action_failure_label")
            }
            NotificationToggle::ConnectionLossCue => {
                tr!("settings_notifications_connection_loss_label")
            }
        }
    }

    fn hint(self) -> String {
        match self {
            NotificationToggle::ActionFailureNotice => {
                tr!("settings_notifications_action_failure_hint")
            }
            NotificationToggle::ConnectionLossCue => {
                tr!("settings_notifications_connection_loss_hint")
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
    writing: Vec<NotificationToggle>,
}

impl SettingsNotificationsView {
    pub fn new(
        settings: Arc<dyn SettingsRepo>,
        rt_handle: Handle,
        policy: NotificationPolicyHandle,
    ) -> Self {
        let preferences = policy.preferences();
        Self {
            settings,
            rt_handle,
            policy,
            preferences,
            stored: preferences,
            writing: Vec::new(),
        }
    }

    pub fn flip(&mut self, setting: NotificationToggle, cx: &mut Context<Self>) {
        let value = !setting.read(&self.preferences);
        setting.write(&mut self.preferences, value);
        if !self.writing.contains(&setting) {
            self.start_write(setting, value, cx);
        }
        cx.notify();
    }

    fn start_write(&mut self, setting: NotificationToggle, value: bool, cx: &mut Context<Self>) {
        self.writing.push(setting);
        let repo = Arc::clone(&self.settings);
        let key = setting.storage_key();
        async_bridge::run_async(
            &self.rt_handle,
            async move { set_bool_setting(repo.as_ref(), key, value).await },
            move |this, result, cx| this.finish_write(setting, value, result, cx),
            cx,
        );
    }

    pub fn finish_write(
        &mut self,
        setting: NotificationToggle,
        written: bool,
        result: Result<(), StorageError>,
        cx: &mut Context<Self>,
    ) {
        self.writing.retain(|pending| *pending != setting);
        match result {
            Ok(()) => {
                setting.write(&mut self.stored, written);
                self.policy.set_preferences(self.stored);
                let wanted = setting.read(&self.preferences);
                if wanted != written {
                    self.start_write(setting, wanted, cx);
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, key = setting.storage_key(), "notification preference not saved");
                setting.write(&mut self.preferences, setting.read(&self.stored));
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
        let rows = NotificationToggle::ALL.map(|setting| {
            let glyph_color = match setting {
                NotificationToggle::ActionFailureNotice => palette.brand,
                NotificationToggle::ConnectionLossCue => palette.warning,
            };
            setting_row(
                setting.label(),
                Some(setting.hint().into()),
                toggle(setting.read(&self.preferences), &palette).on_click(
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
