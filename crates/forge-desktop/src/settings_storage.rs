use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    Density, FONT_LG, FONT_SM, ForgePalette, Icon, InputEvent, Spacing, TextInput, ToastAction,
    ToastKind, body_family, card, field_hint, field_title, icon, mono_family, primary_button,
    segment, segmented, spacing, tr,
};
use forge_runtime::ChatHistoryRetentionHandle;
use forge_storage::{
    DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT, DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
    DEFAULT_EVENT_LOG_RETENTION_DAYS, DataProvider, MAX_CHAT_HISTORY_PER_VIEWER_LIMIT,
    MAX_EVENT_LOG_RETENTION_DAYS, MIN_CHAT_HISTORY_PER_VIEWER_LIMIT, MIN_EVENT_LOG_RETENTION_DAYS,
    SettingsRepo, UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT, chat_history_display_limit,
    chat_history_per_viewer_limit, event_log_retention_days, set_chat_history_display_limit,
    set_chat_history_per_viewer_limit, set_event_log_retention_days,
};
use gpui::{
    ClickEvent, Context, Entity, FontWeight, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};

use crate::async_bridge;
use crate::data_backup;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const BACKUP_TOAST_DURATION: Duration = Duration::from_secs(10);
const LIMIT_INPUT_MAX_W: Pixels = px(200.0);
const PER_VIEWER_PRESETS: [u32; 3] = [25, 50, 100];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PerViewerChoice {
    Preset(u32),
    Unlimited,
    Custom,
}

pub(crate) fn per_viewer_choice(limit: u32, custom_selected: bool) -> PerViewerChoice {
    if custom_selected {
        PerViewerChoice::Custom
    } else if limit == UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT {
        PerViewerChoice::Unlimited
    } else if PER_VIEWER_PRESETS.contains(&limit) {
        PerViewerChoice::Preset(limit)
    } else {
        PerViewerChoice::Custom
    }
}

pub struct SettingsStorageView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    retention: ChatHistoryRetentionHandle,

    per_viewer_limit: u32,
    custom_selected: bool,
    display_limit: u32,
    retention_days: u32,
    backing_up: bool,

    per_viewer_input: Entity<TextInput>,
    display_input: Entity<TextInput>,
    retention_input: Entity<TextInput>,
    _subs: Vec<Subscription>,
}

impl SettingsStorageView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        retention: ChatHistoryRetentionHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let per_viewer_input = cx.new(|cx| {
            TextInput::new(DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });
        let display_input = cx.new(|cx| {
            TextInput::new(DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });
        let retention_input = cx.new(|cx| {
            TextInput::new(DEFAULT_EVENT_LOG_RETENTION_DAYS.to_string(), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });

        let subs = vec![
            cx.subscribe(&per_viewer_input, |this, _input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submitted(_) | InputEvent::Blurred(_)) {
                    this.commit_custom_per_viewer(cx);
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
            retention,
            per_viewer_limit: DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
            custom_selected: false,
            display_limit: DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT,
            retention_days: DEFAULT_EVENT_LOG_RETENTION_DAYS,
            backing_up: false,
            per_viewer_input,
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
            Ok((per_viewer, display, retention)) => {
                self.per_viewer_limit = per_viewer;
                self.custom_selected = false;
                self.display_limit = display;
                self.retention_days = retention;
                self.per_viewer_input
                    .update(cx, |i, cx| i.set_content(per_viewer.to_string(), cx));
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

    fn select_per_viewer_preset(&mut self, limit: u32, cx: &mut Context<Self>) {
        self.custom_selected = false;
        self.per_viewer_input
            .update(cx, |i, cx| i.set_content(limit.to_string(), cx));
        self.apply_per_viewer_limit(limit);
        cx.notify();
    }

    fn select_custom_per_viewer(&mut self, cx: &mut Context<Self>) {
        self.custom_selected = true;
        cx.notify();
    }

    fn commit_custom_per_viewer(&mut self, cx: &mut Context<Self>) {
        match parse_per_viewer_limit(self.per_viewer_input.read(cx).content()) {
            Some(value) => {
                if value == UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT {
                    self.custom_selected = false;
                }
                self.per_viewer_input
                    .update(cx, |i, cx| i.set_content(value.to_string(), cx));
                self.apply_per_viewer_limit(value);
            }
            None => {
                let restore = self.per_viewer_limit.to_string();
                self.per_viewer_input
                    .update(cx, |i, cx| i.set_content(restore, cx));
            }
        }
        cx.notify();
    }

    fn apply_per_viewer_limit(&mut self, limit: u32) {
        if self.per_viewer_limit == limit {
            return;
        }
        self.per_viewer_limit = limit;
        self.retention.set_per_viewer_limit(limit);
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        self.rt_handle.spawn(async move {
            if let Err(e) = set_chat_history_per_viewer_limit(repo.as_ref(), limit).await {
                tracing::warn!(error = %e, "failed to persist chat history per-viewer limit");
            }
        });
    }

    fn per_viewer_field(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let choice = per_viewer_choice(self.per_viewer_limit, self.custom_selected);
        let mut segments: Vec<_> = PER_VIEWER_PRESETS
            .iter()
            .map(|&limit| {
                segment(
                    SharedString::from(format!("settings-storage-per-viewer-{limit}")),
                    limit.to_string(),
                    choice == PerViewerChoice::Preset(limit),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.select_per_viewer_preset(limit, cx)
                    }),
                )
            })
            .collect();
        segments.push(segment(
            "settings-storage-per-viewer-unlimited",
            tr!("settings_storage_per_viewer_limit_unlimited"),
            choice == PerViewerChoice::Unlimited,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.select_per_viewer_preset(UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT, cx)
            }),
        ));
        segments.push(segment(
            "settings-storage-per-viewer-custom",
            tr!("settings_storage_per_viewer_limit_custom"),
            choice == PerViewerChoice::Custom,
            cx.listener(|this, _: &ClickEvent, _, cx| this.select_custom_per_viewer(cx)),
        ));

        let mut controls = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(segmented(segments, palette));
        if choice == PerViewerChoice::Custom {
            controls = controls.child(
                div()
                    .w(LIMIT_INPUT_MAX_W)
                    .min_w(LIMIT_INPUT_MAX_W)
                    .max_w(LIMIT_INPUT_MAX_W)
                    .child(self.per_viewer_input.clone()),
            );
        }

        let mut field = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(
                tr!("settings_storage_per_viewer_limit_label"),
                palette,
            ))
            .child(field_hint(
                tr!("settings_storage_per_viewer_limit_hint"),
                palette,
            ))
            .child(controls);
        if choice == PerViewerChoice::Unlimited {
            field = field.child(field_hint(
                tr!("settings_storage_per_viewer_limit_unlimited_hint"),
                palette,
            ));
        }
        field
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
            .child(div().max_w(LIMIT_INPUT_MAX_W).child(input))
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
            .child(self.per_viewer_field(&palette, density, cx))
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
    let per_viewer = chat_history_per_viewer_limit(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    let display = chat_history_display_limit(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    let retention = event_log_retention_days(repo.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    Ok((per_viewer, display, retention))
}

fn parse_limit(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok().filter(|v| *v >= 1)
}

fn parse_per_viewer_limit(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok().filter(|v| {
        *v == UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT
            || (MIN_CHAT_HISTORY_PER_VIEWER_LIMIT..=MAX_CHAT_HISTORY_PER_VIEWER_LIMIT).contains(v)
    })
}

fn parse_retention_days(raw: &str) -> Option<u32> {
    raw.trim()
        .parse::<u32>()
        .ok()
        .filter(|v| (MIN_EVENT_LOG_RETENTION_DAYS..=MAX_EVENT_LOG_RETENTION_DAYS).contains(v))
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
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use forge_events::{Event, EventSource};
    use forge_runtime::{EventBus, spawn_chat_history_persistence};
    use forge_storage::chat_history::MockChatHistoryRepo;
    use forge_storage::{DataProvider, SettingsRepo, chat_history_per_viewer_limit};
    use forge_types::{ChatPayload, ModerationMarks};
    use gpui::{AppContext, Entity, TestAppContext};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
    use tokio::time::{Duration, timeout};

    use super::{
        PerViewerChoice, SettingsStorageView, parse_limit, parse_per_viewer_limit,
        parse_retention_days, per_viewer_choice,
    };
    use crate::test_support::{
        SettingWrite, TestBackend, install_presentation, pump, quiet_bus, runtime, test_backend,
    };

    const RECV_TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn per_viewer_choice_marks_a_preset_only_when_it_matches_and_custom_is_not_chosen() {
        for (limit, custom_selected, expected) in [
            (25, false, PerViewerChoice::Preset(25)),
            (50, false, PerViewerChoice::Preset(50)),
            (100, false, PerViewerChoice::Preset(100)),
            (75, false, PerViewerChoice::Custom),
            (1, false, PerViewerChoice::Custom),
            (50, true, PerViewerChoice::Custom),
        ] {
            assert_eq!(
                per_viewer_choice(limit, custom_selected),
                expected,
                "limit={limit} custom_selected={custom_selected}"
            );
        }
    }

    #[test]
    fn parse_per_viewer_limit_accepts_exactly_the_range_storage_honours() {
        for (raw, expected) in [
            ("0", None),
            ("1", Some(1)),
            ("10000", Some(10_000)),
            ("10001", None),
            (" 250 ", Some(250)),
            ("abc", None),
            ("", None),
            ("-1", None),
            ("2.5", None),
        ] {
            assert_eq!(
                parse_per_viewer_limit(raw),
                expected,
                "parse_per_viewer_limit({raw:?})"
            );
        }
    }

    struct Rig {
        rt: tokio::runtime::Runtime,
        backend: Arc<TestBackend>,
        writes: UnboundedReceiver<SettingWrite>,
        bus: Arc<EventBus>,
        retained_at: UnboundedReceiver<usize>,
        view: Entity<SettingsStorageView>,
    }

    fn rig(cx: &mut TestAppContext) -> Rig {
        install_presentation(cx);
        let rt = runtime();
        let (backend, writes) = test_backend();
        let bus = quiet_bus(&rt);
        let (retained_tx, retained_at) = unbounded_channel();
        let mut repo = MockChatHistoryRepo::new();
        repo.expect_append_batch().returning(|_| Ok(()));
        repo.expect_apply_retention()
            .returning(move |_, per_viewer| {
                retained_tx.send(per_viewer).unwrap();
                Ok(0)
            });
        let retention = rt.block_on(async {
            spawn_chat_history_persistence(
                Arc::clone(&bus),
                Arc::new(repo),
                Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            )
        });
        pump(&rt);
        let view = cx.update(|cx| {
            cx.new(|cx| {
                SettingsStorageView::new(
                    Arc::clone(&backend) as Arc<dyn DataProvider>,
                    rt.handle().clone(),
                    retention,
                    cx,
                )
            })
        });
        pump(&rt);
        cx.run_until_parked();
        Rig {
            rt,
            backend,
            writes,
            bus,
            retained_at,
            view,
        }
    }

    fn chat_message() -> Event {
        let payload = ChatPayload {
            platform_msg_id: "m1".to_string(),
            author: "bob".to_string(),
            author_color: None,
            segments: vec![],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        };
        Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({ (ChatPayload::KEY): payload }),
        )
    }

    impl Rig {
        fn next_retention_limit(&mut self) -> usize {
            let bus = Arc::clone(&self.bus);
            let retained_at = &mut self.retained_at;
            self.rt.block_on(async move {
                bus.publish(chat_message());
                timeout(RECV_TIMEOUT, retained_at.recv())
                    .await
                    .unwrap()
                    .unwrap()
            })
        }

        fn stored_limit(&self) -> u32 {
            self.rt
                .block_on(chat_history_per_viewer_limit(self.backend.as_ref()))
                .unwrap()
        }

        fn shown_input(&self, cx: &mut TestAppContext) -> String {
            self.view.read_with(cx, |view, cx| {
                view.per_viewer_input.read(cx).content().to_owned()
            })
        }
    }

    #[gpui::test]
    fn choosing_a_preset_persists_it_and_retains_the_next_written_batch_at_it(
        cx: &mut TestAppContext,
    ) {
        let mut rig = rig(cx);

        rig.view
            .update(cx, |view, cx| view.select_per_viewer_preset(25, cx));
        pump(&rig.rt);

        assert_eq!((rig.stored_limit(), rig.next_retention_limit()), (25, 25));
    }

    #[gpui::test]
    fn committing_a_custom_value_applies_a_valid_one_and_restores_an_invalid_one(
        cx: &mut TestAppContext,
    ) {
        for (typed, shown, written) in [
            (" 250 ", "250", Some("250")),
            ("10001", "50", None),
            ("abc", "50", None),
        ] {
            let mut rig = rig(cx);
            rig.view.update(cx, |view, cx| {
                view.select_custom_per_viewer(cx);
                view.per_viewer_input
                    .update(cx, |input, cx| input.set_content(typed.to_owned(), cx));
                view.commit_custom_per_viewer(cx);
            });
            pump(&rig.rt);

            assert_eq!(
                (
                    rig.shown_input(cx),
                    rig.writes.try_recv().ok().map(|(_, value)| value)
                ),
                (shown.to_owned(), written.map(str::to_owned)),
                "typed {typed:?}"
            );
        }
    }

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

    #[test]
    fn parse_retention_days_accepts_exactly_the_range_storage_honours() {
        for (raw, expected) in [
            ("0", None),
            ("1", Some(1)),
            ("365", Some(365)),
            ("366", None),
            (" 30 ", Some(30)),
            ("", None),
            ("-1", None),
        ] {
            assert_eq!(
                parse_retention_days(raw),
                expected,
                "parse_retention_days({raw:?})"
            );
        }
    }
}
