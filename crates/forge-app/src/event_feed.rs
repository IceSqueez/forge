use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use forge_events::{Event, EventSource};
use forge_types::{ActionId, EventId};
use forge_widgets::{
    EventInspectorParams, EventRowData, FontRole, ForgePalette, Radius, Spacing, category_chip,
    event_inspector, event_row_observability, font, radius, sp, spf,
    tokens::{FONT_SM, FONT_XS},
};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Background, Border, Color, Element, Length, Padding};

use crate::message::ActionsMsg;
use crate::runtime_view::RuntimeView;
use crate::{Message, Screen};

const RING_CAP: usize = 10_000;
const RATE_WINDOW_SECS: u64 = 10;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum EventFilter {
    #[default]
    All,
    Chat,
    Subs,
    Bits,
    Timers,
    Obs,
    Errors,
}

#[derive(Debug, Clone)]
pub enum EventFeedMsg {
    EventArrived(Event),
    EventSelected(EventId),
    FilterChanged(EventFilter),
    PauseToggled,
    Cleared,
    ExportRequested,
    ExportResult(Result<std::path::PathBuf, String>),
    AutoScrollToggled,
    ReplayRequested(EventId),
    ReplayResult(Result<(), String>),
    CausationChipClicked(ActionId),
}

struct EvRateTracker {
    timestamps: VecDeque<Instant>,
}

impl EvRateTracker {
    fn new() -> Self {
        Self {
            timestamps: VecDeque::new(),
        }
    }

    fn push(&mut self) {
        let now = Instant::now();
        self.timestamps.push_back(now);
        let window = std::time::Duration::from_secs(RATE_WINDOW_SECS);
        while self
            .timestamps
            .front()
            .is_some_and(|t| now.duration_since(*t) > window)
        {
            self.timestamps.pop_front();
        }
    }

    fn rate(&self) -> f32 {
        self.timestamps.len() as f32 / RATE_WINDOW_SECS as f32
    }

    fn clear(&mut self) {
        self.timestamps.clear();
    }
}

pub struct EventFeedState {
    pub events: VecDeque<Event>,
    pub selected: Option<EventId>,
    pub paused: bool,
    pub auto_scroll: bool,
    pub active_filter: EventFilter,
    ev_rate: EvRateTracker,
    pub replay_loading: bool,
    selected_ts_str: String,
    selected_id_str: String,
    selected_action_name: Option<String>,
    selected_action_id_str: Option<String>,
    selected_action_id: Option<ActionId>,
}

impl EventFeedState {
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
            selected: None,
            paused: false,
            auto_scroll: true,
            active_filter: EventFilter::All,
            ev_rate: EvRateTracker::new(),
            replay_loading: false,
            selected_ts_str: String::new(),
            selected_id_str: String::new(),
            selected_action_name: None,
            selected_action_id_str: None,
            selected_action_id: None,
        }
    }

    pub fn push_event(&mut self, event: Event) {
        if self.events.len() >= RING_CAP {
            self.events.pop_front();
        }
        self.ev_rate.push();
        self.events.push_back(event);
    }

    pub fn ev_rate(&self) -> f32 {
        self.ev_rate.rate()
    }

    fn selected_event(&self) -> Option<&Event> {
        let id = self.selected?;
        self.events.iter().rev().find(|e| e.id == id)
    }

    fn update_selection(&mut self, id: EventId) {
        self.selected = Some(id);
        let found = self.events.iter().rev().find(|e| e.id == id).cloned();
        if let Some(ev) = found {
            let ts = ev.timestamp;
            self.selected_ts_str = format!(
                "{:02}:{:02}:{:02}.{:03}",
                ts.hour(),
                ts.minute(),
                ts.second(),
                ts.millisecond()
            );
            let id_s = id.to_string();
            self.selected_id_str = format!("ev_{}", &id_s[..id_s.len().min(4)]);
            self.selected_action_name = ev.payload["action_name"].as_str().map(str::to_owned);
            let aid_opt = ev.payload["action_id"].as_str();
            self.selected_action_id_str = aid_opt.map(|s| format!("#{}", &s[..s.len().min(6)]));
            self.selected_action_id = aid_opt.and_then(|s| {
                serde_json::from_value::<ActionId>(serde_json::Value::String(s.to_owned())).ok()
            });
        } else {
            self.selected_ts_str.clear();
            self.selected_id_str.clear();
            self.selected_action_name = None;
            self.selected_action_id_str = None;
            self.selected_action_id = None;
        }
    }
}

impl Default for EventFeedState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn on_event(state: &mut EventFeedState, event: &Arc<Event>) -> iced::Task<Message> {
    if !state.paused {
        state.push_event(Arc::unwrap_or_clone(Arc::clone(event)));
    }
    iced::Task::none()
}

pub fn matches_filter(event: &Event, filter: EventFilter) -> bool {
    match filter {
        EventFilter::All => true,
        EventFilter::Chat => {
            let k = event.kind.as_str();
            k.contains("chat") || k.contains("command")
        }
        EventFilter::Subs => {
            let k = event.kind.as_str();
            k.contains("sub") || k.contains("subscription") || k.contains("follow")
        }
        EventFilter::Bits => {
            let k = event.kind.as_str();
            k.contains("cheer") || k.contains("bits") || k.contains("raid")
        }
        EventFilter::Timers => {
            matches!(event.source, EventSource::Timer) || event.kind.contains("timer")
        }
        EventFilter::Obs => {
            matches!(event.source, EventSource::Obs)
                || event.kind.contains("scene")
                || event.kind.contains("obs")
        }
        EventFilter::Errors => event.kind.contains("error") || event.kind.contains("fail"),
    }
}

fn format_timestamp(ts: &time::OffsetDateTime) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ts.hour(),
        ts.minute(),
        ts.second(),
        ts.millisecond()
    )
}

pub(crate) fn format_summary(event: &Event) -> String {
    let p = &event.payload;
    match event.kind.as_str() {
        "chat.message" => {
            use forge_platform_twitch::{TwitchChatEvent, parse_chat_event};
            let (user, msg) = parse_chat_event(event)
                .and_then(|e| match e {
                    TwitchChatEvent::Message { username, text, .. } => Some((username, text)),
                    _ => None,
                })
                .unwrap_or_else(|| ("?".to_owned(), String::new()));
            format!("{user}: {msg}")
        }
        "command.matched" => {
            let cmd = p["command"].as_str().unwrap_or("?");
            let user = p["user"]["login"]
                .as_str()
                .or_else(|| p["user"].as_str())
                .unwrap_or("?");
            format!("{cmd} by {user}")
        }
        "action.start" => {
            let name = p["action_name"].as_str().unwrap_or("?");
            let queue = p["queue"].as_str().unwrap_or("Default");
            format!("{name} · queue={queue}")
        }
        "action.done" => {
            let name = p["action_name"].as_str().unwrap_or("?");
            let status = p["status"].as_str().unwrap_or("ok");
            format!("{name} · status={status}")
        }
        "subaction.run" => {
            let idx = p["index"].as_u64().unwrap_or(0);
            let total = p["total"].as_u64().unwrap_or(0);
            let kind = p["kind"].as_str().unwrap_or("?");
            format!("[{idx}/{total}] {kind}")
        }
        "script.exec" => p["script_name"].as_str().unwrap_or("?").to_owned(),
        "timer.tick" => p["name"].as_str().unwrap_or("?").to_owned(),
        "scene.changed" => {
            let from = p["from"].as_str().unwrap_or("?");
            let to = p["to"].as_str().unwrap_or("?");
            format!("\"{from}\" \u{2192} \"{to}\"")
        }
        "chat.send" => p["message"]
            .as_str()
            .or_else(|| p.as_str())
            .unwrap_or("?")
            .to_owned(),
        "request.fail" => {
            let url = p["url"].as_str().unwrap_or("?");
            let status = p["status"]
                .as_u64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "error".to_owned());
            format!("{url} \u{2192} {status}")
        }
        "global.set" => {
            let name = p["name"].as_str().unwrap_or("?");
            let val = p["value"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| p["value"].to_string());
            format!("{name} = {val}")
        }
        "global.incr" => p["name"].as_str().unwrap_or("?").to_owned(),
        _ => event.kind.clone(),
    }
}

fn format_result_tag(event: &Event) -> Option<String> {
    let p = &event.payload;
    match event.kind.as_str() {
        "action.done" => {
            let status = p["status"].as_str().unwrap_or("ok");
            let dur = p["duration_ms"].as_u64();
            match (status, dur) {
                ("ok", Some(d)) => Some(format!("{d}ms total")),
                ("ok", None) => Some("ok".to_owned()),
                (s, _) => Some(s.to_owned()),
            }
        }
        "command.matched" => Some("\u{2192} trigger fired".to_owned()),
        "chat.send" => Some("sent".to_owned()),
        "request.fail" => {
            let retry = p["retry_in_secs"].as_u64();
            retry.map(|s| format!("retry in {s}s"))
        }
        "subaction.run" => p["duration_ms"].as_u64().map(|d| {
            if d == 0 {
                "<1ms".to_owned()
            } else {
                format!("{d}ms")
            }
        }),
        "scene.changed" => {
            let count = p["action_count"].as_u64();
            count.map(|c| format!("\u{2192} {c} actions"))
        }
        "chat.message" => {
            let matched = p["matched"].as_bool().unwrap_or(false);
            Some(
                if matched {
                    "\u{2192} 1 action"
                } else {
                    "no match"
                }
                .to_owned(),
            )
        }
        _ => None,
    }
}

fn is_error_event(event: &Event) -> bool {
    event.kind.contains("error") || event.kind.contains("fail")
}

pub fn update(
    state: &mut EventFeedState,
    rt: &RuntimeView,
    msg: EventFeedMsg,
) -> iced::Task<Message> {
    match msg {
        EventFeedMsg::EventArrived(event) => {
            if !state.paused {
                state.push_event(event);
            }
            iced::Task::none()
        }
        EventFeedMsg::EventSelected(id) => {
            state.update_selection(id);
            iced::Task::none()
        }
        EventFeedMsg::FilterChanged(f) => {
            state.active_filter = f;
            iced::Task::none()
        }
        EventFeedMsg::PauseToggled => {
            state.paused = !state.paused;
            iced::Task::none()
        }
        EventFeedMsg::Cleared => {
            state.events.clear();
            state.selected = None;
            state.selected_ts_str.clear();
            state.selected_id_str.clear();
            state.selected_action_name = None;
            state.selected_action_id_str = None;
            state.selected_action_id = None;
            state.ev_rate.clear();
            iced::Task::none()
        }
        EventFeedMsg::ExportRequested => {
            let events: Vec<Event> = state.events.iter().cloned().collect();
            iced::Task::perform(
                async move {
                    let Some(handle) = rfd::AsyncFileDialog::new()
                        .add_filter("JSON", &["json"])
                        .set_file_name("forge-events.json")
                        .save_file()
                        .await
                    else {
                        return Err("export cancelled".to_string());
                    };
                    let path = handle.path().to_path_buf();
                    let json = serde_json::to_string_pretty(&events).map_err(|e| e.to_string())?;
                    tokio::fs::write(&path, json)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(path)
                },
                |r| Message::EventFeed(EventFeedMsg::ExportResult(r)),
            )
        }
        EventFeedMsg::ExportResult(Ok(path)) => {
            tracing::info!(path = %path.display(), "event feed exported");
            iced::Task::none()
        }
        EventFeedMsg::ExportResult(Err(e)) => {
            tracing::warn!(error = %e, "event feed export failed");
            iced::Task::none()
        }
        EventFeedMsg::AutoScrollToggled => {
            state.auto_scroll = !state.auto_scroll;
            iced::Task::none()
        }
        EventFeedMsg::ReplayRequested(event_id) => {
            state.replay_loading = true;
            let bus = Arc::clone(&rt.bus);
            iced::Task::perform(
                async move {
                    bus.replay_and_publish(event_id)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::EventFeed(EventFeedMsg::ReplayResult(r)),
            )
        }
        EventFeedMsg::ReplayResult(Ok(())) => {
            state.replay_loading = false;
            iced::Task::none()
        }
        EventFeedMsg::ReplayResult(Err(e)) => {
            state.replay_loading = false;
            tracing::warn!(error = %e, "event replay failed");
            iced::Task::none()
        }
        EventFeedMsg::CausationChipClicked(action_id) => iced::Task::batch([
            iced::Task::done(Message::Navigate(Screen::Actions)),
            iced::Task::done(Message::Actions(ActionsMsg::ActionSelected(action_id))),
        ]),
    }
}

fn toolbar_action_btn<'a>(
    label: &'a str,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let text_color = palette.text_secondary;
    let hover_bg = Color {
        a: 0.05,
        ..palette.border_regular
    };

    button(text(label).size(FONT_XS).color(text_color))
        .on_press(on_press)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
        .style(move |_theme: &iced::Theme, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(Background::Color(hover_bg))
                }
                _ => None,
            },
            text_color,
            border: Border {
                radius: radius(Radius::Md).into(),
                ..Border::default()
            },
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

pub fn event_feed_view<'a>(
    state: &'a EventFeedState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let mono = font(FontRole::Monospace);
    let filter = state.active_filter;

    let _ = mono;

    let mut all_n = 0u32;
    let mut chat_n = 0u32;
    let mut subs_n = 0u32;
    let mut bits_n = 0u32;
    let mut timers_n = 0u32;
    let mut obs_n = 0u32;
    let mut errors_n = 0u32;

    for ev in &state.events {
        all_n += 1;
        if matches_filter(ev, EventFilter::Chat) {
            chat_n += 1;
        }
        if matches_filter(ev, EventFilter::Subs) {
            subs_n += 1;
        }
        if matches_filter(ev, EventFilter::Bits) {
            bits_n += 1;
        }
        if matches_filter(ev, EventFilter::Timers) {
            timers_n += 1;
        }
        if matches_filter(ev, EventFilter::Obs) {
            obs_n += 1;
        }
        if matches_filter(ev, EventFilter::Errors) {
            errors_n += 1;
        }
    }

    let all_label = format!("All {all_n}");
    let chat_label = format!("Chat {chat_n}");
    let subs_label = format!("Subs {subs_n}");
    let bits_label = format!("Bits {bits_n}");
    let timers_label = format!("Timers {timers_n}");
    let obs_label = format!("OBS {obs_n}");
    let errors_label = format!("Errors {errors_n}");

    let chips = row![
        category_chip(
            palette,
            &all_label,
            palette.brand,
            filter == EventFilter::All,
            Message::EventFeed(EventFeedMsg::FilterChanged(EventFilter::All)),
        ),
        category_chip(
            palette,
            &chat_label,
            palette.info,
            filter == EventFilter::Chat,
            Message::EventFeed(EventFeedMsg::FilterChanged(EventFilter::Chat)),
        ),
        category_chip(
            palette,
            &subs_label,
            palette.success,
            filter == EventFilter::Subs,
            Message::EventFeed(EventFeedMsg::FilterChanged(EventFilter::Subs)),
        ),
        category_chip(
            palette,
            &bits_label,
            palette.bits,
            filter == EventFilter::Bits,
            Message::EventFeed(EventFeedMsg::FilterChanged(EventFilter::Bits)),
        ),
        category_chip(
            palette,
            &timers_label,
            palette.warning,
            filter == EventFilter::Timers,
            Message::EventFeed(EventFeedMsg::FilterChanged(EventFilter::Timers)),
        ),
        category_chip(
            palette,
            &obs_label,
            palette.success,
            filter == EventFilter::Obs,
            Message::EventFeed(EventFeedMsg::FilterChanged(EventFilter::Obs)),
        ),
        category_chip(
            palette,
            &errors_label,
            palette.random,
            filter == EventFilter::Errors,
            Message::EventFeed(EventFeedMsg::FilterChanged(EventFilter::Errors)),
        ),
    ]
    .spacing(spf(Spacing::Xxs));

    let sep = container(iced::widget::Space::new().width(0.5).height(14.0)).style(
        move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.border_regular)),
            ..container::Style::default()
        },
    );

    let pause_label = if state.paused { "Resume" } else { "Pause" };
    let pause_btn = toolbar_action_btn(
        pause_label,
        Message::EventFeed(EventFeedMsg::PauseToggled),
        palette,
    );
    let clear_btn = toolbar_action_btn("Clear", Message::EventFeed(EventFeedMsg::Cleared), palette);
    let export_btn = toolbar_action_btn(
        "Export",
        Message::EventFeed(EventFeedMsg::ExportRequested),
        palette,
    );

    let action_row = row![pause_btn, sep, clear_btn, export_btn]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center);

    let divider = crate::page_chrome::header_divider(palette);
    let right_side = row![chips, divider, action_row]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center);

    let filtered: Vec<&Event> = state
        .events
        .iter()
        .filter(|e| matches_filter(e, filter))
        .collect();

    let selected_id = state.selected;

    let row_elements: Vec<Element<'_, Message>> = filtered
        .iter()
        .map(|ev| {
            let is_err = is_error_event(ev);
            let is_sel = selected_id == Some(ev.id);
            let ev_id = ev.id;

            let row_data = EventRowData {
                timestamp: format_timestamp(&ev.timestamp),
                source: ev.source,
                event_type: ev.kind.clone(),
                summary: format_summary(ev),
                result_tag: format_result_tag(ev),
                is_error: is_err,
            };

            event_row_observability(
                &row_data,
                is_sel,
                Message::EventFeed(EventFeedMsg::EventSelected(ev_id)),
                palette,
            )
        })
        .collect();

    let empty_list = row_elements.is_empty();

    let list_content = if empty_list {
        column![
            text(if matches!(filter, EventFilter::All) {
                "No events yet \u{2014} system activity appears here in real time."
            } else {
                "No events match the active filter."
            })
            .size(FONT_SM)
            .color(palette.text_faint)
        ]
        .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
    } else {
        column(row_elements)
    };

    let event_list_pane = container(scrollable(list_content).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.base)),
            ..container::Style::default()
        });

    let inspector_pane: Element<'_, Message> = if let Some(ev) = state.selected_event() {
        let caused_action = state
            .selected_action_name
            .as_deref()
            .zip(state.selected_action_id_str.as_deref())
            .zip(state.selected_action_id)
            .map(|((name, id_disp), aid)| {
                (
                    name,
                    id_disp,
                    Message::EventFeed(EventFeedMsg::CausationChipClicked(aid)),
                )
            });

        let params = EventInspectorParams {
            source: ev.source,
            event_type: &ev.kind,
            timestamp: &state.selected_ts_str,
            event_id: &state.selected_id_str,
            payload: &ev.payload,
            caused_action,
            on_replay: Message::EventFeed(EventFeedMsg::ReplayRequested(ev.id)),
        };

        container(
            scrollable(column![event_inspector(params, palette)].padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: spf(Spacing::Sm),
                left: 0.0,
            }))
            .height(Length::Fill),
        )
        .width(Length::Fixed(280.0))
        .height(Length::Fill)
        .padding(sp(Spacing::Sm))
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.shell)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    } else {
        let inspector_header = text("Event inspector")
            .size(FONT_SM)
            .color(palette.text_primary);

        let placeholder = text("Select an event to inspect its payload.")
            .size(FONT_XS)
            .color(palette.text_faint)
            .font(mono);

        container(column![inspector_header, placeholder].spacing(spf(Spacing::Xs)))
            .width(Length::Fixed(280.0))
            .height(Length::Fill)
            .padding(sp(Spacing::Sm))
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(palette.shell)),
                border: Border {
                    color: palette.border_regular,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    };

    let body_row = row![event_list_pane, inspector_pane].height(Length::Fill);

    let buf_count = state.events.len();
    let rate = state.ev_rate();
    let auto_scroll_dot = container(iced::widget::Space::new())
        .width(6.0)
        .height(6.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(if state.auto_scroll {
                palette.success
            } else {
                palette.disabled
            })),
            border: Border {
                radius: 3.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let auto_scroll_label = if state.auto_scroll {
        "Auto-scroll on"
    } else {
        "Auto-scroll off"
    };

    let footer_right = row![
        text(format!("Buffer: {buf_count:>5} / 10,000"))
            .size(FONT_XS)
            .color(palette.text_faint)
            .font(mono),
        text(format!("{:.1} ev/s", rate))
            .size(FONT_XS)
            .color(palette.text_faint)
            .font(mono),
        container(
            row![
                auto_scroll_dot,
                text(auto_scroll_label)
                    .size(FONT_XS)
                    .color(palette.text_faint)
                    .font(mono),
            ]
            .spacing(spf(Spacing::Xxs))
            .align_y(iced::Alignment::Center),
        ),
    ]
    .spacing(spf(Spacing::Sm))
    .align_y(iced::Alignment::Center);

    let footer = container(
        row![
            text("Streaming \u{b7} WebSocket :8081")
                .size(FONT_XS)
                .color(palette.text_faint)
                .font(mono),
            iced::widget::Space::new().width(Length::Fill),
            footer_right,
        ]
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(palette.shell)),
        border: Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let page_header = crate::page_chrome::page_header_with_actions(
        &[("Automation", false), ("Event Feed", true)],
        Some(right_side.into()),
        palette,
    );

    column![page_header, body_row, footer]
        .height(Length::Fill)
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry};
    use forge_storage::CredentialsRepo;
    use forge_storage_sqlite::SqliteBackend;

    use crate::runtime_view::RuntimeView;
    use crate::server_subsystem::ServerSubsystem;

    fn test_rt() -> RuntimeView {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let backend = Arc::new(
            rt.block_on(SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32]))
                .expect("in-memory SQLite"),
        );
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
        let backend: Arc<dyn forge_storage::DataProvider> = backend;
        RuntimeView {
            actions: Arc::new(forge_runtime::actions::ActionsService::new(
                backend.action_repo(),
                backend.queue_repo(),
                backend.history_repo(),
                backend.trigger_instance_repo(),
                backend.soundboard_clips_repo(),
            )),
            backend,
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            script_registry: Arc::new(ScriptRegistry::new()),
            server_subsystem,
            action_engine: None,
            scheduler: None,
            obs_client: None,
            vtube_client: None,
            vtube_sink: forge_vtube::SwitchableVTubeSink::new(),
            discord_client: None,
            midi_client: None,
            hotkey_client: None,
            speak_queue: None,
            sound_player: None,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            twitch_flow: None,
            youtube_flow: None,
            kick_flow: None,
            tts_engine_ids: Vec::new(),
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    fn core_event(kind: &str) -> Event {
        Event::new(EventSource::Core, kind, serde_json::Value::Null)
    }

    #[test]
    fn event_arrived_increments_events_and_updates_rate() {
        let mut state = EventFeedState::new();
        let rt = test_rt();
        let event = core_event("action.start");

        let _task = update(&mut state, &rt, EventFeedMsg::EventArrived(event));

        assert_eq!(state.events.len(), 1);
        assert!(state.ev_rate() > 0.0);
    }

    #[test]
    fn toggle_pause_flips_paused_flag() {
        let mut state = EventFeedState::new();
        let rt = test_rt();

        assert!(!state.paused);
        let _task = update(&mut state, &rt, EventFeedMsg::PauseToggled);
        assert!(state.paused);
    }

    #[test]
    fn event_arrived_while_paused_does_not_add_to_ring() {
        let mut state = EventFeedState::new();
        let rt = test_rt();

        state.paused = true;
        let event = core_event("action.start");
        let _task = update(&mut state, &rt, EventFeedMsg::EventArrived(event));

        assert_eq!(state.events.len(), 0);
    }

    #[test]
    fn filter_changed_updates_active_filter() {
        let mut state = EventFeedState::new();
        let rt = test_rt();

        assert_eq!(state.active_filter, EventFilter::All);
        let _task = update(
            &mut state,
            &rt,
            EventFeedMsg::FilterChanged(EventFilter::Errors),
        );
        assert_eq!(state.active_filter, EventFilter::Errors);
    }

    #[test]
    fn matches_filter_errors_on_fail_events() {
        let fail_event = Event::new(EventSource::Http, "request.fail", serde_json::Value::Null);
        assert!(matches_filter(&fail_event, EventFilter::Errors));

        let chat_event = Event::new(EventSource::Twitch, "chat.message", serde_json::Value::Null);
        assert!(!matches_filter(&chat_event, EventFilter::Errors));
    }

    #[test]
    fn matches_filter_chat_on_command_events() {
        let cmd_event = Event::new(
            EventSource::Core,
            "command.matched",
            serde_json::Value::Null,
        );
        assert!(matches_filter(&cmd_event, EventFilter::Chat));
    }

    #[test]
    fn matches_filter_obs_on_scene_events() {
        let scene_event = Event::new(EventSource::Obs, "scene.changed", serde_json::Value::Null);
        assert!(matches_filter(&scene_event, EventFilter::Obs));
    }

    #[test]
    fn cleared_empties_ring_and_resets_selection() {
        let mut state = EventFeedState::new();
        let rt = test_rt();

        state.push_event(core_event("action.start"));
        state.push_event(core_event("action.done"));
        state.selected = Some(state.events[0].id);

        let _task = update(&mut state, &rt, EventFeedMsg::Cleared);

        assert!(state.events.is_empty());
        assert!(state.selected.is_none());
        assert_eq!(state.ev_rate(), 0.0);
    }

    #[test]
    fn ring_evicts_oldest_when_at_cap() {
        let mut state = EventFeedState::new();
        for _ in 0..RING_CAP {
            state.push_event(core_event("filler"));
        }
        assert_eq!(state.events.len(), RING_CAP);

        state.push_event(core_event("overflow"));
        assert_eq!(state.events.len(), RING_CAP);
        assert_eq!(state.events.back().unwrap().kind, "overflow");
    }
}
