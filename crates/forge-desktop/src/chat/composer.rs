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
            .filter(|platform| stats.is_connected(platform_integration(*platform)))
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
