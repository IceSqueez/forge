use std::time::Duration;

use forge_components::Platform;
use forge_events::{Event, EventSource};
use forge_runtime::{Delivery, EventSubscription};
use forge_types::EventId;

use super::platform_gate::platform_integration;
use crate::home_stats::Integration;

pub(crate) const CHAT_SEND_REQUEST_KIND: &str = "chat.send.request";
pub(crate) const CHAT_SENT_KIND: &str = "chat.send";
pub(crate) const CHAT_SEND_FAILED_KIND: &str = "chat.send.failed";
pub(crate) const CHAT_SEND_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

const TWITCH_MAX_MESSAGE_CHARS: usize = 500;
const YOUTUBE_MAX_MESSAGE_CHARS: usize = 200;
const KICK_MAX_MESSAGE_CHARS: usize = 500;

pub(crate) fn max_message_chars(platform: Platform) -> usize {
    match platform {
        Platform::Twitch => TWITCH_MAX_MESSAGE_CHARS,
        Platform::YouTube => YOUTUBE_MAX_MESSAGE_CHARS,
        Platform::Kick => KICK_MAX_MESSAGE_CHARS,
    }
}

fn platform_of_channel(channel: &str) -> Option<Platform> {
    match Integration::from_id(channel)? {
        Integration::Twitch => Some(Platform::Twitch),
        Integration::YouTube => Some(Platform::YouTube),
        Integration::Kick => Some(Platform::Kick),
        Integration::Obs | Integration::VTube => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PlatformReach {
    pub enabled: Vec<Platform>,
    pub connected: Vec<Platform>,
}

impl PlatformReach {
    pub(crate) fn selectable(&self) -> Vec<Platform> {
        self.enabled
            .iter()
            .copied()
            .filter(|platform| self.connected.contains(platform))
            .collect()
    }

    pub(crate) fn route(&self, effective: &[Platform]) -> SendRoute {
        let selectable = self.selectable();
        let chosen: Vec<Platform> = effective
            .iter()
            .copied()
            .filter(|platform| selectable.contains(platform))
            .collect();
        let covers_selectable = selectable.iter().all(|platform| chosen.contains(platform));
        match chosen.as_slice() {
            [] => SendRoute::Broadcast,
            [_] => SendRoute::Targeted(chosen),
            _ if covers_selectable => SendRoute::Broadcast,
            _ => SendRoute::Targeted(chosen),
        }
    }

    pub(crate) fn recipients(&self, route: &SendRoute) -> Vec<Platform> {
        match route {
            SendRoute::Broadcast => self.enabled.clone(),
            SendRoute::Targeted(platforms) => platforms.clone(),
        }
    }

    pub(crate) fn char_limit(&self, effective: &[Platform]) -> Option<usize> {
        self.recipients(&self.route(effective))
            .into_iter()
            .map(max_message_chars)
            .min()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SendRoute {
    Broadcast,
    Targeted(Vec<Platform>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SendPlan {
    pub message: String,
    pub route: SendRoute,
    pub recipients: Vec<Platform>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SendRefusal {
    Blank,
    NoPlatformEnabled,
    NoPlatformConnected,
    TooLong { count: usize, limit: usize },
}

pub(crate) fn plan_send(
    text: &str,
    effective: &[Platform],
    reach: &PlatformReach,
) -> Result<SendPlan, SendRefusal> {
    let message = text.trim();
    if message.is_empty() {
        return Err(SendRefusal::Blank);
    }
    if reach.enabled.is_empty() {
        return Err(SendRefusal::NoPlatformEnabled);
    }
    if reach.selectable().is_empty() {
        return Err(SendRefusal::NoPlatformConnected);
    }
    let route = reach.route(effective);
    let recipients = reach.recipients(&route);
    let count = message.chars().count();
    if let Some(limit) = recipients.iter().copied().map(max_message_chars).min()
        && count > limit
    {
        return Err(SendRefusal::TooLong { count, limit });
    }
    Ok(SendPlan {
        message: message.to_owned(),
        route,
        recipients,
    })
}

impl SendPlan {
    pub(crate) fn request_events(&self) -> Vec<Event> {
        match &self.route {
            SendRoute::Broadcast => vec![Event::new(
                EventSource::Core,
                CHAT_SEND_REQUEST_KIND,
                serde_json::json!({ "message": self.message }),
            )],
            SendRoute::Targeted(platforms) => platforms
                .iter()
                .map(|platform| {
                    Event::new(
                        EventSource::Core,
                        CHAT_SEND_REQUEST_KIND,
                        serde_json::json!({
                            "target": platform_integration(*platform).id_str(),
                            "message": self.message,
                        }),
                    )
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeliveryResult {
    Delivered,
    Failed(String),
    NoResponse,
}

pub(crate) fn delivery_response(
    event: &Event,
    requests: &[EventId],
) -> Option<(Platform, DeliveryResult)> {
    let caused_by = event.caused_by?;
    if !requests.contains(&caused_by) {
        return None;
    }
    let platform = event
        .payload
        .get("channel")
        .and_then(|v| v.as_str())
        .and_then(platform_of_channel)?;
    match event.kind.as_str() {
        CHAT_SENT_KIND => Some((platform, DeliveryResult::Delivered)),
        CHAT_SEND_FAILED_KIND => {
            let error = event
                .payload
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned();
            Some((platform, DeliveryResult::Failed(error)))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SendReport {
    pub message: String,
    pub results: Vec<(Platform, DeliveryResult)>,
}

impl SendReport {
    pub(crate) fn unanswered(message: String, recipients: &[Platform]) -> Self {
        Self {
            message,
            results: recipients
                .iter()
                .map(|platform| (*platform, DeliveryResult::NoResponse))
                .collect(),
        }
    }

    pub(crate) fn record(&mut self, platform: Platform, result: DeliveryResult) -> bool {
        match self.results.iter_mut().find(|(recipient, current)| {
            *recipient == platform && *current == DeliveryResult::NoResponse
        }) {
            Some(slot) => {
                slot.1 = result;
                true
            }
            None => false,
        }
    }

    pub(crate) fn is_settled(&self) -> bool {
        self.results
            .iter()
            .all(|(_, result)| *result != DeliveryResult::NoResponse)
    }

    pub(crate) fn any_delivered(&self) -> bool {
        self.results
            .iter()
            .any(|(_, result)| *result == DeliveryResult::Delivered)
    }

    pub(crate) fn problems(&self) -> impl Iterator<Item = &(Platform, DeliveryResult)> {
        self.results
            .iter()
            .filter(|(_, result)| *result != DeliveryResult::Delivered)
    }
}

pub(crate) async fn collect_delivery(
    mut subscription: EventSubscription,
    requests: Vec<EventId>,
    mut report: SendReport,
    timeout: Duration,
) -> SendReport {
    let deadline = tokio::time::Instant::now() + timeout;
    while !report.is_settled() {
        let delivery = match tokio::time::timeout_at(deadline, subscription.next()).await {
            Ok(delivery) => delivery,
            Err(_) => break,
        };
        match delivery {
            Delivery::Event(event) => {
                if let Some((platform, result)) = delivery_response(&event, &requests) {
                    report.record(platform, result);
                }
            }
            Delivery::Skipped(_) => {}
            Delivery::Closed => break,
        }
    }
    report
}
