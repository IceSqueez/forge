use std::sync::Arc;

use forge_events::EventSource;
use forge_runtime::EventBus;
use iced::Subscription;

use crate::app::App;
use crate::builtin_detail::health_subscription;
use crate::message::{LiveChatMsg, Message};
use crate::screen::Screen;
use crate::server_screen::ServerScreenMsg;

fn event_source_label(source: EventSource) -> &'static str {
    match source {
        EventSource::Twitch => "twitch",
        EventSource::YouTube => "youtube",
        EventSource::Kick => "kick",
        EventSource::Core => "core",
        EventSource::Rhai => "rhai",
        EventSource::Http => "http",
        EventSource::Obs => "obs",
        EventSource::VTube => "vtube",
        EventSource::Discord => "discord",
        EventSource::Midi => "midi",
        EventSource::Hotkey => "hotkey",
        EventSource::Timer => "timer",
        EventSource::Server => "server",
        EventSource::Audio => "audio",
    }
}

fn format_short_duration(d: time::Duration) -> String {
    let secs = d.whole_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

async fn scan_overlay_root(root: &std::path::Path) -> crate::server_screen::OverlayListingSnapshot {
    use crate::server_screen::{
        OverlayListingSnapshot, OwnedFileMime, OwnedOverlayEntry, OwnedOverlayKind,
    };

    let root_str = root.to_string_lossy().into_owned();
    let mut read_dir = match tokio::fs::read_dir(root).await {
        Ok(rd) => rd,
        Err(_) => {
            return OverlayListingSnapshot {
                root: root_str,
                entries: Vec::new(),
            };
        }
    };

    let mut entries: Vec<OwnedOverlayEntry> = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            let mut count: u32 = 0;
            if let Ok(mut child) = tokio::fs::read_dir(entry.path()).await {
                while let Ok(Some(_)) = child.next_entry().await {
                    count = count.saturating_add(1);
                }
            }
            entries.push(OwnedOverlayEntry {
                name,
                kind: OwnedOverlayKind::Dir,
                size_bytes: 0,
                child_count: count,
            });
        } else {
            let mime = OwnedFileMime::from_path(&entry.path());
            entries.push(OwnedOverlayEntry {
                name,
                kind: OwnedOverlayKind::File { mime },
                size_bytes: meta.len(),
                child_count: 0,
            });
        }
    }

    entries.sort_by(|a, b| {
        let dir_a = matches!(a.kind, OwnedOverlayKind::Dir);
        let dir_b = matches!(b.kind, OwnedOverlayKind::Dir);
        match (dir_a, dir_b) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase()),
        }
    });

    OverlayListingSnapshot {
        root: root_str,
        entries,
    }
}

fn soundboard_hotkey_filter(event: iced::keyboard::Event) -> Option<Message> {
    use iced::keyboard::Event::KeyPressed;
    use iced::keyboard::Key::Character;
    use iced::keyboard::key::Named;

    let KeyPressed { key, modifiers, .. } = event else {
        return None;
    };

    let label = match &key {
        Character(c) => {
            if modifiers.control() {
                format!("Ctrl+{}", c.to_uppercase())
            } else if modifiers.shift() {
                format!("Shift+{}", c.to_uppercase())
            } else {
                return None;
            }
        }
        iced::keyboard::Key::Named(Named::F1) => "F1".to_string(),
        iced::keyboard::Key::Named(Named::F2) => "F2".to_string(),
        iced::keyboard::Key::Named(Named::F3) => "F3".to_string(),
        iced::keyboard::Key::Named(Named::F4) => "F4".to_string(),
        iced::keyboard::Key::Named(Named::F5) => "F5".to_string(),
        iced::keyboard::Key::Named(Named::F6) => "F6".to_string(),
        iced::keyboard::Key::Named(Named::F7) => "F7".to_string(),
        iced::keyboard::Key::Named(Named::F8) => "F8".to_string(),
        iced::keyboard::Key::Named(Named::F9) => "F9".to_string(),
        iced::keyboard::Key::Named(Named::F10) => "F10".to_string(),
        iced::keyboard::Key::Named(Named::F11) => "F11".to_string(),
        iced::keyboard::Key::Named(Named::F12) => "F12".to_string(),
        _ => return None,
    };

    Some(Message::Soundboard(
        crate::message::SoundboardMsg::HotkeyPressed(label),
    ))
}

pub fn subscription(app: &App) -> Subscription<Message> {
    use iced::advanced::subscription::{EventStream, Hasher, Recipe, from_recipe};
    use iced::futures::StreamExt as _;

    struct BusRecipe(Arc<EventBus>);

    impl Recipe for BusRecipe {
        type Output = Message;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash as _;
            (Arc::as_ptr(&self.0) as usize).hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: EventStream,
        ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
            let bus = self.0;
            iced::stream::channel(
                64,
                |mut tx: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let mut stream = bus.subscribe();
                    loop {
                        if let Ok(event) = stream.recv().await {
                            let _ = tx.try_send(Message::EventArrived(Arc::new(event)));
                        }
                    }
                },
            )
            .boxed()
        }
    }

    let bus = from_recipe(BusRecipe(app.rt.bus.clone()));

    struct ChatStreamRecipe(Arc<EventBus>);

    impl Recipe for ChatStreamRecipe {
        type Output = Message;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash as _;
            "forge:chat-stream".hash(state);
            (Arc::as_ptr(&self.0) as usize).hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: EventStream,
        ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
            use iced::futures::StreamExt as _;
            let stream = forge_runtime::chat_stream(self.0);
            stream
                .map(|row| Message::LiveChat(LiveChatMsg::RowReceived(row)))
                .boxed()
        }
    }

    let chat_stream = from_recipe(ChatStreamRecipe(app.rt.bus.clone()));

    struct ServerMetricsRecipe(Arc<crate::server_subsystem::ServerSubsystem>);

    impl Recipe for ServerMetricsRecipe {
        type Output = Message;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash as _;
            "server-metrics-tick".hash(state);
            (Arc::as_ptr(&self.0) as usize).hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: EventStream,
        ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
            let subsystem = self.0;
            iced::stream::channel(
                4,
                |mut tx: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
                    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    let mut tick_count: u32 = 0;
                    loop {
                        ticker.tick().await;
                        tick_count = tick_count.wrapping_add(1);
                        let Some(info) = subsystem.server_info().await else {
                            continue;
                        };
                        let bus_adapter = subsystem.bus_adapter().await;
                        let clients_guard = info.connected_clients.read().await;
                        let mut rows: Vec<crate::server_screen::OwnedClientRow> = Vec::new();
                        let mut events_per_second_total: f32 = 0.0;
                        for (client_id, client) in clients_guard.iter() {
                            let eps = client.events_per_second();
                            events_per_second_total += eps;
                            let subscriptions = match bus_adapter.as_ref() {
                                Some(adapter) => {
                                    let filters = adapter.current_subscriptions(*client_id).await;
                                    filters
                                        .into_iter()
                                        .map(|f| {
                                            let label = match (&f.source, &f.kind) {
                                                (Some(s), Some(k)) => {
                                                    format!("{}.{}", event_source_label(*s), k)
                                                }
                                                (Some(s), None) => {
                                                    format!("{}.*", event_source_label(*s))
                                                }
                                                (None, Some(k)) => k.clone(),
                                                (None, None) => "*".to_owned(),
                                            };
                                            let source =
                                                f.source.unwrap_or(forge_events::EventSource::Core);
                                            crate::server_screen::OwnedSubscriptionChip {
                                                label,
                                                source,
                                            }
                                        })
                                        .collect()
                                }
                                None => Vec::new(),
                            };
                            let liveness = if eps > 0.0 {
                                crate::server_screen::ClientLiveness::Active
                            } else {
                                crate::server_screen::ClientLiveness::Idle
                            };
                            rows.push(crate::server_screen::OwnedClientRow {
                                identification: (**client.identification.load()).clone(),
                                client_type_label: client.client_type.load().type_str().to_owned(),
                                liveness,
                                subscriptions,
                                events_per_second: eps,
                                uptime_short: format_short_duration(client.uptime()),
                            });
                        }
                        drop(clients_guard);
                        let kbps = info.bandwidth.current_bps() as f32 / 1000.0;
                        let peak_kbps = info.bandwidth.peak() as f32 / 1000.0;
                        let total_bytes = info.bandwidth.total();
                        let stats = crate::server_screen::ServerStats {
                            events_per_second: events_per_second_total,
                            events_per_second_avg: events_per_second_total,
                            http_requests: info.http_requests(),
                            bandwidth_kbps: kbps,
                            bandwidth_peak_kbps: peak_kbps,
                            total_bytes_sent: total_bytes,
                            total_events_out: info.events_out(),
                        };
                        let snapshot = crate::server_screen::ServerInfoSnapshot {
                            uptime_seconds: info.uptime_seconds(),
                            connected_clients: rows,
                            stats,
                        };
                        let _ = tx.try_send(Message::Server(ServerScreenMsg::ServerInfoArrived(
                            snapshot,
                        )));
                        let _ = tx.try_send(Message::Server(ServerScreenMsg::BandwidthTick(kbps)));

                        let should_scan = tick_count == 1 || tick_count.is_multiple_of(5);
                        if should_scan && let Some(root) = subsystem.overlay_root().await {
                            let listing = scan_overlay_root(root.as_ref()).await;
                            let _ = tx.try_send(Message::Server(
                                ServerScreenMsg::OverlayListingArrived(listing),
                            ));
                        }
                    }
                },
            )
            .boxed()
        }
    }

    let server_tick = if matches!(app.screen, Screen::Server) {
        from_recipe(ServerMetricsRecipe(Arc::clone(&app.rt.server_subsystem)))
    } else {
        Subscription::none()
    };

    let soundboard_keys = if matches!(app.screen, Screen::Soundboard) {
        iced::keyboard::listen().filter_map(soundboard_hotkey_filter)
    } else {
        Subscription::none()
    };

    struct SpeakEventRecipe(Arc<forge_speak_queue::SpeakQueueHandle>);

    impl Recipe for SpeakEventRecipe {
        type Output = Message;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash as _;
            "speak-event-stream".hash(state);
            (Arc::as_ptr(&self.0) as usize).hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: EventStream,
        ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
            let mut rx = self.0.subscribe();
            iced::stream::channel(
                64,
                |mut tx: iced::futures::channel::mpsc::Sender<Message>| async move {
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                let _ =
                                    tx.try_send(Message::Tts(crate::message::TtsMsg::Dashboard(
                                        crate::message::TtsDashMsg::SpeakEventReceived(event),
                                    )));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                },
            )
            .boxed()
        }
    }

    let tts_events = if matches!(app.screen, Screen::Tts(_))
        && let Some(handle) = app.rt.speak_queue.as_ref()
    {
        from_recipe(SpeakEventRecipe(Arc::clone(handle)))
    } else {
        Subscription::none()
    };

    let toast_tick = iced::time::every(std::time::Duration::from_millis(200))
        .map(|instant| Message::Toast(crate::message::ToastMsg::Tick(instant)));

    let outside_click = iced::event::listen_with(|event, status, _window| match (event, status) {
        (
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(_)),
            iced::event::Status::Ignored,
        ) => Some(Message::OutsideClick),
        _ => None,
    });

    if let Some(state) = app.ui.builtin_detail.as_ref() {
        Subscription::batch([
            bus,
            chat_stream,
            health_subscription(state),
            server_tick,
            soundboard_keys,
            tts_events,
            toast_tick,
            outside_click,
        ])
    } else {
        Subscription::batch([
            bus,
            chat_stream,
            server_tick,
            soundboard_keys,
            tts_events,
            toast_tick,
            outside_click,
        ])
    }
}
