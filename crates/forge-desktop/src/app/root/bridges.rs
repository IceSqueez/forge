use super::*;

pub(super) fn mirror_connections_into_home(
    platforms: &Entity<PlatformConnectivity>,
    home_stats: Entity<HomeStats>,
    cx: &mut App,
) {
    cx.observe(platforms, move |platforms, cx| {
        let connections = platforms.read(cx).connections().to_vec();
        home_stats.update(cx, |stats, cx| {
            if stats.set_connections(&connections) {
                cx.notify();
            }
        });
    })
    .detach();
}

pub(super) fn start_bridge(
    cx: &mut AsyncApp,
    home_stats: Entity<HomeStats>,
    event_log: Entity<EventLog>,
    platforms: Entity<PlatformConnectivity>,
    queue_health: Entity<QueueHealth>,
    twitch_bot: Entity<TwitchBotPresence>,
    sub: EventSubscription,
) {
    cx.spawn(async move |cx| {
        drain_subscription(sub, cx, move |batch, cx| {
            let decoded: Vec<Option<RuntimeEvent>> =
                batch.iter().map(runtime_event::decode).collect();

            platforms.update(cx, |connectivity, cx| {
                let mut changed = false;
                for typed in decoded.iter().flatten() {
                    changed |= connectivity.apply_runtime_event(typed);
                }
                if changed {
                    cx.notify();
                }
            });

            event_log.update(cx, |log, cx| {
                for (event, typed) in batch.iter().zip(&decoded) {
                    log.record(event, typed.clone());
                }
                if !batch.is_empty() {
                    cx.notify();
                }
            });

            home_stats.update(cx, |stats, cx| {
                let mut changed = false;
                for (event, typed) in batch.iter().zip(&decoded) {
                    changed |= stats.record_event(event, typed.as_ref());
                    if matches!(typed, Some(RuntimeEvent::ActionDone(_))) {
                        stats.record_action_done();
                        changed = true;
                    }
                }
                if changed {
                    cx.notify();
                }
            });

            queue_health.update(cx, |health, cx| {
                let mut changed = false;
                for event in batch {
                    changed |= health.apply_event(event);
                }
                if changed {
                    cx.notify();
                }
            });

            twitch_bot.update(cx, |presence, cx| {
                let mut changed = false;
                for typed in decoded.iter().flatten() {
                    if let RuntimeEvent::TwitchBotStatus(status) = typed {
                        changed |= presence.apply_status_changed(status);
                    }
                }
                if changed {
                    cx.notify();
                }
            });

            cx.update(|cx| {
                for (event, typed) in batch.iter().zip(&decoded) {
                    match typed {
                        Some(RuntimeEvent::ClipPlayback(playback)) => {
                            crate::audio::clip_playback::report_clip_playback(playback, cx);
                        }
                        Some(RuntimeEvent::OverlaySpeechFailed) => {
                            crate::tools::overlay_speech_failure::report_overlay_speech_failure(
                                event, cx,
                            );
                        }
                        _ => {}
                    }
                }
            });

            BridgeFlow::Continue
        })
        .await;
    })
    .detach();
}

pub(super) fn start_loss_bridge(cx: &mut AsyncApp, loss: Entity<EventLoss>, mut watch: LossWatch) {
    cx.spawn(async move |cx| {
        let mut report = watch.current();
        loop {
            loss.update(cx, |loss, cx| {
                if loss.apply_report(report) {
                    cx.notify();
                }
            });
            cx.background_executor().timer(LOSS_REPAINT_INTERVAL).await;
            match watch.changed().await {
                Some(next) => report = next,
                None => break,
            }
        }
    })
    .detach();
}

pub(super) fn start_queue_depth_bridge(
    cx: &mut AsyncApp,
    health: Entity<QueueHealth>,
    mut watch: QueueDepthWatch,
) {
    cx.spawn(async move |cx| {
        let mut depths = watch.current();
        loop {
            let running = watch.running();
            health.update(cx, |health, cx| {
                let depths_changed = health.apply_depths(depths);
                let running_changed = health.apply_running(running);
                if depths_changed || running_changed {
                    cx.notify();
                }
            });
            cx.background_executor()
                .timer(QUEUE_DEPTH_REPAINT_INTERVAL)
                .await;
            match watch.changed().await {
                Some(next) => depths = next,
                None => break,
            }
        }
    })
    .detach();
}

pub(super) fn start_obs_stats_bridge(
    cx: &mut AsyncApp,
    home_stats: Entity<HomeStats>,
    mut watch: forge_obs::ObsStatsWatch,
) {
    cx.spawn(async move |cx| {
        let mut stats = watch.current();
        loop {
            home_stats.update(cx, |home, cx| {
                home.set_obs_stats(stats, time::OffsetDateTime::now_utc());
                cx.notify();
            });
            match watch.changed().await {
                Some(next) => stats = next,
                None => break,
            }
        }
    })
    .detach();
}

pub(super) fn start_awake_bridge(
    cx: &mut AsyncApp,
    awake: Entity<AwakeState>,
    mut watch: StatusWatch,
) {
    cx.spawn(async move |cx| {
        let mut status = watch.current();
        loop {
            awake.update(cx, |awake, cx| {
                if awake.apply(status) {
                    cx.notify();
                }
            });
            match watch.changed().await {
                Ok(next) => status = next,
                Err(_) => break,
            }
        }
    })
    .detach();
}

pub(super) fn start_uptime_clock(cx: &mut AsyncApp, status: Entity<RuntimeStatus>) {
    let mut presence = cx.update(|cx| PresenceGate::of(cx));
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(UPTIME_TICK).await;
            presence.until_visible().await;
            status.update(cx, |status, cx| {
                status.refresh(Instant::now());
                cx.notify();
            });
        }
    })
    .detach();
}

pub(super) fn start_live_viewers_bridge(
    cx: &mut AsyncApp,
    home_stats: Entity<HomeStats>,
    handle: LiveViewerAggregatorHandle,
) {
    cx.spawn(async move |cx| {
        let mut stream = std::pin::pin!(handle.subscribe());
        while let Some(count) = stream.next().await {
            home_stats.update(cx, |stats, cx| {
                if stats.set_live_viewers(count) {
                    cx.notify();
                }
            });
        }
    })
    .detach();
}

pub(super) fn apply_persisted_shortcuts(
    cx: &mut AsyncApp,
    backend: Arc<dyn forge_storage::DataProvider>,
    rt_handle: tokio::runtime::Handle,
) {
    cx.spawn(async move |cx| {
        let repo = backend.settings_repo();
        let reply = crate::app::async_bridge::on_runtime(&rt_handle, async move {
crate::automation::shortcut_overrides::load_overrides(repo).await
        });
        let overrides = match reply.await {
            Ok(Ok(overrides)) => overrides,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "stored keyboard shortcuts unreadable; defaults stay active");
                return;
            }
            Err(_) => return,
        };
        if overrides.is_empty() {
            return;
        }
        cx.update(|cx| overrides.apply(cx));
    })
    .detach();
}

pub(super) fn start_speak_bridge(
    cx: &mut AsyncApp,
    speak: Entity<SpeakState>,
    mut events: SpeakEventStream,
) {
    cx.spawn(async move |cx| {
        loop {
            match events.recv().await {
                Ok(event) => {
                    speak.update(cx, |state, cx| {
                        if state.apply_event(event) {
                            cx.notify();
                        }
                    });
                }
                Err(SpeakError::LaggingReceiver) => {
                    tracing::warn!("speak event bridge lagged; dropped events");
                }
                Err(SpeakError::ActorGone) => break,
            }
        }
    })
    .detach();
}
