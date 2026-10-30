use crate::platform_visuals::{send_target, send_target_id, send_target_platforms};
use std::sync::Arc;

use forge_components::{ForgePalette, ToastKind, tr};
use forge_platform_core::SenderPreference;
use forge_runtime::EventBus;
use forge_types::{EventId, PlatformId};
use gpui::{AppContext, Context, Entity, IntoElement, Render, SharedString, Subscription, Window};

use super::platform_display_name;
use super::platform_gate::platform_integration;
use super::send_plan::{
    CHAT_SEND_RESPONSE_TIMEOUT, DeliveryResult, PlatformReach, SendPlan, SendRefusal, SendReport,
    collect_delivery, plan_send,
};
use crate::chat_input_bar::{InputBar, InputBarEvent, InputBarSwitch};
use crate::home_stats::HomeStats;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::twitch_bot_presence::TwitchBotPresence;
use forge_components::PushToast;
use forge_runtime::LifecycleState;

const BOT_SWITCH_OPTION: usize = 1;

pub struct ChatComposer {
    input: Entity<InputBar>,
    rt_handle: tokio::runtime::Handle,
    bus: Option<Arc<EventBus>>,
    home_stats: Entity<HomeStats>,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
    twitch_bot: Option<Entity<TwitchBotPresence>>,
    reach: PlatformReach,
    in_flight: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl ChatComposer {
    pub fn new(
        home_stats: Entity<HomeStats>,
        rt_handle: tokio::runtime::Handle,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let targets = PlatformId::ALL
            .into_iter()
            .map(|platform| (send_target(platform, &palette), true))
            .collect();
        let input = cx.new(|cx| {
            InputBar::new(tr!("chat_send_placeholder_connected"), palette, cx).with_targets(targets)
        });
        let subscriptions = vec![
            cx.subscribe(&input, Self::on_input_event),
            cx.observe(&home_stats, |this, _, cx| this.refresh_reach(cx)),
        ];
        let mut this = Self {
            input,
            rt_handle,
            bus: None,
            home_stats,
            lifecycle: None,
            twitch_bot: None,
            reach: PlatformReach::default(),
            in_flight: None,
            _subscriptions: subscriptions,
        };
        this.reach = this.current_reach(cx);
        this.apply_reach(cx);
        this
    }

    pub fn set_bus(&mut self, bus: Arc<EventBus>, cx: &mut Context<Self>) {
        self.bus = Some(bus);
        cx.notify();
    }

    pub fn set_lifecycle(
        &mut self,
        lifecycle: Entity<IntegrationLifecycle>,
        cx: &mut Context<Self>,
    ) {
        self._subscriptions
            .push(cx.observe(&lifecycle, |this, _, cx| this.refresh_reach(cx)));
        self.lifecycle = Some(lifecycle);
        self.refresh_reach(cx);
    }

    pub fn set_twitch_bot(&mut self, presence: Entity<TwitchBotPresence>, cx: &mut Context<Self>) {
        self._subscriptions
            .push(cx.observe(&presence, |this, _, cx| this.refresh_sender_switch(cx)));
        self.twitch_bot = Some(presence);
        self.refresh_sender_switch(cx);
    }

    pub(crate) fn twitch_sender(&self, cx: &Context<Self>) -> Option<SenderPreference> {
        let presence = self.twitch_bot.as_ref()?.read(cx);
        if !presence.is_connected() || !self.reach.selectable().contains(&PlatformId::Twitch) {
            return None;
        }
        Some(if presence.composer_sends_as_bot() {
            SenderPreference::BotOnly
        } else {
            SenderPreference::Streamer
        })
    }

    fn refresh_sender_switch(&mut self, cx: &mut Context<Self>) {
        let switch = self.twitch_sender(cx).map(|sender| InputBarSwitch {
            options: vec![
                tr!("chat_send_as_me").into(),
                tr!("chat_send_as_bot").into(),
            ],
            selected: usize::from(sender == SenderPreference::BotOnly),
        });
        self.input.update(cx, |bar, cx| bar.set_switch(switch, cx));
        cx.notify();
    }

    fn pick_sender(&mut self, option: usize, cx: &mut Context<Self>) {
        let Some(presence) = self.twitch_bot.clone() else {
            return;
        };
        presence.update(cx, |presence, cx| {
            if presence.set_composer_sends_as_bot(option == BOT_SWITCH_OPTION) {
                cx.notify();
            }
        });
    }

    pub(crate) fn is_sending(&self) -> bool {
        self.in_flight.is_some()
    }

    fn current_reach(&self, cx: &Context<Self>) -> PlatformReach {
        let stats = self.home_stats.read(cx);
        let lifecycle = self.lifecycle.as_ref().map(|lifecycle| lifecycle.read(cx));
        let enabled: Vec<PlatformId> = PlatformId::ALL
            .into_iter()
            .filter(|platform| {
                lifecycle.is_none_or(|lifecycle| {
                    lifecycle.is_on(&platform_integration(*platform).builtin_id())
                })
            })
            .collect();
        let connected = enabled
            .iter()
            .copied()
            .filter(|platform| {
                let running = lifecycle.is_none_or(|lifecycle| {
                    lifecycle.state_of(&platform_integration(*platform).builtin_id())
                        == LifecycleState::Running
                });
                running && stats.is_connected(platform_integration(*platform))
            })
            .collect();
        PlatformReach { enabled, connected }
    }

    fn refresh_reach(&mut self, cx: &mut Context<Self>) {
        let reach = self.current_reach(cx);
        if reach != self.reach {
            self.reach = reach;
            self.apply_reach(cx);
        }
    }

    fn apply_reach(&mut self, cx: &mut Context<Self>) {
        let selectable: Vec<SharedString> = self
            .reach
            .selectable()
            .into_iter()
            .map(send_target_id)
            .collect();
        self.input
            .update(cx, |bar, cx| bar.set_available_targets(&selectable, cx));
        self.refresh_targets(cx);
        self.refresh_sender_switch(cx);
        cx.notify();
    }

    fn refresh_targets(&mut self, cx: &mut Context<Self>) {
        let effective = send_target_platforms(&self.input.read(cx).effective_targets());
        let limit = self.reach.char_limit(&effective);
        let placeholder = match (self.reach.selectable().is_empty(), effective.as_slice()) {
            (true, _) => tr!("chat_send_placeholder_offline"),
            (false, [only]) => tr!(
                "chat_send_placeholder_to",
                platform = platform_display_name(*only)
            ),
            (false, _) => tr!("chat_send_placeholder_connected"),
        };
        self.input.update(cx, |bar, cx| {
            bar.set_max_chars(limit, cx);
            bar.set_placeholder(placeholder, cx);
        });
    }

    fn on_input_event(
        &mut self,
        _input: Entity<InputBar>,
        event: &InputBarEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputBarEvent::Send { text, targets } => {
                self.send(text, &send_target_platforms(targets), cx)
            }
            InputBarEvent::TargetsChanged => {
                self.refresh_targets(cx);
                cx.notify();
            }
            InputBarEvent::SwitchChanged(option) => self.pick_sender(*option, cx),
            InputBarEvent::EmojiToggled => {}
        }
    }

    fn send(&mut self, text: &str, targets: &[PlatformId], cx: &mut Context<Self>) {
        if self.is_sending() {
            return;
        }
        match plan_send(text, targets, &self.reach) {
            Ok(plan) => self.dispatch(plan, cx),
            Err(refusal) => {
                if let Some(message) = refusal_message(&refusal) {
                    cx.push_toast(ToastKind::Error, message);
                }
            }
        }
        cx.notify();
    }

    fn dispatch(&mut self, plan: SendPlan, cx: &mut Context<Self>) {
        let Some(bus) = self.bus.clone() else {
            cx.push_toast(ToastKind::Error, tr!("chat_send_unavailable"));
            return;
        };
        let requests = plan.request_events(self.twitch_sender(cx));
        let request_ids: Vec<EventId> = requests.iter().map(|event| event.id).collect();
        let pending = SendReport::unanswered(plan.message.clone(), &plan.recipients);
        let fallback = pending.clone();
        self.in_flight = Some(plan.message);

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let subscription = bus.subscribe();
            for request in requests {
                bus.publish(request);
            }
            let report = collect_delivery(
                subscription,
                request_ids,
                pending,
                CHAT_SEND_RESPONSE_TIMEOUT,
            )
            .await;
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = tx.send(report);
        });
        cx.spawn(async move |this, cx| {
            let report = rx.await.unwrap_or(fallback);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| this.apply_report(report, cx));
        })
        .detach();
    }

    fn apply_report(&mut self, report: SendReport, cx: &mut Context<Self>) {
        self.in_flight = None;
        if report.any_delivered() {
            let unchanged = self.input.read(cx).content(cx).trim() == report.message;
            if unchanged {
                self.input.update(cx, |bar, cx| bar.clear(cx));
            }
        }
        for (platform, result) in report.problems() {
            let name = platform_display_name(*platform);
            let message = match result {
                DeliveryResult::Failed(error) => {
                    tr!("chat_send_failed", platform = name, error = error.clone())
                }
                DeliveryResult::NoResponse => tr!("chat_send_no_response", platform = name),
                DeliveryResult::Delivered => continue,
            };
            cx.push_toast(ToastKind::Error, message);
        }
        cx.notify();
    }
}

fn refusal_message(refusal: &SendRefusal) -> Option<String> {
    match refusal {
        SendRefusal::Blank => None,
        SendRefusal::NoPlatformEnabled => Some(tr!("chat_send_no_platform_enabled")),
        SendRefusal::NoPlatformConnected => Some(tr!("chat_send_no_platform_connected")),
        SendRefusal::TooLong { count, limit } => {
            Some(tr!("chat_send_too_long", count = *count, limit = *limit))
        }
    }
}

impl Render for ChatComposer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.input.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_components::{FORGE_DEFAULT, ToastKind, tr};
    use forge_events::{Event, EventSource};
    use forge_runtime::{EventBus, NullEventLogRepo};
    use forge_types::PlatformId;
    use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

    use super::super::platform_display_name;
    use super::super::send_plan::PlatformReach;
    use super::ChatComposer;
    use crate::chat::send_plan::{DeliveryResult, SendReport};
    use crate::chat_input_bar::InputBarEvent;
    use crate::home_stats::{HomeStats, Integration};
    use crate::integration_lifecycle::IntegrationLifecycle;
    use crate::test_support::{install_presentation, pump, runtime};
    use crate::twitch_bot_presence::TwitchBotPresence;
    use forge_components::Toasts;
    use forge_events::{ChatSendRequest, ChatSent, TypedEvent};
    use forge_platform_core::SENDER_FIELD;
    use forge_platform_twitch::{BotModerator, BotStatus};
    use forge_runtime::{LifecycleState, LifecycleStates};

    fn mount<'a>(
        cx: &'a mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        connected: &[Integration],
    ) -> (Entity<ChatComposer>, &'a mut VisualTestContext) {
        install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let connections: Vec<(Integration, bool)> =
            [Integration::Twitch, Integration::YouTube, Integration::Kick]
                .into_iter()
                .map(|integration| (integration, connected.contains(&integration)))
                .collect();
        let home_stats = cx.new(|_| {
            let mut stats = HomeStats::new();
            stats.set_connections(&connections);
            stats
        });
        let handle = rt.handle().clone();
        cx.add_window_view(|_window, cx| ChatComposer::new(home_stats, handle, FORGE_DEFAULT, cx))
    }

    fn type_text(vcx: &mut VisualTestContext, text: &str) {
        vcx.update(|window, cx| window.focus_next(cx));
        vcx.simulate_input(text);
        vcx.run_until_parked();
    }

    fn field(composer: &Entity<ChatComposer>, vcx: &mut VisualTestContext) -> String {
        vcx.update(|_window, cx| composer.read(cx).input.read(cx).content(cx))
    }

    fn error_toasts(vcx: &mut VisualTestContext) -> Vec<String> {
        vcx.update(|_window, cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .filter(|toast| toast.kind == ToastKind::Error)
                .map(|toast| toast.message.to_string())
                .collect()
        })
    }

    fn send(composer: &Entity<ChatComposer>, vcx: &mut VisualTestContext, text: &str) {
        vcx.update(|_window, cx| {
            composer.update(cx, |composer, cx| {
                composer.send(text, &[PlatformId::Twitch], cx);
            });
        });
        vcx.run_until_parked();
    }

    fn connected_bus(
        composer: &Entity<ChatComposer>,
        vcx: &mut VisualTestContext,
    ) -> Arc<EventBus> {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let handed = Arc::clone(&bus);
        vcx.update(|_window, cx| composer.update(cx, |composer, cx| composer.set_bus(handed, cx)));
        bus
    }

    #[gpui::test]
    fn sending_without_a_bus_toasts_unavailable_and_stays_idle(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch]);

        send(&composer, vcx, "hello");

        assert_eq!(error_toasts(vcx), vec![tr!("chat_send_unavailable")]);
        assert!(!vcx.update(|_window, cx| composer.read(cx).is_sending()));
    }

    #[gpui::test]
    fn refused_sends_toast_the_reason_except_for_blank_text(cx: &mut TestAppContext) {
        for (connected, text, expected) in [
            (
                vec![],
                "hello",
                vec![tr!("chat_send_no_platform_connected")],
            ),
            (vec![Integration::Twitch], "   ", vec![]),
        ] {
            let rt = runtime();
            let (composer, vcx) = mount(cx, &rt, &connected);

            send(&composer, vcx, text);

            assert_eq!(
                error_toasts(vcx),
                expected,
                "text {text:?} with {connected:?}"
            );
        }
    }

    #[gpui::test]
    fn a_second_send_while_one_is_in_flight_is_ignored(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch]);
        connected_bus(&composer, vcx);

        send(&composer, vcx, "first");
        send(&composer, vcx, "second");

        let in_flight = vcx.update(|_window, cx| composer.read(cx).in_flight.clone());
        assert_eq!(in_flight.as_deref(), Some("first"));
    }

    #[gpui::test]
    fn the_report_clears_the_field_only_when_delivered_and_the_text_is_unchanged(
        cx: &mut TestAppContext,
    ) {
        for (typed, result, cleared) in [
            ("hello ", DeliveryResult::Delivered, true),
            ("hello again", DeliveryResult::Delivered, false),
            ("hello", DeliveryResult::Failed("banned".to_owned()), false),
            ("hello", DeliveryResult::NoResponse, false),
        ] {
            let rt = runtime();
            let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch]);
            type_text(vcx, typed);
            let report = SendReport {
                message: "hello".to_owned(),
                results: vec![(PlatformId::Twitch, result.clone())],
            };

            vcx.update(|_window, cx| {
                composer.update(cx, |composer, cx| composer.apply_report(report, cx));
            });

            let expected = if cleared { "" } else { typed };
            assert_eq!(
                field(&composer, vcx),
                expected,
                "typed {typed:?}, {result:?}"
            );
        }
    }

    #[gpui::test]
    fn an_undelivered_report_toasts_once_per_problem_platform(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch, Integration::Kick]);
        let report = SendReport {
            message: "hello".to_owned(),
            results: vec![
                (PlatformId::Twitch, DeliveryResult::Delivered),
                (
                    PlatformId::Kick,
                    DeliveryResult::Failed("banned".to_owned()),
                ),
            ],
        };

        vcx.update(|_window, cx| {
            composer.update(cx, |composer, cx| composer.apply_report(report, cx));
        });

        assert_eq!(
            error_toasts(vcx),
            vec![tr!(
                "chat_send_failed",
                platform = platform_display_name(PlatformId::Kick),
                error = "banned".to_owned()
            )]
        );
    }

    #[gpui::test]
    fn a_send_the_platform_confirms_clears_the_field(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch]);
        let bus = connected_bus(&composer, vcx);
        let mut observer = bus.subscribe();
        type_text(vcx, "hello");

        send(&composer, vcx, "hello");
        pump(&rt);
        let request = observer
            .try_recv()
            .unwrap()
            .expect("a send request on the bus");
        assert_eq!(request.kind, ChatSendRequest::KIND);
        bus.publish(Event::caused_by(
            EventSource::Twitch,
            ChatSent::KIND,
            serde_json::json!({ "channel": "twitch", "message": "hello" }),
            request.id,
        ));
        pump(&rt);
        vcx.run_until_parked();

        assert_eq!(field(&composer, vcx), "");
        assert!(!vcx.update(|_window, cx| composer.read(cx).is_sending()));
    }

    #[gpui::test]
    fn reach_counts_only_running_integrations_as_connected(cx: &mut TestAppContext) {
        let twitch = Integration::Twitch.builtin_id();
        let youtube = Integration::YouTube.builtin_id();
        let kick = Integration::Kick.builtin_id();
        let rt = runtime();
        let (composer, vcx) = mount(
            cx,
            &rt,
            &[Integration::Twitch, Integration::YouTube, Integration::Kick],
        );
        let states: LifecycleStates = [
            (twitch, LifecycleState::Running),
            (
                youtube,
                LifecycleState::Failed(crate::test_support::start_failure("boom")),
            ),
            (kick, LifecycleState::Starting),
        ]
        .into_iter()
        .collect();
        let lifecycle = vcx.update(|_window, cx| cx.new(|_| IntegrationLifecycle::new(states)));

        vcx.update(|_window, cx| {
            composer.update(cx, |composer, cx| composer.set_lifecycle(lifecycle, cx));
        });

        let reach = vcx
            .update(|_window, cx| composer.update(cx, |composer, cx| composer.current_reach(cx)));
        assert_eq!(
            reach,
            PlatformReach {
                enabled: vec![PlatformId::Twitch, PlatformId::YouTube, PlatformId::Kick],
                connected: vec![PlatformId::Twitch],
            }
        );
    }

    #[gpui::test]
    fn reach_has_no_connected_platform_while_every_enabled_one_is_starting_or_failed(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch, Integration::YouTube]);
        let states: LifecycleStates = [
            (Integration::Twitch.builtin_id(), LifecycleState::Starting),
            (
                Integration::YouTube.builtin_id(),
                LifecycleState::Failed(crate::test_support::start_failure("boom")),
            ),
        ]
        .into_iter()
        .collect();
        let lifecycle = vcx.update(|_window, cx| cx.new(|_| IntegrationLifecycle::new(states)));

        vcx.update(|_window, cx| {
            composer.update(cx, |composer, cx| composer.set_lifecycle(lifecycle, cx));
        });

        let reach = vcx
            .update(|_window, cx| composer.update(cx, |composer, cx| composer.current_reach(cx)));
        assert_eq!(reach.enabled, vec![PlatformId::Twitch, PlatformId::YouTube]);
        assert!(reach.connected.is_empty());
    }

    fn bot_status(login: Option<&str>) -> BotStatus {
        match login {
            Some(login) => BotStatus::Connected {
                login: login.to_owned(),
            },
            None => BotStatus::NotConnected,
        }
    }

    fn attach_bot(
        composer: &Entity<ChatComposer>,
        vcx: &mut VisualTestContext,
        login: Option<&str>,
    ) -> Entity<TwitchBotPresence> {
        let status = bot_status(login);
        vcx.update(|_window, cx| {
            let presence = cx.new(|_| {
                let mut presence = TwitchBotPresence::new();
                presence.apply(status, BotModerator::Moderator);
                presence
            });
            composer.update(cx, |composer, cx| {
                composer.set_twitch_bot(presence.clone(), cx);
            });
            presence
        })
    }

    fn set_bot(
        presence: &Entity<TwitchBotPresence>,
        vcx: &mut VisualTestContext,
        login: Option<&str>,
    ) {
        let status = bot_status(login);
        vcx.update(|_window, cx| {
            presence.update(cx, |presence, cx| {
                if presence.apply(status, BotModerator::Moderator) {
                    cx.notify();
                }
            });
        });
        vcx.run_until_parked();
    }

    fn pick_switch(composer: &Entity<ChatComposer>, vcx: &mut VisualTestContext, option: usize) {
        vcx.update(|_window, cx| {
            let input = composer.read(cx).input.clone();
            input.update(cx, |_, cx| cx.emit(InputBarEvent::SwitchChanged(option)));
        });
        vcx.run_until_parked();
    }

    fn sent_sender(
        composer: &Entity<ChatComposer>,
        vcx: &mut VisualTestContext,
        rt: &tokio::runtime::Runtime,
        bus: &Arc<EventBus>,
    ) -> Option<String> {
        let mut observer = bus.subscribe();
        send(composer, vcx, "hello");
        pump(rt);
        let request = observer
            .try_recv()
            .unwrap()
            .expect("a send request on the bus");
        request
            .payload
            .get(SENDER_FIELD)
            .and_then(|sender| sender.as_str())
            .map(str::to_owned)
    }

    #[gpui::test]
    fn sends_carry_no_sender_while_no_bot_is_connected(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch]);
        let bus = connected_bus(&composer, vcx);
        attach_bot(&composer, vcx, None);

        assert_eq!(sent_sender(&composer, vcx, &rt, &bus), None);
    }

    #[gpui::test]
    fn sends_go_as_the_streamer_until_the_bot_is_picked(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch]);
        let bus = connected_bus(&composer, vcx);
        attach_bot(&composer, vcx, Some("forgebot"));

        assert_eq!(
            sent_sender(&composer, vcx, &rt, &bus).as_deref(),
            Some("streamer")
        );
    }

    #[gpui::test]
    fn picking_bot_on_the_switch_sends_as_the_bot_only(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch]);
        let bus = connected_bus(&composer, vcx);
        attach_bot(&composer, vcx, Some("forgebot"));

        pick_switch(&composer, vcx, 1);

        assert_eq!(
            sent_sender(&composer, vcx, &rt, &bus).as_deref(),
            Some("bot")
        );
    }

    #[gpui::test]
    fn a_bot_reconnect_after_a_disconnect_starts_back_at_me(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch]);
        let bus = connected_bus(&composer, vcx);
        let presence = attach_bot(&composer, vcx, Some("forgebot"));
        pick_switch(&composer, vcx, 1);

        set_bot(&presence, vcx, None);
        let while_gone = sent_sender(&composer, vcx, &rt, &bus);
        release(&composer, vcx);
        set_bot(&presence, vcx, Some("forgebot"));
        let after_return = sent_sender(&composer, vcx, &rt, &bus);

        assert_eq!(
            (while_gone, after_return.as_deref()),
            (None, Some("streamer"))
        );
    }

    fn set_connected(
        composer: &Entity<ChatComposer>,
        vcx: &mut VisualTestContext,
        connected: &[Integration],
    ) {
        let connections: Vec<(Integration, bool)> =
            [Integration::Twitch, Integration::YouTube, Integration::Kick]
                .into_iter()
                .map(|integration| (integration, connected.contains(&integration)))
                .collect();
        vcx.update(|_window, cx| {
            let stats = composer.read(cx).home_stats.clone();
            stats.update(cx, |stats, cx| {
                stats.set_connections(&connections);
                cx.notify();
            });
        });
        vcx.run_until_parked();
    }

    fn release(composer: &Entity<ChatComposer>, vcx: &mut VisualTestContext) {
        vcx.update(|_window, cx| composer.update(cx, |composer, _| composer.in_flight = None));
    }

    #[gpui::test]
    fn a_connected_bot_adds_no_sender_while_twitch_is_not_reachable(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::YouTube]);
        let bus = connected_bus(&composer, vcx);
        attach_bot(&composer, vcx, Some("forgebot"));
        pick_switch(&composer, vcx, 1);

        assert_eq!(sent_sender(&composer, vcx, &rt, &bus), None);
    }

    #[gpui::test]
    fn the_sender_follows_twitch_dropping_out_and_coming_back(cx: &mut TestAppContext) {
        let rt = runtime();
        let (composer, vcx) = mount(cx, &rt, &[Integration::Twitch, Integration::YouTube]);
        let bus = connected_bus(&composer, vcx);
        attach_bot(&composer, vcx, Some("forgebot"));
        pick_switch(&composer, vcx, 1);

        set_connected(&composer, vcx, &[Integration::YouTube]);
        let while_out = sent_sender(&composer, vcx, &rt, &bus);
        release(&composer, vcx);
        set_connected(&composer, vcx, &[Integration::Twitch, Integration::YouTube]);
        let back = sent_sender(&composer, vcx, &rt, &bus);

        assert_eq!((while_out, back.as_deref()), (None, Some("bot")));
    }
}
