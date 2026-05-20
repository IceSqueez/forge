//! Command permission stored on `Command` rows is currently informational only;
//! every matched command dispatches regardless of the chatter's role. Badge
//! parsing from EventSub payloads is required before permission filtering can
//! be enforced.

use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use forge_events::{Event, EventSource};
use forge_storage::DataProvider;
use forge_types::{ArgStack, CommandId, Variant};
use serde_json::json;
use tracing::warn;

use crate::{EventBus, EventSubscription, QueueSchedulerHandle, SchedulerRequest};

#[derive(Clone)]
pub struct CommandParserHandle {
    cancel: Arc<AtomicBool>,
}

impl CommandParserHandle {
    pub fn shutdown(self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub struct CommandParser {
    bus: Arc<EventBus>,
    dp: Arc<dyn DataProvider>,
    scheduler: QueueSchedulerHandle,
    subscription: EventSubscription,
    cooldowns: HashMap<CommandId, Instant>,
}

impl CommandParser {
    pub fn spawn(
        bus: Arc<EventBus>,
        dp: Arc<dyn DataProvider>,
        scheduler: QueueSchedulerHandle,
    ) -> CommandParserHandle {
        let subscription = bus.subscribe();
        let parser = Self {
            bus: Arc::clone(&bus),
            dp,
            scheduler,
            subscription,
            cooldowns: HashMap::new(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        tokio::spawn(async move { parser.run(cancel_clone).await });
        CommandParserHandle { cancel }
    }

    async fn run(mut self, cancel: Arc<AtomicBool>) {
        while !cancel.load(Ordering::Relaxed) {
            match self.subscription.recv().await {
                Ok(event)
                    if event.source == EventSource::Twitch && event.kind == "chat.message" =>
                {
                    self.handle_chat_message(event).await;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }

    async fn handle_chat_message(&mut self, event: Event) {
        let message = match event.payload.get("message").and_then(|v| v.as_str()) {
            Some(m) => m.to_string(),
            None => return,
        };

        let first_token = match message.split_whitespace().next() {
            Some(t) => t,
            None => return,
        };

        if !first_token.starts_with('!') {
            return;
        }

        let normalized = first_token.to_ascii_lowercase();

        let command = match self.dp.command_repo().get_by_name(&normalized).await {
            Ok(Some(c)) => c,
            Ok(None) => return,
            Err(e) => {
                warn!("command_repo.get_by_name failed: {e}");
                return;
            }
        };

        let user_login: Option<String> = event
            .payload
            .get("user_login")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let channel: Option<String> = event
            .payload
            .get("channel")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if let Some(last) = self.cooldowns.get(&command.id) {
            let elapsed = last.elapsed();
            let cooldown_dur = Duration::from_secs(command.cooldown_secs);
            if elapsed < cooldown_dur {
                let remaining_ms = (cooldown_dur - elapsed).as_millis() as u64;
                self.bus.publish(Event::caused_by(
                    EventSource::Core,
                    "command.cooldown_blocked",
                    json!({
                        "command": normalized,
                        "channel": channel.as_deref().unwrap_or(""),
                        "user_login": user_login.as_deref().unwrap_or(""),
                        "cooldown_remaining_ms": remaining_ms,
                    }),
                    event.id,
                ));
                return;
            }
        }

        self.cooldowns.insert(command.id, Instant::now());

        let cmd_event = Event::caused_by(
            EventSource::Core,
            "command.matched",
            json!({
                "command_name": normalized,
                "command_id": command.id.to_string(),
                "action_id": command.action_id.to_string(),
                "user_login": user_login,
                "raw_message": message,
            }),
            event.id,
        );
        let cmd_event_id = cmd_event.id;
        self.bus.publish(cmd_event);

        let action = match self.dp.action_repo().get(command.action_id).await {
            Ok(Some(a)) => a,
            Ok(None) => {
                warn!(
                    "command {} references unknown action {}",
                    command.id, command.action_id
                );
                return;
            }
            Err(e) => {
                warn!("action_repo.get failed: {e}");
                return;
            }
        };

        let cmd_args = message
            .split_once(' ')
            .map(|x| x.1)
            .unwrap_or("")
            .to_string();

        let mut args = ArgStack::new();
        args = args.set("message".into(), Variant::String(message.clone()));
        args = args.set("args".into(), Variant::String(cmd_args));
        if let Some(user) = user_login {
            args = args.set("user".into(), Variant::String(user));
        }

        let req = SchedulerRequest {
            queue_id: action.queue_id,
            action_id: command.action_id,
            trigger_event_id: cmd_event_id,
            initial_args: args,
            bypass_pause: action.bypass_pause,
        };

        if let Err(e) = self.scheduler.dispatch(req).await {
            warn!("scheduler dispatch failed: {e}");
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_events::{Event, EventSource};
    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{
        Action, ActionId, Command, CommandId, CommandPermission, LogLevel, Queue, QueueId,
        SubActionSpec,
    };
    use serde_json::json;

    use super::*;
    use crate::{
        EventBus, EventSubscription, NullEventLogRepo, QueueScheduler, ScriptRegistry,
        spawn_action_engine,
    };

    async fn make_dp() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    fn log_action(id: ActionId, queue_id: QueueId) -> Action {
        Action {
            id,
            name: "test-action".to_string(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            description: None,
            sub_actions: vec![SubActionSpec::Log {
                level: LogLevel::Info,
                message: "ok".to_string(),
            }],
        }
    }

    fn make_command(id: CommandId, action_id: ActionId, name: &str, cooldown_secs: u64) -> Command {
        Command {
            id,
            action_id,
            name: name.to_string(),
            cooldown_secs,
            permission: CommandPermission::Everyone,
        }
    }

    async fn seed(dp: &Arc<dyn DataProvider>, queue: &Queue, action: &Action, command: &Command) {
        dp.queue_repo().save(queue).await.unwrap();
        dp.action_repo().save(action).await.unwrap();
        dp.command_repo().save(command).await.unwrap();
    }

    fn chat_event(message: &str, user: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "chat.message",
            json!({
                "message": message,
                "user_login": user,
            }),
        )
    }

    async fn collect_kind(
        sub: &mut EventSubscription,
        target: &str,
        attempts: usize,
    ) -> Option<Event> {
        for _ in 0..attempts {
            match tokio::time::timeout(Duration::from_millis(300), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == target => return Some(ev),
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        None
    }

    async fn drain_no_kind(sub: &mut EventSubscription, forbidden: &str, wait_ms: u64) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(50), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == forbidden => return true,
                Ok(Ok(_)) => {}
                _ => {}
            }
        }
        false
    }

    #[tokio::test]
    async fn matched_event_caused_by_chat_message() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let c_id = CommandId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let command = make_command(c_id, a_id, "!quote", 0);
        seed(&dp, &queue, &action, &command).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        tokio::time::sleep(Duration::from_millis(10)).await;

        let chat = chat_event("!quote", "viewer1");
        let chat_id = chat.id;
        bus.publish(chat);

        let matched = collect_kind(&mut sub, "command.matched", 30).await.unwrap();
        assert_eq!(matched.caused_by, Some(chat_id));
        assert_eq!(matched.payload["command_name"].as_str().unwrap(), "!quote");
    }

    #[tokio::test]
    async fn cooldown_blocks_rapid_second_call() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let c_id = CommandId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let command = make_command(c_id, a_id, "!quote", 60);
        seed(&dp, &queue, &action, &command).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        bus.publish(chat_event("!quote", "viewer1"));
        tokio::time::sleep(Duration::from_millis(50)).await;
        bus.publish(chat_event("!quote", "viewer1"));

        let mut matched_count = 0usize;
        let mut blocked_count = 0usize;
        for _ in 0..40 {
            match tokio::time::timeout(Duration::from_millis(150), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "command.matched" => matched_count += 1,
                Ok(Ok(ev)) if ev.kind == "command.cooldown_blocked" => blocked_count += 1,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }

        assert_eq!(
            matched_count, 1,
            "only first invocation fires command.matched"
        );
        assert_eq!(blocked_count, 1, "second invocation emits cooldown_blocked");
    }

    #[tokio::test]
    async fn case_insensitive_command_name() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let c_id = CommandId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let command = make_command(c_id, a_id, "!quote", 0);
        seed(&dp, &queue, &action, &command).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        bus.publish(chat_event("!QUOTE", "viewer1"));

        let matched = collect_kind(&mut sub, "command.matched", 30).await;
        assert!(
            matched.is_some(),
            "uppercase !QUOTE must match stored !quote"
        );
    }

    #[tokio::test]
    async fn non_command_message_emits_nothing() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let c_id = CommandId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let command = make_command(c_id, a_id, "!quote", 0);
        seed(&dp, &queue, &action, &command).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        bus.publish(chat_event("hello world", "viewer1"));

        let saw = drain_no_kind(&mut sub, "command.matched", 150).await;
        assert!(!saw, "plain chat message must not produce command.matched");
    }

    #[tokio::test]
    async fn unknown_command_emits_nothing() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let c_id = CommandId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let command = make_command(c_id, a_id, "!quote", 0);
        seed(&dp, &queue, &action, &command).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        bus.publish(chat_event("!unknown", "viewer1"));

        let saw = drain_no_kind(&mut sub, "command.matched", 150).await;
        assert!(!saw, "unknown command must not produce command.matched");
    }

    #[tokio::test]
    async fn argstack_populated_from_message() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let c_id = CommandId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let command = make_command(c_id, a_id, "!quote", 0);
        seed(&dp, &queue, &action, &command).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        bus.publish(chat_event("!quote some args here", "viewer1"));

        let matched = collect_kind(&mut sub, "command.matched", 30).await.unwrap();
        assert_eq!(
            matched.payload["raw_message"].as_str().unwrap(),
            "!quote some args here"
        );

        let done = collect_kind(&mut sub, "action.done", 30).await;
        assert!(done.is_some(), "action must execute after command.matched");
    }

    #[tokio::test]
    async fn cooldown_blocked_has_causation_and_payload() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let c_id = CommandId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let command = make_command(c_id, a_id, "!ping", 60);
        seed(&dp, &queue, &action, &command).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        tokio::time::sleep(Duration::from_millis(10)).await;

        bus.publish(chat_event("!ping", "user1"));
        tokio::time::sleep(Duration::from_millis(50)).await;

        let second = chat_event("!ping", "user1");
        let second_id = second.id;
        bus.publish(second);

        let blocked = collect_kind(&mut sub, "command.cooldown_blocked", 40)
            .await
            .expect("command.cooldown_blocked must be emitted on second call");

        assert_eq!(
            blocked.caused_by,
            Some(second_id),
            "cooldown_blocked must be caused by the blocking chat.message"
        );
        assert_eq!(
            blocked.payload["command"].as_str().unwrap(),
            "!ping",
            "command field must match the normalized command name"
        );
        assert!(
            blocked.payload["cooldown_remaining_ms"].as_u64().unwrap() > 0,
            "cooldown_remaining_ms must be positive"
        );
        assert_eq!(
            blocked.payload["user_login"].as_str().unwrap(),
            "user1",
            "user_login must be propagated from chat.message payload"
        );
    }

    #[tokio::test]
    async fn full_causation_chain_integrity() {
        use forge_types::EventId;
        use std::collections::HashMap;

        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let c_id = CommandId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = Action {
            id: a_id,
            name: "chain-action".to_string(),
            group: None,
            queue_id: q_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            description: None,
            sub_actions: vec![SubActionSpec::SendChat {
                message: "hello from chain".to_string(),
                target: "twitch".to_string(),
            }],
        };
        let command = make_command(c_id, a_id, "!chain", 0);
        seed(&dp, &queue, &action, &command).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        tokio::time::sleep(Duration::from_millis(10)).await;

        let chat = chat_event("!chain", "viewer1");
        let chat_id: EventId = chat.id;
        bus.publish(chat);

        let expected_kinds = [
            "chat.message",
            "command.matched",
            "action.start",
            "subaction.run",
            "chat.send.request",
            "action.done",
        ];
        let mut by_kind: HashMap<String, forge_events::Event> = HashMap::new();
        let mut seen = 0usize;

        while let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_millis(500), sub.recv()).await {
            let kind = ev.kind.clone();
            if expected_kinds.contains(&kind.as_str()) {
                by_kind.insert(kind, ev);
                seen += 1;
                if seen >= expected_kinds.len() {
                    break;
                }
            }
        }

        let chat_ev = by_kind
            .get("chat.message")
            .expect("chat.message not received");
        assert_eq!(chat_ev.id, chat_id);
        assert_eq!(chat_ev.caused_by, None, "chat.message: no parent");

        let cmd_ev = by_kind
            .get("command.matched")
            .expect("command.matched not received");
        let cmd_id: EventId = cmd_ev.id;
        assert_eq!(
            cmd_ev.caused_by,
            Some(chat_id),
            "command.matched must be caused by chat.message"
        );

        let start_ev = by_kind
            .get("action.start")
            .expect("action.start not received");
        let start_id: EventId = start_ev.id;
        assert_eq!(
            start_ev.caused_by,
            Some(cmd_id),
            "action.start must be caused by command.matched"
        );

        let run_ev = by_kind
            .get("subaction.run")
            .expect("subaction.run not received");
        let run_id: EventId = run_ev.id;
        assert_eq!(
            run_ev.caused_by,
            Some(start_id),
            "subaction.run must be caused by action.start"
        );

        let send_ev = by_kind
            .get("chat.send.request")
            .expect("chat.send.request not received");
        assert_eq!(
            send_ev.caused_by,
            Some(run_id),
            "chat.send.request must be caused by subaction.run"
        );

        let done_ev = by_kind
            .get("action.done")
            .expect("action.done not received");
        assert_eq!(
            done_ev.caused_by,
            Some(start_id),
            "action.done must be caused by action.start"
        );
    }
}
