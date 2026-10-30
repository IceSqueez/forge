use std::sync::Arc;

use forge_audio::{alert_cue, play_on_default_output};
use forge_components::{PushToast, ToastKind, tr};
use forge_runtime::sub_action_runners::{DesktopNotice, NotifyPort, NotifyUrgency, OsPortError};
use forge_runtime::{
    ActionFailureNotice, NotificationIntent, NotificationIntents, NotificationPolicyHandle,
};
use gpui::AsyncApp;
use tokio::runtime::Handle;

use crate::user_text::resolve_user_text;

pub const NOTICE_REASON_MAX_CHARS: usize = 160;
pub const NOTICE_DISPLAY_MS: u32 = 8000;
const FLUENT_ISOLATE_START: char = '\u{2068}';
const FLUENT_ISOLATE_END: char = '\u{2069}';
const TRUNCATION_MARK: &str = "...";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeText {
    pub title: String,
    pub body: String,
}

impl NoticeText {
    pub fn toast_message(&self) -> String {
        format!("{} - {}", self.title, self.body)
    }

    pub fn desktop_notice(&self) -> DesktopNotice {
        DesktopNotice {
            title: without_isolates(&self.title),
            body: without_isolates(&self.body),
            urgency: NotifyUrgency::Normal,
            icon_path: None,
            timeout_ms: NOTICE_DISPLAY_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoticePlan {
    Show(NoticeText),
    PlayCue,
}

pub fn plan_for(intent: &NotificationIntent) -> NoticePlan {
    match intent {
        NotificationIntent::ActionFailed(notice) => NoticePlan::Show(action_failed_text(notice)),
        NotificationIntent::ActionFailuresFolded { count } => NoticePlan::Show(NoticeText {
            title: tr!("system_notice_actions_folded_title"),
            body: tr!(
                "system_notice_actions_folded_body",
                count = i64::from(*count)
            ),
        }),
        NotificationIntent::ConnectionLossCue { .. } => NoticePlan::PlayCue,
    }
}

fn action_failed_text(notice: &ActionFailureNotice) -> NoticeText {
    let action = notice.action_name.clone();
    let reason = notice
        .reason
        .as_ref()
        .map(|reason| resolve_user_text(reason).inline());
    let body = match reason.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        Some(reason) => tr!(
            "system_notice_action_failed_body_reason",
            action = action,
            reason = truncate_reason(reason, NOTICE_REASON_MAX_CHARS)
        ),
        None => tr!("system_notice_action_failed_body", action = action),
    };
    NoticeText {
        title: tr!("system_notice_action_failed_title"),
        body,
    }
}

pub fn truncate_reason(reason: &str, max_chars: usize) -> String {
    if reason.chars().count() <= max_chars {
        return reason.to_owned();
    }
    let mut kept: String = reason
        .chars()
        .take(max_chars.saturating_sub(TRUNCATION_MARK.len()))
        .collect();
    kept.push_str(TRUNCATION_MARK);
    kept
}

fn without_isolates(text: &str) -> String {
    text.chars()
        .filter(|c| *c != FLUENT_ISOLATE_START && *c != FLUENT_ISOLATE_END)
        .collect()
}

#[derive(Default)]
pub struct FallbackLog {
    reported: bool,
}

impl FallbackLog {
    pub fn first_failure(&mut self) -> bool {
        !std::mem::replace(&mut self.reported, true)
    }
}

pub fn start_system_notices(
    cx: &mut AsyncApp,
    policy: &NotificationPolicyHandle,
    port: Arc<dyn NotifyPort>,
    rt: Handle,
) {
    let Some(intents) = policy.take_intents() else {
        tracing::warn!("notification intents already claimed; system notices stay off");
        return;
    };
    cx.spawn(async move |cx| drain_intents(intents, port, rt, cx).await)
        .detach();
}

pub async fn drain_intents(
    mut intents: NotificationIntents,
    port: Arc<dyn NotifyPort>,
    rt: Handle,
    cx: &mut AsyncApp,
) {
    let mut fallback_log = FallbackLog::default();
    while let Some(intent) = intents.next().await {
        let plan = cx.update(|_| plan_for(&intent));
        match plan {
            NoticePlan::Show(text) => {
                if let Err(error) =
                    show_off_thread(&rt, Arc::clone(&port), text.desktop_notice()).await
                {
                    if fallback_log.first_failure() {
                        tracing::warn!(
                            error = %error,
                            "desktop notifications unavailable; notices fall back to in-app toasts"
                        );
                    }
                    cx.update(|cx| cx.push_toast(ToastKind::Error, text.toast_message()));
                }
            }
            NoticePlan::PlayCue => play_cue_detached(&rt),
        }
    }
}

pub(crate) async fn show_off_thread(
    rt: &Handle,
    port: Arc<dyn NotifyPort>,
    notice: DesktopNotice,
) -> Result<(), OsPortError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn_blocking(move || {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = tx.send(port.show(notice));
    });
    rx.await
        .unwrap_or_else(|_| Err(OsPortError::Failed("notice task ended".to_owned())))
}

fn play_cue_detached(rt: &Handle) {
    rt.spawn(async {
        if let Err(error) = play_on_default_output(alert_cue()).await {
            tracing::warn!(error = %error, "connection loss cue could not play");
        }
    });
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use forge_components::Toasts;
    use forge_events::{ActionDone, Event, EventSource, RunOutcome, TypedEvent};
    use forge_runtime::{NotificationPreferences, spawn_notification_policy};
    use forge_storage::Language;
    use forge_types::ActionId;
    use gpui::TestAppContext;

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{pump, quiet_bus, runtime};

    const SETTLE_ROUNDS: usize = 2000;
    const ALL_ON: NotificationPreferences = NotificationPreferences {
        action_failure_notice: true,
        connection_loss_cue: true,
    };

    struct ScriptedPort {
        results: Mutex<VecDeque<Result<(), OsPortError>>>,
        seen: Mutex<Vec<DesktopNotice>>,
    }

    impl ScriptedPort {
        fn new(results: Vec<Result<(), OsPortError>>) -> Arc<Self> {
            Arc::new(Self {
                results: Mutex::new(results.into()),
                seen: Mutex::new(Vec::new()),
            })
        }

        fn seen_bodies(&self) -> Vec<String> {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .map(|notice| notice.body.clone())
                .collect()
        }
    }

    impl NotifyPort for ScriptedPort {
        fn show(&self, notice: DesktopNotice) -> Result<(), OsPortError> {
            self.seen.lock().unwrap().push(notice);
            self.results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(OsPortError::Failed("unscripted".to_owned())))
        }
    }

    fn run_failed(name: &str) -> (Event, NotificationIntent) {
        let action_id = ActionId::new();
        let event = ActionDone {
            action_id,
            action_name: name.to_owned(),
            outcome: RunOutcome::Failed,
            total_ms: 0,
            manual_test: false,
            reason: Some("OBS is offline".to_owned()),
            reason_text: Some(forge_types::UserText::unlocalized("OBS is offline")),
        }
        .into_event(EventSource::Core, None);
        let intent = NotificationIntent::ActionFailed(ActionFailureNotice {
            action_id,
            action_name: name.to_owned(),
            reason: Some(forge_types::UserText::unlocalized("OBS is offline")),
        });
        (event, intent)
    }

    fn fallback_toast(intent: &NotificationIntent) -> (ToastKind, String) {
        match plan_for(intent) {
            NoticePlan::Show(text) => (ToastKind::Error, text.toast_message()),
            NoticePlan::PlayCue => panic!("expected a notice for {intent:?}"),
        }
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

    fn drain_events_through(
        cx: &mut TestAppContext,
        port: Arc<ScriptedPort>,
        events: Vec<Event>,
        expected_toasts: usize,
    ) -> Vec<(ToastKind, String)> {
        install_language(Language::En);
        cx.executor().allow_parking();
        cx.update(|cx| cx.set_global(Toasts::new()));
        let rt = runtime();
        let bus = quiet_bus(&rt);
        let policy = rt.block_on(async { spawn_notification_policy(&bus, ALL_ON) });
        start_system_notices(
            &mut cx.to_async(),
            &policy,
            port as Arc<dyn NotifyPort>,
            rt.handle().clone(),
        );
        pump(&rt);
        rt.block_on(async {
            for event in events {
                bus.publish(event);
            }
        });
        for _ in 0..SETTLE_ROUNDS {
            pump(&rt);
            cx.run_until_parked();
            if toasts(cx).len() >= expected_toasts {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        pump(&rt);
        cx.run_until_parked();
        toasts(cx)
    }

    #[gpui::test]
    fn a_desktop_notice_the_os_rejects_falls_back_to_exactly_one_error_toast(
        cx: &mut TestAppContext,
    ) {
        let port = ScriptedPort::new(vec![Err(OsPortError::Unavailable("no daemon".to_owned()))]);
        let (event, intent) = run_failed("Shoutout");

        let shown = drain_events_through(cx, Arc::clone(&port), vec![event], 1);

        assert_eq!(
            (port.seen_bodies().len(), shown),
            (1, vec![fallback_toast(&intent)])
        );
    }

    #[gpui::test]
    fn a_desktop_notice_the_os_shows_raises_no_toast(cx: &mut TestAppContext) {
        let port = ScriptedPort::new(vec![
            Ok(()),
            Err(OsPortError::Failed("daemon gone".to_owned())),
        ]);
        let (shown_event, _) = run_failed("Alpha");
        let (rejected_event, rejected) = run_failed("Beta");

        let shown =
            drain_events_through(cx, Arc::clone(&port), vec![shown_event, rejected_event], 1);

        assert_eq!(
            (port.seen_bodies(), shown),
            (
                vec![
                    "Alpha - OBS is offline".to_owned(),
                    "Beta - OBS is offline".to_owned()
                ],
                vec![fallback_toast(&rejected)]
            )
        );
    }

    fn failed(action: &str, reason: Option<&str>) -> NotificationIntent {
        NotificationIntent::ActionFailed(ActionFailureNotice {
            action_id: ActionId::new(),
            action_name: action.to_owned(),
            reason: reason.map(forge_types::UserText::unlocalized),
        })
    }

    fn shown(intent: &NotificationIntent) -> DesktopNotice {
        match plan_for(intent) {
            NoticePlan::Show(text) => text.desktop_notice(),
            NoticePlan::PlayCue => panic!("expected a notice for {intent:?}"),
        }
    }

    fn folded_body(lang: Language, count: u32) -> String {
        install_language(lang);
        shown(&NotificationIntent::ActionFailuresFolded { count }).body
    }

    #[test]
    fn action_failed_notice_carries_action_and_reason_in_each_locale() {
        for (lang, title) in [
            (Language::En, "Action failed"),
            (Language::Uk, "Дія не виконалася"),
        ] {
            install_language(lang);
            let notice = shown(&failed("Shoutout", Some("OBS is offline")));
            assert_eq!(notice.title, title, "{lang:?}");
            assert_eq!(notice.body, "Shoutout - OBS is offline", "{lang:?}");
        }
    }

    #[test]
    fn action_failed_without_a_usable_reason_uses_the_reasonless_body() {
        for (lang, expected) in [
            (Language::En, "Shoutout did not finish."),
            (Language::Uk, "Shoutout не завершилася."),
        ] {
            install_language(lang);
            for reason in [None, Some(""), Some("   \n\t")] {
                assert_eq!(
                    shown(&failed("Shoutout", reason)).body,
                    expected,
                    "{lang:?} {reason:?}"
                );
            }
        }
    }

    #[test]
    fn action_failed_reason_is_trimmed_before_display() {
        install_language(Language::En);
        assert_eq!(
            shown(&failed("Shoutout", Some("  OBS is offline \n"))).body,
            "Shoutout - OBS is offline"
        );
    }

    #[test]
    fn folded_notice_body_uses_english_plural_forms() {
        for (count, expected) in [
            (1, "1 more action failed. See the event log."),
            (2, "2 more actions failed. See the event log."),
            (0, "0 more actions failed. See the event log."),
        ] {
            assert_eq!(folded_body(Language::En, count), expected, "count {count}");
        }
    }

    #[test]
    fn folded_notice_body_uses_ukrainian_one_few_many_forms() {
        for (count, expected) in [
            (1, "Ще 1 дія не виконалася. Перегляньте журнал подій."),
            (21, "Ще 21 дія не виконалася. Перегляньте журнал подій."),
            (3, "Ще 3 дії не виконалися. Перегляньте журнал подій."),
            (22, "Ще 22 дії не виконалися. Перегляньте журнал подій."),
            (5, "Ще 5 дій не виконалися. Перегляньте журнал подій."),
            (11, "Ще 11 дій не виконалися. Перегляньте журнал подій."),
            (12, "Ще 12 дій не виконалися. Перегляньте журнал подій."),
        ] {
            assert_eq!(folded_body(Language::Uk, count), expected, "count {count}");
        }
    }

    #[test]
    fn folded_notice_has_a_translated_title_in_each_locale() {
        for (lang, title) in [
            (Language::En, "More actions failed"),
            (Language::Uk, "Не виконалися й інші дії"),
        ] {
            install_language(lang);
            let notice = shown(&NotificationIntent::ActionFailuresFolded { count: 2 });
            assert_eq!(notice.title, title, "{lang:?}");
        }
    }

    #[test]
    fn truncate_reason_keeps_a_reason_at_the_limit_untouched() {
        for reason in ["a".repeat(8), "є".repeat(8), "🎮".repeat(8)] {
            assert_eq!(truncate_reason(&reason, 8), reason);
        }
    }

    #[test]
    fn truncate_reason_one_over_the_limit_ends_with_the_mark_at_the_limit_length() {
        for unit in ["a", "є", "🎮"] {
            let truncated = truncate_reason(&unit.repeat(9), 8);
            assert_eq!(truncated, format!("{}...", unit.repeat(5)), "{unit}");
        }
    }

    #[test]
    fn truncate_reason_with_a_limit_below_the_mark_length_yields_only_the_mark() {
        assert_eq!(truncate_reason("abcdef", 2), "...");
    }

    #[test]
    fn action_failed_body_truncates_the_reason_at_the_notice_limit() {
        install_language(Language::En);
        let at_limit = "є".repeat(NOTICE_REASON_MAX_CHARS);
        let over_limit = "є".repeat(NOTICE_REASON_MAX_CHARS + 1);
        assert_eq!(
            shown(&failed("A", Some(&at_limit))).body,
            format!("A - {at_limit}")
        );
        assert_eq!(
            shown(&failed("A", Some(&over_limit))).body,
            format!(
                "A - {}...",
                "є".repeat(NOTICE_REASON_MAX_CHARS - TRUNCATION_MARK.len())
            )
        );
    }

    #[test]
    fn desktop_notice_contains_no_fluent_isolate_marks() {
        let isolated = "\u{2068}Shout\u{2069}out";
        for lang in [Language::En, Language::Uk] {
            install_language(lang);
            for intent in [
                failed(isolated, Some(isolated)),
                failed(isolated, None),
                NotificationIntent::ActionFailuresFolded { count: 3 },
            ] {
                let notice = shown(&intent);
                let text = format!("{}{}", notice.title, notice.body);
                assert!(
                    !text.contains([FLUENT_ISOLATE_START, FLUENT_ISOLATE_END]),
                    "{lang:?} {intent:?} leaked isolates: {text:?}"
                );
            }
        }
    }

    #[test]
    fn connection_loss_cue_plans_a_cue_not_a_notice() {
        install_language(Language::En);
        let intent = NotificationIntent::ConnectionLossCue {
            platform_ids: vec!["twitch".to_owned(), "kick".to_owned()],
        };
        assert_eq!(plan_for(&intent), NoticePlan::PlayCue);
    }

    #[test]
    fn fallback_log_reports_only_the_first_failure_of_a_session() {
        let mut log = FallbackLog::default();
        let reports: Vec<bool> = (0..3).map(|_| log.first_failure()).collect();
        assert_eq!(reports, [true, false, false]);
    }
}
