use std::sync::Arc;

use forge_audio::{NullAudioEventSink, alert_cue, play_on_default_output};
use forge_components::{PushToast, ToastKind, tr};
use forge_runtime::sub_action_runners::{
    DesktopNotice, NotifyPort, NotifyUrgency, OsPortError, SystemNotifyPort,
};
use forge_runtime::{
    ActionFailureNotice, NotificationIntent, NotificationIntents, NotificationPolicyHandle,
};
use gpui::AsyncApp;
use tokio::runtime::Handle;

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
    let body = match notice
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
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

pub fn start_system_notices(cx: &mut AsyncApp, policy: &NotificationPolicyHandle, rt: Handle) {
    let Some(intents) = policy.take_intents() else {
        tracing::warn!("notification intents already claimed; system notices stay off");
        return;
    };
    cx.spawn(async move |cx| drain_intents(intents, rt, cx).await)
        .detach();
}

async fn drain_intents(mut intents: NotificationIntents, rt: Handle, cx: &mut AsyncApp) {
    let mut fallback_log = FallbackLog::default();
    while let Some(intent) = intents.next().await {
        let plan = cx.update(|_| plan_for(&intent));
        match plan {
            NoticePlan::Show(text) => {
                if let Err(error) = show_off_thread(&rt, text.desktop_notice()).await {
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

async fn show_off_thread(rt: &Handle, notice: DesktopNotice) -> Result<(), OsPortError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn_blocking(move || {
        let _ = tx.send(SystemNotifyPort.show(notice));
    });
    rx.await
        .unwrap_or_else(|_| Err(OsPortError::Failed("notice task ended".to_owned())))
}

fn play_cue_detached(rt: &Handle) {
    rt.spawn(async {
        if let Err(error) = play_on_default_output(alert_cue(), Arc::new(NullAudioEventSink)).await
        {
            tracing::warn!(error = %error, "connection loss cue could not play");
        }
    });
}
