use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    ConfirmTone, Density, FONT_LG, FONT_SM, FONT_XS, ForgePalette, Icon, OverlayPosition, Spacing,
    ToastKind, body_family, card, confirm_modal, drive_overlay_focus, field_hint,
    ghost_button_with_icon, icon, mono_family, overlay, segment, segmented, setting_row, spacing,
    tr,
};
use forge_storage::{
    DEFAULT_DIAGNOSTIC_LOG_LEVEL, DataProvider, SettingsRepo, diagnostic_log_level,
    set_diagnostic_log_level,
};
use forge_types::LogLevel;
use gpui::{
    AnyElement, ClickEvent, Context, FocusHandle, FontWeight, Pixels, Rgba, ScrollHandle,
    SharedString, Window, div, prelude::*, px,
};
use tracing::Level;

use crate::async_bridge;
use crate::log_archive;
use crate::log_tail::{LogLine, LogTail};
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const TAIL_REFRESH: Duration = Duration::from_secs(2);
const TAIL_MAX_HEIGHT: Pixels = px(320.0);
const TAIL_LINE_HEIGHT: Pixels = px(20.0);
const LEVEL_COLUMN: Pixels = px(38.0);
const FOLLOW_SLACK: Pixels = px(24.0);
const BUNDLE_FILE_NAME: &str = "forge-diagnostics.txt";
const NOTE_GLYPH: Pixels = px(14.0);

pub struct SettingsDiagnosticsView {
    tail: LogTail,
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    lines: Vec<LogLine>,
    level: LogLevel,
    env_overridden: bool,
    scroll: ScrollHandle,
    active: bool,
    clear_pending: bool,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
}

impl SettingsDiagnosticsView {
    pub fn new(
        tail: LogTail,
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::spawn_refresher(cx);
        let mut view = Self {
            tail,
            backend,
            rt_handle,
            lines: Vec::new(),
            level: DEFAULT_DIAGNOSTIC_LOG_LEVEL,
            env_overridden: crate::log_level::env_overridden(),
            scroll: ScrollHandle::new(),
            active: false,
            clear_pending: false,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
        };
        view.load_level(cx);
        view
    }

    fn load_level(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                diagnostic_log_level(repo.as_ref())
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| {
                match result {
                    Ok(level) => this.level = level,
                    Err(message) => {
                        tracing::warn!(error = %message, "could not read the diagnostics log level");
                    }
                }
                cx.notify();
            },
            cx,
        );
    }

    fn select_level(&mut self, level: LogLevel, cx: &mut Context<Self>) {
        if self.env_overridden || self.level == level {
            return;
        }
        self.level = level.clone();
        if !crate::log_level::apply(&level) {
            cx.push_toast(
                ToastKind::Error,
                tr!("settings_diagnostics_level_apply_failed"),
            );
        }
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.rt_handle.spawn(async move {
            if let Err(e) = set_diagnostic_log_level(repo.as_ref(), &level).await {
                tracing::warn!(error = %e, "could not persist the diagnostics log level");
            }
        });
        cx.notify();
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.refresh(cx);
        }
    }

    fn spawn_refresher(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TAIL_REFRESH).await;
                let alive = this.update(cx, |this, cx| {
                    if this.active {
                        this.refresh(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let follow = self.is_at_bottom();
        self.lines = self.tail.snapshot();
        if follow {
            self.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    fn is_at_bottom(&self) -> bool {
        let max = self.scroll.max_offset().y;
        max <= px(0.0) || self.scroll.offset().y <= FOLLOW_SLACK - max
    }

    fn log_dir() -> std::path::PathBuf {
        forge_platform_core::paths::data_dir().join("logs")
    }

    fn open_log_dir(&mut self, cx: &mut Context<Self>) {
        cx.reveal_path(&Self::log_dir());
    }

    fn export_bundle(&mut self, cx: &mut Context<Self>) {
        let dir = Self::log_dir();
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async move {
                let filter = async_bridge::DialogFilter {
                    name: "Text".to_owned(),
                    extensions: &["txt"],
                };
                let path = async_bridge::save_file(Some(filter), Some(BUNDLE_FILE_NAME.to_owned()))
                    .await?;
                let bundle = tokio::task::spawn_blocking(move || log_archive::bundle(&dir))
                    .await
                    .map_err(|e| e.to_string())??;
                tokio::fs::write(&path, bundle)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(path)
            },
            |_this, result, cx| match result {
                Ok(path) => {
                    let path = path.display().to_string();
                    cx.push_toast(
                        ToastKind::Success,
                        tr!("settings_diagnostics_exported", path = path.as_str()),
                    );
                }
                Err(e) if e == async_bridge::DIALOG_CANCELLED => {}
                Err(e) => {
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("settings_diagnostics_export_failed", error = e.as_str()),
                    );
                }
            },
            cx,
        );
    }

    fn request_clear(&mut self, cx: &mut Context<Self>) {
        self.clear_pending = true;
        cx.notify();
    }

    fn cancel_clear(&mut self, cx: &mut Context<Self>) {
        self.clear_pending = false;
        cx.notify();
    }

    fn clear_logs(&mut self, cx: &mut Context<Self>) {
        self.clear_pending = false;
        let dir = Self::log_dir();
        async_bridge::run_blocking(
            &self.rt_handle,
            move || log_archive::clear(&dir),
            |this, result, cx| {
                match result {
                    Ok(()) => {
                        this.tail.clear();
                        this.lines.clear();
                        cx.push_toast(ToastKind::Success, tr!("settings_diagnostics_cleared"));
                    }
                    Err(e) => cx.push_toast(
                        ToastKind::Error,
                        tr!("settings_diagnostics_clear_failed", error = e.as_str()),
                    ),
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    fn header(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::Bug, px(18.0), palette.brand))
                    .child(
                        div()
                            .font_family(body_family())
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_LG)
                            .text_color(palette.text_primary)
                            .child(tr!("settings_diagnostics_section_title")),
                    ),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_diagnostics_subtitle")),
            )
    }

    fn actions(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                ghost_button_with_icon(
                    Icon::FolderOpen,
                    tr!("settings_diagnostics_open_log_dir"),
                    palette,
                )
                .density(density)
                .on_click(
                    "settings-diagnostics-open",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.open_log_dir(cx)),
                ),
            )
            .child(
                ghost_button_with_icon(
                    Icon::Download,
                    tr!("settings_diagnostics_export_bundle"),
                    palette,
                )
                .density(density)
                .on_click(
                    "settings-diagnostics-export",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.export_bundle(cx)),
                ),
            )
            .child(
                ghost_button_with_icon(
                    Icon::Trash,
                    tr!("settings_diagnostics_clear_logs"),
                    palette,
                )
                .density(density)
                .ink(palette.random)
                .on_click(
                    "settings-diagnostics-clear",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.request_clear(cx)),
                ),
            )
    }

    fn level_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let segments = crate::log_level::SELECTABLE
            .iter()
            .map(|level| {
                let choice = level.clone();
                segment(
                    SharedString::from(format!("settings-diagnostics-level-{}", level_key(level))),
                    tr!(level_label_key(level)),
                    self.level == *level,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.select_level(choice.clone(), cx)
                    }),
                )
                .disabled(self.env_overridden)
            })
            .collect();

        let hint = if self.env_overridden {
            tr!("settings_diagnostics_level_env_locked")
        } else {
            tr!("settings_diagnostics_level_hint")
        };

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(setting_row(
                tr!("settings_diagnostics_level_label"),
                Some(hint.into()),
                segmented(segments, palette),
                palette,
                density,
            ))
            .child(note_row(
                tr!("settings_diagnostics_level_env_hatch"),
                palette,
                density,
            ));

        card(body, palette).full_width()
    }

    fn tail_body(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let pad = spacing(Spacing::Sm, density);
        if self.lines.is_empty() {
            return div()
                .p(pad)
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("settings_diagnostics_tail_empty"))
                .into_any_element();
        }

        let mut list = div()
            .id("settings-diagnostics-tail")
            .track_scroll(&self.scroll)
            .max_h(TAIL_MAX_HEIGHT)
            .overflow_y_scroll()
            .p(pad)
            .font_family(mono_family())
            .text_size(FONT_XS)
            .line_height(TAIL_LINE_HEIGHT);
        for line in &self.lines {
            list = list.child(log_row(line, palette, density));
        }
        list.into_any_element()
    }

    fn clear_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.clear_pending {
            return None;
        }

        let modal = confirm_modal(
            tr!("settings_diagnostics_clear_confirm_title"),
            tr!("settings_diagnostics_clear_confirm_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .on_cancel(
            "settings-diagnostics-clear-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_clear(cx)),
        )
        .on_confirm(
            "settings-diagnostics-clear-confirm",
            tr!("settings_diagnostics_clear_confirm_action"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.clear_logs(cx)),
        );

        let weak = cx.entity().downgrade();
        Some(
            overlay(modal, palette)
                .position(OverlayPosition::Center)
                .dismiss_on_escape(&self.overlay_focus)
                .on_dismiss("settings-diagnostics-clear-dismiss", move |_window, cx| {
                    let _ = weak.update(cx, |this, cx| this.cancel_clear(cx));
                })
                .into_any_element(),
        )
    }
}

impl Render for SettingsDiagnosticsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        drive_overlay_focus(
            self.clear_pending,
            &self.overlay_focus,
            &mut self.focus_restore,
            window,
            cx,
        );

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(self.header(&palette, density))
            .child(self.level_card(&palette, density, cx))
            .child(self.actions(&palette, density, cx))
            .child(
                card(self.tail_body(&palette, density), &palette)
                    .padding(px(0.0))
                    .full_width(),
            );

        div()
            .flex()
            .flex_col()
            .child(body)
            .children(self.clear_overlay(&palette, cx))
    }
}

fn level_key(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

fn level_label_key(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "settings_diagnostics_level_trace",
        LogLevel::Debug => "settings_diagnostics_level_debug",
        LogLevel::Info => "settings_diagnostics_level_info",
        LogLevel::Warn => "settings_diagnostics_level_warn",
        LogLevel::Error => "settings_diagnostics_level_error",
    }
}

fn note_row(
    text: impl Into<SharedString>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .flex_none()
                .child(icon(Icon::InfoCircle, NOTE_GLYPH, palette.info)),
        )
        .child(field_hint(text.into(), palette))
}

fn log_row(line: &LogLine, palette: &ForgePalette, density: Density) -> impl IntoElement {
    div()
        .flex()
        .gap(spacing(Spacing::Sm, density))
        .child(
            div()
                .flex_none()
                .text_color(palette.text_faint)
                .child(format_time(line)),
        )
        .child(
            div()
                .flex_none()
                .w(LEVEL_COLUMN)
                .font_weight(FontWeight::MEDIUM)
                .text_color(level_color(line.level, palette))
                .child(line.level.as_str()),
        )
        .child(
            div()
                .flex_none()
                .text_color(palette.brand)
                .child(SharedString::from(line.target)),
        )
        .child(
            div()
                .flex_1()
                .text_color(palette.text_primary)
                .child(SharedString::from(line.message.clone())),
        )
}

fn format_time(line: &LogLine) -> SharedString {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        line.at.hour(),
        line.at.minute(),
        line.at.second(),
        line.at.millisecond(),
    )
    .into()
}

fn level_color(level: Level, palette: &ForgePalette) -> Rgba {
    if level == Level::ERROR {
        palette.random
    } else if level == Level::WARN {
        palette.warning
    } else if level == Level::INFO {
        palette.success
    } else {
        palette.text_muted
    }
}
