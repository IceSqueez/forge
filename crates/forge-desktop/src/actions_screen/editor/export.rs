use super::*;
use crate::async_bridge;
use forge_components::tr;
use gpui::Context;

fn sanitize_action_stem(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "action".to_owned()
    } else {
        trimmed.to_owned()
    }
}

async fn export_action_to_chosen_file(action: Action) -> Result<std::path::PathBuf, String> {
    let json = serde_json::to_string_pretty(&action).map_err(|e| e.to_string())?;
    let default_name = format!("{}.forge.json", sanitize_action_stem(&action.name));
    let filter = async_bridge::DialogFilter {
        name: "JSON".to_owned(),
        extensions: vec!["json"],
    };
    let path = async_bridge::save_file(Some(filter), Some(default_name)).await?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

impl ScreenActionsView {
    pub(super) fn export_json(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let action = detail.action.clone();
        self.header_menu_open = None;
        async_bridge::spawn_dialog(
            &self.rt_handle,
            export_action_to_chosen_file(action),
            |_this, result, cx| match result {
                Ok(path) => {
                    let shown = path.display().to_string();
                    cx.push_toast(
                        ToastKind::Success,
                        tr!("action_editor_export_done", path = shown.as_str()),
                    );
                }
                Err(reason) if reason == async_bridge::DIALOG_CANCELLED => {}
                Err(reason) => {
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("action_editor_export_failed", error = reason.as_str()),
                    );
                }
            },
            cx,
        );
        cx.notify();
    }
}
