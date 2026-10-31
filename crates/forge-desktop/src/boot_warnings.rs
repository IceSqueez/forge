use std::time::Duration;

use forge_components::{PushToast, ToastAction, ToastKind, tr};
use gpui::App;

use crate::navigation::{Navigator, Screen, SettingsSection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BootWarning {
    Scripts,
    CoreSubActions,
    ScheduledRunSubActions,
    DonationSubActions,
    LatestValueSubActions,
    CoreTriggers,
    SoundboardSubActions,
    SpeechSubActions,
    TriggerDefaults,
    Queues,
    OverlayTypes,
    BotAccounts,
    VoiceGate,
    VoiceAliases,
    ServerBindAddress,
}

impl BootWarning {
    fn message_key(self) -> &'static str {
        match self {
            Self::Scripts => "boot_warning_scripts",
            Self::CoreSubActions => "boot_warning_core_sub_actions",
            Self::ScheduledRunSubActions => "boot_warning_scheduled_run_sub_actions",
            Self::DonationSubActions => "boot_warning_donation_sub_actions",
            Self::LatestValueSubActions => "boot_warning_latest_value_sub_actions",
            Self::CoreTriggers => "boot_warning_core_triggers",
            Self::SoundboardSubActions => "boot_warning_soundboard_sub_actions",
            Self::SpeechSubActions => "boot_warning_speech_sub_actions",
            Self::TriggerDefaults => "boot_warning_trigger_defaults",
            Self::Queues => "boot_warning_queues",
            Self::OverlayTypes => "boot_warning_overlay_types",
            Self::BotAccounts => "boot_warning_bot_accounts",
            Self::VoiceGate => "boot_warning_voice_gate",
            Self::VoiceAliases => "boot_warning_voice_aliases",
            Self::ServerBindAddress => "boot_warning_server_bind_address",
        }
    }

    pub(crate) fn message(self) -> String {
        tr!(self.message_key())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BootWarnings(Vec<BootWarning>);

impl BootWarnings {
    pub(crate) fn record(&mut self, warning: BootWarning) {
        if !self.0.contains(&warning) {
            self.0.push(warning);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = BootWarning> + '_ {
        self.0.iter().copied()
    }

    pub(crate) fn raise(&self, navigator: Navigator, cx: &mut App) {
        if self.is_empty() {
            return;
        }
        let count = i64::try_from(self.0.len()).unwrap_or(i64::MAX);
        let action = ToastAction::new(tr!("boot_warnings_open"), move |_window, cx| {
            navigator.open(Screen::Settings(Some(SettingsSection::Diagnostics)), cx);
        });
        cx.push_toast_full(
            ToastKind::Warn,
            tr!("boot_warnings_toast", count = count),
            None,
            Some(action),
            Duration::ZERO,
        );
    }
}

#[cfg(test)]
mod tests {
    use forge_components::Toasts;
    use forge_components::tr_find_warned;
    use forge_storage::Language;
    use gpui::TestAppContext;

    use super::*;
    use crate::i18n::install_language;

    const EVERY_WARNING: [BootWarning; 14] = [
        BootWarning::Scripts,
        BootWarning::CoreSubActions,
        BootWarning::ScheduledRunSubActions,
        BootWarning::DonationSubActions,
        BootWarning::LatestValueSubActions,
        BootWarning::CoreTriggers,
        BootWarning::SoundboardSubActions,
        BootWarning::SpeechSubActions,
        BootWarning::TriggerDefaults,
        BootWarning::Queues,
        BootWarning::OverlayTypes,
        BootWarning::BotAccounts,
        BootWarning::VoiceGate,
        BootWarning::VoiceAliases,
    ];

    fn raised(warnings: &BootWarnings, cx: &mut TestAppContext) -> Vec<(ToastKind, String, bool)> {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Toasts::new());
            warnings.raise(Navigator::new(|_, _| {}), cx);
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| {
                    (
                        toast.kind,
                        toast
                            .message
                            .replace(crate::test_support::FORMAT_ISOLATES, ""),
                        toast.action.is_some(),
                    )
                })
                .collect()
        })
    }

    #[test]
    fn a_warning_recorded_twice_is_listed_once_in_first_seen_order() {
        let mut warnings = BootWarnings::default();

        for warning in [
            BootWarning::Queues,
            BootWarning::Scripts,
            BootWarning::Queues,
        ] {
            warnings.record(warning);
        }

        assert_eq!(
            warnings.iter().collect::<Vec<_>>(),
            [BootWarning::Queues, BootWarning::Scripts]
        );
    }

    #[test]
    fn every_warning_has_its_own_text_in_both_languages() {
        for language in [Language::En, Language::Uk] {
            install_language(language);
            let mut seen = std::collections::BTreeSet::new();
            for warning in EVERY_WARNING {
                assert!(
                    tr_find_warned(warning.message_key(), None).is_some(),
                    "{warning:?} has no text"
                );
                assert!(seen.insert(warning.message()), "{warning:?} repeats a text");
            }
        }
    }

    #[gpui::test]
    fn a_clean_boot_raises_no_notice(cx: &mut TestAppContext) {
        assert_eq!(raised(&BootWarnings::default(), cx), Vec::new());
    }

    #[gpui::test]
    fn a_degraded_boot_raises_one_warning_that_counts_the_failures_and_offers_the_details(
        cx: &mut TestAppContext,
    ) {
        let mut warnings = BootWarnings::default();
        warnings.record(BootWarning::Queues);
        warnings.record(BootWarning::VoiceGate);

        let toasts = raised(&warnings, cx);

        assert_eq!(toasts.len(), 1);
        let (kind, message, offers_details) = &toasts[0];
        assert_eq!((*kind, *offers_details), (ToastKind::Warn, true));
        assert!(message.contains('2'), "{message}");
    }
}
