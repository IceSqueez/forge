use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    Density, FONT_LG, FONT_SM, ForgePalette, Icon, InputEvent, Spacing, TextInput, ToastAction,
    ToastKind, body_family, card, field_hint, field_title, icon, mono_family, primary_button,
    spacing, tr,
};
use forge_storage::{
    DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT, DataProvider, SettingsRepo, chat_history_display_limit,
    chat_history_store_limit, event_log_retention_days, set_chat_history_display_limit,
    set_chat_history_store_limit, set_event_log_retention_days,
};
use gpui::{
    ClickEvent, Context, Entity, FontWeight, SharedString, Subscription, Window, div, prelude::*,
    px,
};

use crate::async_bridge;
use crate::data_backup;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const DEFAULT_STORE_LIMIT: u32 = 5000;
const DEFAULT_RETENTION_DAYS: u32 = 7;
const MIN_RETENTION_DAYS: u32 = 1;
const MAX_RETENTION_DAYS: u32 = 365;
const BACKUP_TOAST_DURATION: Duration = Duration::from_secs(10);

pub struct SettingsStorageView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,

    store_limit: u32,
    display_limit: u32,
    retention_days: u32,
    backing_up: bool,

    store_input: Entity<TextInput>,
    display_input: Entity<TextInput>,
    retention_input: Entity<TextInput>,
    _subs: Vec<Subscription>,
}

impl SettingsStorageView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let store_input = cx.new(|cx| {
            TextInput::new(DEFAULT_STORE_LIMIT.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });
        let display_input = cx.new(|cx| {
            TextInput::new(DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });
        let retention_input = cx.new(|cx| {
            TextInput::new(DEFAULT_RETENTION_DAYS.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });

        let subs = vec![
            cx.subscribe(&store_input, |this, _input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                    this.commit_store(cx);
                }
            }),
            cx.subscribe(&display_input, |this, _input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                    this.commit_display(cx);
                }
            }),
            cx.subscribe(&retention_input, |this, _input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                    this.commit_retention(cx);
                }
            }),
        ];

        let mut view = Self {
            backend,
            rt_handle,
            store_limit: DEFAULT_STORE_LIMIT,
            display_limit: DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT,
            retention_days: DEFAULT_RETENTION_DAYS,
            backing_up: false,
            store_input,
            display_input,
            retention_input,
            _subs: subs,
        };
        view.load(cx);
        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            load_limits(repo),
            |this, result, cx| this.apply_loaded(result, cx),
            cx,
        );
    }

    fn apply_loaded(&mut self, result: Result<(u32, u32, u32), String>, cx: &mut Context<Self>) {
        match result {
            Ok((store, display, retention)) => {
                self.store_limit = store;
                self.display_limit = display;
                self.retention_days = retention;
                self.store_input
                    .update(cx, |i, cx| i.set_content(store.to_string(), cx));
                self.display_input
                    .update(cx, |i, cx| i.set_content(display.to_string(), cx));
                self.retention_input
                    .update(cx, |i, cx| i.set_content(retention.to_string(), cx));
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to load storage settings");
            }
        }
        cx.notify();
    }

    fn commit_store(&mut self, cx: &mut Context<Self>) {
        match parse_limit(self.store_input.read(cx).content()) {
            Some(value) => {
                self.store_limit = value;
                self.store_input
                    .update(cx, |i, cx| i.set_content(value.to_string(), cx));
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.rt_handle.spawn(async move {
                    if let Err(e) = set_chat_history_store_limit(repo.as_ref(), value).await {
                        tracing::warn!(error = %e, "failed to persist chat history keep limit");
                    }
                });
            }
            None => {
                let restore = self.store_limit.to_string();
                self.store_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn commit_display(&mut self, cx: &mut Context<Self>) {
        match parse_limit(self.display_input.read(cx).content()) {
            Some(value) => {
                self.display_limit = value;
                self.display_input
                    .update(cx, |i, cx| i.set_content(value.to_string(), cx));
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.rt_handle.spawn(async move {
                    if let Err(e) = set_chat_history_display_limit(repo.as_ref(), value).await {
                        tracing::warn!(error = %e, "failed to persist chat history display limit");
                    }
                });
            }
            None => {
                let restore = self.display_limit.to_string();
                self.display_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn commit_retention(&mut self, cx: &mut Context<Self>) {
        match parse_retention_days(self.retention_input.read(cx).content()) {
            Some(value) => {
                self.retention_days = value;
                self.retention_input
                    .update(cx, |i, cx| i.set_content(value.to_string(), cx));
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.rt_handle.spawn(async move {
                    if let Err(e) = set_event_log_retention_days(repo.as_ref(), value).await {
                        tracing::warn!(error = %e, "failed to persist event log retention days");
                    }
                });
            }
            None => {
                let restore = self.retention_days.to_string();
                self.retention_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn backup_now(&mut self, cx: &mut Context<Self>) {
        if self.backing_up {
            return;
        }
        self.backing_up = true;
        cx.notify();
        async_bridge::run_async(
            &self.rt_handle,
            data_backup::run_backup(
                Arc::clone(&self.backend),
                forge_platform_core::paths::data_dir(),
            ),
            |this, result, cx| this.apply_backup_result(result, cx),
            cx,
        );
    }

    fn apply_backup_result(&mut self, result: Result<PathBuf, String>, cx: &mut Context<Self>) {
        self.backing_up = false;
        match result {
            Ok(path) => {
                tracing::info!(path = %path.display(), "data backup created");
                let shown = path.display().to_string();
                cx.push_toast_full(
                    ToastKind::Success,
                    tr!("settings_storage_backup_done", path = shown.as_str()),
                    None,
                    Some(ToastAction::new(
                        tr!("settings_storage_backup_reveal"),
                        move |_window, app| app.reveal_path(&path),
                    )),
                    BACKUP_TOAST_DURATION,
                );
            }
            Err(message) => {
                tracing::warn!(error = %message, "data backup failed");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("settings_storage_backup_failed", error = message.as_str()),
                );
            }
        }
        cx.notify();
    }

    fn limit_field(
        &self,
        label: impl Into<SharedString>,
        hint: impl Into<SharedString>,
        input: Entity<TextInput>,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(label, palette))
            .child(field_hint(hint, palette))
            .child(div().max_w(px(200.0)).child(input))
    }
}

impl Render for SettingsStorageView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let db_path = forge_platform_core::paths::data_dir().join("forge.db");
        let backup_label = if self.backing_up {
            tr!("settings_storage_backup_btn_busy")
        } else {
            tr!("settings_storage_backup_btn")
        };
        let backup_btn = primary_button(backup_label, &palette)
            .busy(self.backing_up)
            .on_click(
                "settings-db-backup",
                cx.listener(|this, _: &ClickEvent, _, cx| this.backup_now(cx)),
            );

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(info_row(
                tr!("settings_storage_db_path_label"),
                db_path.display().to_string(),
                &palette,
            ))
            .child(backup_btn)
            .child(field_hint(tr!("settings_storage_backup_hint"), &palette))
            .child(self.limit_field(
                tr!("settings_storage_keep_limit_label"),
                tr!("settings_storage_keep_limit_hint"),
                self.store_input.clone(),
                &palette,
                density,
            ))
            .child(self.limit_field(
                tr!("settings_storage_display_limit_label"),
                tr!("settings_storage_display_limit_hint"),
                self.display_input.clone(),
                &palette,
                density,
            ))
            .child(self.limit_field(
                tr!("settings_storage_retention_label"),
                tr!("settings_storage_retention_hint"),
                self.retention_input.clone(),
                &palette,
                density,
            ));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(
                Icon::Folder,
                tr!("settings_storage_section_title"),
                &palette,
            ))
            .child(card(body, &palette))
    }
}

async fn load_limits(repo: Arc<dyn SettingsRepo>) -> Result<(u32, u32, u32), String> {
    let store = chat_history_store_limit(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    let display = chat_history_display_limit(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    let retention = event_log_retention_days(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    Ok((store, display, retention))
}

fn parse_limit(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok().filter(|v| *v >= 1)
}

fn parse_retention_days(raw: &str) -> Option<u32> {
    raw.trim()
        .parse::<u32>()
        .ok()
        .filter(|v| (MIN_RETENTION_DAYS..=MAX_RETENTION_DAYS).contains(v))
}

fn pane_header(
    glyph: Icon,
    title: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .child(icon(glyph, px(18.0), palette.brand))
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(title.into()),
        )
}

fn info_row(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py(spacing(Spacing::Xs, Density::Cozy))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(value.into()),
        )
}

#[cfg(test)]
mod tests {
    use super::parse_limit;

    #[test]
    fn parse_limit_accepts_positive_integers_and_rejects_everything_else() {
        for (raw, expected) in [
            ("500", Some(500)),
            (" 42 ", Some(42)),
            ("1", Some(1)),
            ("0", None),
            ("", None),
            ("abc", None),
            ("-5", None),
            ("1.5", None),
        ] {
            assert_eq!(parse_limit(raw), expected, "parse_limit({raw:?})");
        }
    }
}
