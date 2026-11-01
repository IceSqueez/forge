use forge_components::STICKY_TOAST;

use forge_components::{PushToast, ToastAction, ToastKind, tr};
use forge_host::boot_warnings::{BootWarning, BootWarnings};
use gpui::App;

use crate::app::navigation::{Navigator, Screen, SettingsSection};

fn message_key(warning: BootWarning) -> &'static str {
    match warning {
        BootWarning::Scripts => "boot_warning_scripts",
        BootWarning::CoreSubActions => "boot_warning_core_sub_actions",
        BootWarning::ScheduledRunSubActions => "boot_warning_scheduled_run_sub_actions",
        BootWarning::DonationSubActions => "boot_warning_donation_sub_actions",
        BootWarning::LatestValueSubActions => "boot_warning_latest_value_sub_actions",
        BootWarning::CoreTriggers => "boot_warning_core_triggers",
        BootWarning::SoundboardSubActions => "boot_warning_soundboard_sub_actions",
        BootWarning::SpeechSubActions => "boot_warning_speech_sub_actions",
        BootWarning::TriggerDefaults => "boot_warning_trigger_defaults",
        BootWarning::Queues => "boot_warning_queues",
        BootWarning::OverlayTypes => "boot_warning_overlay_types",
        BootWarning::BotAccounts => "boot_warning_bot_accounts",
        BootWarning::VoiceGate => "boot_warning_voice_gate",
        BootWarning::VoiceAliases => "boot_warning_voice_aliases",
        BootWarning::ServerBindAddress => "boot_warning_server_bind_address",
    }
}

pub(crate) fn boot_warning_message(warning: BootWarning) -> String {
    tr!(message_key(warning))
}

pub(crate) fn raise_boot_warnings(warnings: &BootWarnings, navigator: Navigator, cx: &mut App) {
    if warnings.is_empty() {
        return;
    }
    let count = i64::try_from(warnings.len()).unwrap_or(i64::MAX);
    let action = ToastAction::new(tr!("boot_warnings_open"), move |_window, cx| {
        navigator.open(Screen::Settings(Some(SettingsSection::Diagnostics)), cx);
    });
    cx.push_toast_full(
        ToastKind::Warn,
        tr!("boot_warnings_toast", count = count),
        None,
        Some(action),
        STICKY_TOAST,
    );
}

#[cfg(test)]
mod tests {
    use forge_components::Toasts;
    use forge_components::tr_find_warned;
    use forge_storage::Language;
    use gpui::TestAppContext;

    use super::*;
    use crate::app::i18n::install_language;

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
            raise_boot_warnings(warnings, Navigator::new(|_, _| {}), cx);
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
                    tr_find_warned(message_key(warning), None).is_some(),
                    "{warning:?} has no text"
                );
                assert!(
                    seen.insert(boot_warning_message(warning)),
                    "{warning:?} repeats a text"
                );
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
