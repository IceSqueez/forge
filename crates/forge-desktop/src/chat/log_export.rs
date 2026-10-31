use super::ChatView;
use crate::async_bridge;
use crate::chat_row::ChatBody;
use crate::repo_failure::RepoFailure;
use forge_components::{PushToast, ToastAction, ToastKind, tr};
use gpui::{App, Context};
use std::path::PathBuf;
use std::time::Duration;
use time::OffsetDateTime;

const EXPORT_TOAST: Duration = Duration::from_secs(8);
const EXPORT_HEADER_RULE: usize = 48;

fn export_stamp(at: time::OffsetDateTime) -> String {
    at.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| at.unix_timestamp().to_string())
}

pub(super) fn body_export_text(body: &ChatBody) -> String {
    match body {
        ChatBody::Message(text) => text.to_string(),
        ChatBody::Command { command, .. } => command.to_string(),
        ChatBody::Cheer { text, .. } => text.to_string(),
        ChatBody::Subscription {
            descriptor,
            message,
            ..
        } => message
            .as_ref()
            .map_or_else(|| descriptor.to_string(), |m| m.to_string()),
        ChatBody::Raid { descriptor, .. } => descriptor.to_string(),
    }
}

impl ChatView {
    pub(super) fn export_chat_log(&self, cx: &mut Context<Self>) {
        let feed = self.feed.read(cx);
        let mut lines: Vec<String> = Vec::new();
        for msg in feed.messages().iter().filter(|m| self.row_visible(m)) {
            let text = body_export_text(&msg.body);
            if msg.username.is_empty() {
                lines.push(format!("[{}] {}", export_stamp(msg.received_at), text));
            } else {
                lines.push(format!(
                    "[{}] {}: {}",
                    export_stamp(msg.received_at),
                    msg.username,
                    text
                ));
            }
        }

        let stamp = OffsetDateTime::now_utc();
        let when = export_stamp(stamp);
        let title = tr!(
            "chat_export_header",
            when = when.as_str(),
            lines = lines.len() as i64
        );
        let rule = "-".repeat(EXPORT_HEADER_RULE);
        let contents = format!("{title}\n{rule}\n{}", lines.join("\n"));
        let filename = format!("forge-chat-{}.txt", stamp.unix_timestamp());

        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let path = forge_paths::data_dir().join(filename);
                tokio::fs::write(&path, contents)
                    .await
                    .map(|()| path)
                    .map_err(RepoFailure::from)
            },
            |_this, result, cx| Self::report_chat_export(result, cx),
            cx,
        );
    }

    fn report_chat_export(result: Result<PathBuf, RepoFailure>, cx: &mut Context<Self>) {
        match result {
            Ok(path) => {
                tracing::info!(path = %path.display(), "chat log exported");
                let file = path.display().to_string();
                cx.push_toast_full(
                    ToastKind::Success,
                    tr!("chat_export_done", file = file.as_str()),
                    None,
                    Some(ToastAction::new(
                        tr!("chat_export_open_folder"),
                        move |_window, app: &mut App| app.reveal_path(&path),
                    )),
                    EXPORT_TOAST,
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "chat log export failed");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("chat_export_failed", error = e.text()),
                );
            }
        }
    }
}
