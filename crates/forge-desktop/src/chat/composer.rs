use std::sync::Arc;

use forge_components::{ForgePalette, InputBar, InputBarEvent, Platform, ToastKind, tr};
use forge_runtime::EventBus;
use forge_types::EventId;
use gpui::{AppContext, Context, Entity, IntoElement, Render, Subscription, Window};

use super::platform_display_name;
use super::platform_gate::platform_integration;
use super::send_plan::{
    CHAT_SEND_RESPONSE_TIMEOUT, DeliveryResult, PlatformReach, SendPlan, SendRefusal, SendReport,
    collect_delivery, plan_send,
};
use crate::home_stats::HomeStats;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::integration_supervisor::LifecycleState;
use crate::toasts::PushToast;

pub struct ChatComposer {
    input: Entity<InputBar>,
    rt_handle: tokio::runtime::Handle,
    bus: Option<Arc<EventBus>>,
    home_stats: Entity<HomeStats>,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
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
        let input = cx.new(|cx| InputBar::new(tr!("chat_send_placeholder_connected"), palette, cx));
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

    pub(crate) fn is_sending(&self) -> bool {
        self.in_flight.is_some()
    }

    fn current_reach(&self, cx: &Context<Self>) -> PlatformReach {
        let stats = self.home_stats.read(cx);
        let lifecycle = self.lifecycle.as_ref().map(|lifecycle| lifecycle.read(cx));
        let enabled: Vec<Platform> = [Platform::Twitch, Platform::YouTube, Platform::Kick]
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
        let selectable = self.reach.selectable();
        self.input
            .update(cx, |bar, cx| bar.set_available_targets(&selectable, cx));
        self.refresh_targets(cx);
        cx.notify();
    }

    fn refresh_targets(&mut self, cx: &mut Context<Self>) {
        let effective = self.input.read(cx).effective_targets();
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
            InputBarEvent::Send { text, targets } => self.send(text, targets, cx),
            InputBarEvent::TargetsChanged => {
                self.refresh_targets(cx);
                cx.notify();
            }
            InputBarEvent::EmojiToggled => {}
        }
    }

    fn send(&mut self, text: &str, targets: &[Platform], cx: &mut Context<Self>) {
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
        let requests = plan.request_events();
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
            let _ = tx.send(report);
        });
        cx.spawn(async move |this, cx| {
            let report = rx.await.unwrap_or(fallback);
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_components::{FORGE_DEFAULT, Platform, ToastKind, tr};
    use forge_events::{Event, EventSource};
    use forge_runtime::{EventBus, NullEventLogRepo};
    use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

    use super::super::platform_display_name;
    use super::ChatComposer;
    use crate::chat::send_plan::{
        CHAT_SEND_REQUEST_KIND, CHAT_SENT_KIND, DeliveryResult, SendReport,
    };
    use crate::home_stats::{HomeStats, Integration};
    use crate::test_support::{pump, runtime};
    use crate::toasts::Toasts;

    fn mount<'a>(
        cx: &'a mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        connected: &[Integration],
    ) -> (Entity<ChatComposer>, &'a mut VisualTestContext) {
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

    fn type_text(composer: &Entity<ChatComposer>, vcx: &mut VisualTestContext, text: &str) {
        vcx.update(|window, cx| {
            let input = composer.read(cx).input.clone();
            input.update(cx, |bar, cx| bar.focus(window, cx));
        });
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
                composer.send(text, &[Platform::Twitch], cx);
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
            type_text(&composer, vcx, typed);
            let report = SendReport {
                message: "hello".to_owned(),
                results: vec![(Platform::Twitch, result.clone())],
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
                (Platform::Twitch, DeliveryResult::Delivered),
                (Platform::Kick, DeliveryResult::Failed("banned".to_owned())),
            ],
        };

        vcx.update(|_window, cx| {
            composer.update(cx, |composer, cx| composer.apply_report(report, cx));
        });

        assert_eq!(
            error_toasts(vcx),
            vec![tr!(
                "chat_send_failed",
                platform = platform_display_name(Platform::Kick),
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
        type_text(&composer, vcx, "hello");

        send(&composer, vcx, "hello");
        pump(&rt);
        let request = observer
            .try_recv()
            .unwrap()
            .expect("a send request on the bus");
        assert_eq!(request.kind, CHAT_SEND_REQUEST_KIND);
        bus.publish(Event::caused_by(
            EventSource::Twitch,
            CHAT_SENT_KIND,
            serde_json::json!({ "channel": "twitch", "message": "hello" }),
            request.id,
        ));
        pump(&rt);
        vcx.run_until_parked();

        assert_eq!(field(&composer, vcx), "");
        assert!(!vcx.update(|_window, cx| composer.read(cx).is_sending()));
    }
}
