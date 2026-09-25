#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_overlay::config::{SHOW, SPEECH, SPEECH_VOICE};
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_registry::CancelSignal;
use forge_runtime::{
    EventBus, NullEventLogRepo, OverlayDispatch, OverlayFrameSink, OverlayReceivers,
    OverlayServiceHandle, SHOW_CEILING, SHOW_SPEECH_START_WAIT, ShowEnd, ShowSpeech, ShowTicket,
    SpeakDispatchError, SpeakDispatcher, SpeechStartSignal,
};
use forge_storage::settings::MockSettingsRepo;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo,
    SettingsRepo,
};
use forge_types::{ArgStack, Variant};
use time::OffsetDateTime;
use tokio::time::Instant;

const ALERT_KIND: &str = "overlay.alert";
const CHAT_KIND: &str = "overlay.chat";
const GOAL_KIND: &str = "overlay.goal";
const STAGE: &str = "stage-alert";

const HEADLINE_KEY: &str = "headline";
const COMMAND_KEY: &str = "command";
const REVEAL: &str = "reveal";
const ALERT_WINDOW: Duration = Duration::from_secs(5);
const NEVER: Duration = Duration::from_secs(24 * 60 * 60);
const CANCEL_POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
struct Frame {
    content: serde_json::Value,
    at: Duration,
}

struct TimedPage {
    started: Instant,
    frames: Mutex<Vec<Frame>>,
}

impl TimedPage {
    fn frames(&self) -> Vec<Frame> {
        self.frames.lock().unwrap().clone()
    }

    fn arrivals(&self) -> Vec<Duration> {
        self.frames()
            .into_iter()
            .filter(|frame| frame.content.get(HEADLINE_KEY).is_some())
            .map(|frame| frame.at)
            .collect()
    }

    fn reveals(&self) -> Vec<Frame> {
        self.frames()
            .into_iter()
            .filter(|frame| frame.content[COMMAND_KEY].as_str() == Some(REVEAL))
            .collect()
    }
}

#[async_trait]
impl OverlayFrameSink for TimedPage {
    async fn deliver_content(
        &self,
        _: &OverlayId,
        content: serde_json::Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        self.frames.lock().unwrap().push(Frame {
            content,
            at: self.started.elapsed(),
        });
        OverlayReceivers {
            sources: 1,
            preview_tabs: 0,
        }
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}
}

#[derive(Debug, Clone, Copy)]
enum Speaks {
    Starts { after: Duration, lasts: Duration },
    FailsAtOnce,
}

fn plays_for(lasts: Duration) -> Speaks {
    Speaks::Starts {
        after: Duration::ZERO,
        lasts,
    }
}

const NEVER_STARTS: Speaks = Speaks::Starts {
    after: NEVER,
    lasts: NEVER,
};

#[derive(Debug, Clone)]
struct Spoken {
    speech: ShowSpeech,
    at: Duration,
    cancelled: bool,
}

struct ScriptedSpeaker {
    started: Instant,
    speaks: Speaks,
    spoken: Mutex<Vec<Spoken>>,
}

impl ScriptedSpeaker {
    fn spoken(&self) -> Vec<Spoken> {
        self.spoken.lock().unwrap().clone()
    }
}

#[async_trait]
impl SpeakDispatcher for ScriptedSpeaker {
    async fn speak(&self, _: String, _: Option<String>) -> Result<(), SpeakDispatchError> {
        panic!("show speech took the global speak path")
    }

    async fn speak_for_show(
        &self,
        speech: ShowSpeech,
        cancel: CancelSignal,
        started: SpeechStartSignal,
    ) -> Result<(), SpeakDispatchError> {
        let index = {
            let mut spoken = self.spoken.lock().unwrap();
            spoken.push(Spoken {
                speech,
                at: self.started.elapsed(),
                cancelled: false,
            });
            spoken.len() - 1
        };
        let (after, lasts) = match self.speaks {
            Speaks::FailsAtOnce => {
                return Err(SpeakDispatchError::Dispatch("rejected by a filter".into()));
            }
            Speaks::Starts { after, lasts } => (after, lasts),
        };
        self.until(index, after, &cancel).await?;
        started.mark_started();
        self.until(index, lasts, &cancel).await
    }
}

impl ScriptedSpeaker {
    async fn until(
        &self,
        index: usize,
        span: Duration,
        cancel: &CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let ends = Instant::now() + span;
        while Instant::now() < ends {
            if cancel.is_cancelled() {
                self.spoken.lock().unwrap()[index].cancelled = true;
                return Err(SpeakDispatchError::Dispatch("cancelled".into()));
            }
            tokio::time::sleep(CANCEL_POLL).await;
        }
        Ok(())
    }
}

fn definition(kind_id: &str, voice: &str) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(STAGE),
        display_name: STAGE.to_owned(),
        kind_id: kind_id.to_owned(),
        enabled: true,
        position: 0,
        config: OverlayConfig::from([(SPEECH_VOICE.to_owned(), Variant::String(voice.to_owned()))]),
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

struct Harness {
    page: Arc<TimedPage>,
    speaker: Arc<ScriptedSpeaker>,
    service: OverlayServiceHandle,
}

fn harness(kind_id: &str, speaks: Speaks) -> Harness {
    let stored = definition(kind_id, "  amy  ");
    let mut repo = MockOverlayRepo::new();
    repo.expect_get()
        .returning(move |id| Ok((id == &stored.id).then(|| stored.clone())));
    repo.expect_set_retained_content().returning(|_, _| Ok(()));
    repo.expect_get_retained_content().returning(|_| Ok(None));
    let mut settings = MockSettingsRepo::new();
    settings.expect_get_string().returning(|_| Ok(None));

    let mut kinds = OverlayKindRegistry::new();
    register_builtin_kinds(&mut kinds).expect("the builtin overlay kinds register");
    let page = Arc::new(TimedPage {
        started: Instant::now(),
        frames: Mutex::new(Vec::new()),
    });
    let speaker = Arc::new(ScriptedSpeaker {
        started: Instant::now(),
        speaks,
        spoken: Mutex::new(Vec::new()),
    });

    let service = OverlayServiceHandle::new(
        Arc::new(repo) as Arc<dyn OverlayRepo>,
        Arc::new(settings) as Arc<dyn SettingsRepo>,
        Arc::new(kinds),
        EventBus::new(Arc::new(NullEventLogRepo)),
        Some(Arc::clone(&page) as Arc<dyn OverlayFrameSink>),
    )
    .with_speech(Arc::clone(&speaker) as Arc<dyn SpeakDispatcher>);
    Harness {
        page,
        speaker,
        service,
    }
}

fn spoken_show(text: &str) -> OverlayConfig {
    OverlayConfig::from([
        (HEADLINE_KEY.to_owned(), Variant::String(text.to_owned())),
        (SPEECH.to_owned(), Variant::String(format!("say {text}"))),
    ])
}

impl Harness {
    async fn dispatch(&self, text: &str) -> OverlayDispatch {
        self.service
            .send_to(
                &OverlayId::new(STAGE),
                &spoken_show(text),
                &ArgStack::new(),
                None,
            )
            .await
            .expect("a stored overlay of a shipped look accepts a send")
    }

    async fn queue(&self, text: &str) -> ShowTicket {
        match self.dispatch(text).await {
            OverlayDispatch::Queued(ticket) => ticket,
            OverlayDispatch::Applied(delivery) => {
                panic!("a transient show was applied at once ({delivery:?}) instead of queued")
            }
        }
    }

    async fn until_spoken(&self, count: usize) {
        for _ in 0..1_000 {
            if self.speaker.spoken().len() >= count {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("speech {count} was never requested");
    }
}

async fn finished(ticket: ShowTicket) -> ShowEnd {
    tokio::time::timeout(NEVER, ticket.finished())
        .await
        .expect("a queued show never ended, so its presenter is stuck or gone")
}

#[tokio::test(start_paused = true)]
async fn a_waiting_shows_speech_is_requested_only_when_that_show_reaches_the_page() {
    let harness = harness(ALERT_KIND, plays_for(Duration::from_secs(1)));

    let first = harness.queue("first").await;
    let second = harness.queue("second").await;
    finished(first).await;
    finished(second).await;

    let requested: Vec<(String, Duration)> = harness
        .speaker
        .spoken()
        .into_iter()
        .map(|spoken| (spoken.speech.text, spoken.at))
        .collect();
    assert_eq!(
        requested,
        vec![
            ("say first".to_owned(), Duration::ZERO),
            ("say second".to_owned(), ALERT_WINDOW),
        ],
        "a queued show's speech was read before its show was on the page"
    );
}

#[tokio::test(start_paused = true)]
async fn a_started_show_is_held_for_its_window_or_speech_counted_from_the_speech_start() {
    let secs = Duration::from_secs;
    for (starts_after, lasts, next_show_at) in [
        (Duration::ZERO, secs(2), ALERT_WINDOW),
        (Duration::ZERO, secs(9), secs(9)),
        (secs(3), secs(1), secs(3) + ALERT_WINDOW),
        (secs(3), secs(9), secs(12)),
        (secs(9), secs(1), secs(9) + ALERT_WINDOW),
    ] {
        let harness = harness(
            ALERT_KIND,
            Speaks::Starts {
                after: starts_after,
                lasts,
            },
        );

        let head = harness.queue("head").await;
        let next = harness.queue("next").await;
        finished(head).await;
        finished(next).await;

        assert_eq!(
            (harness.page.arrivals()[1], harness.page.reveals().len()),
            (next_show_at, 0),
            "speech starting after {starts_after:?} and lasting {lasts:?} ended the show at the \
             wrong moment, or the show was also revealed silently"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn speech_still_playing_at_the_ceiling_is_cancelled_and_the_show_ends_there() {
    let harness = harness(
        ALERT_KIND,
        Speaks::Starts {
            after: Duration::from_secs(5),
            lasts: NEVER,
        },
    );

    let head = harness.queue("head").await;
    let next = harness.queue("next").await;
    finished(head).await;
    harness.until_spoken(2).await;

    let first = &harness.speaker.spoken()[0];
    assert_eq!(
        (harness.page.arrivals()[1], first.cancelled),
        (SHOW_CEILING, true),
        "the ceiling was not counted from the head of the show, or the speech was not cut there"
    );
    drop(next);
}

#[tokio::test(start_paused = true)]
async fn a_show_whose_speech_never_starts_is_revealed_silently_for_its_whole_window() {
    for (speaks, revealed_at) in [
        (Speaks::FailsAtOnce, Duration::ZERO),
        (NEVER_STARTS, SHOW_SPEECH_START_WAIT),
    ] {
        let harness = harness(ALERT_KIND, speaks);

        let head = harness.queue("head").await;
        let next = harness.queue("next").await;
        finished(head).await;
        finished(next).await;

        let frames = harness.page.frames();
        let next_at = frames
            .iter()
            .rposition(|frame| frame.content.get(HEADLINE_KEY).is_some())
            .expect("the next show reached the page");
        let reveals: Vec<Duration> = frames[..next_at]
            .iter()
            .filter(|frame| frame.content[COMMAND_KEY].as_str() == Some(REVEAL))
            .map(|frame| frame.at)
            .collect();
        assert_eq!(
            (reveals, frames[next_at].at),
            (vec![revealed_at], revealed_at + ALERT_WINDOW),
            "{speaks:?}: the page was not told to reveal the show once, when its speech was given \
             up, with the whole window after it"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn speech_not_started_within_the_wait_is_withdrawn_and_the_reveal_names_its_show() {
    let harness = harness(ALERT_KIND, NEVER_STARTS);

    let head = harness.queue("head").await;
    finished(head).await;

    let spoken = harness.speaker.spoken().remove(0);
    let reveal = harness.page.reveals().remove(0);
    assert_eq!(
        (spoken.cancelled, reveal.content[SHOW].as_str()),
        (true, Some(spoken.speech.show.as_str())),
        "speech that never began (a paused or backed-up queue) was left queued, or the reveal \
         frame named another show"
    );
}

#[tokio::test(start_paused = true)]
async fn the_show_frame_carries_the_same_token_as_its_speech() {
    let harness = harness(ALERT_KIND, plays_for(Duration::from_secs(1)));

    finished(harness.queue("head").await).await;

    let spoken = harness.speaker.spoken().remove(0).speech;
    let frame = harness.page.frames().remove(0);
    assert_eq!(
        (
            frame.content[SHOW].as_str(),
            spoken.overlay.as_str(),
            spoken.voice_alias.as_deref(),
        ),
        (Some(spoken.show.as_str()), STAGE, Some("amy")),
        "the show and its speech cannot be joined on the page, or the speech targets the wrong \
         overlay or voice"
    );
}

#[tokio::test(start_paused = true)]
async fn speech_text_never_reaches_the_page_for_any_look() {
    for kind in [ALERT_KIND, CHAT_KIND, GOAL_KIND] {
        let harness = harness(kind, plays_for(Duration::from_secs(1)));

        if let OverlayDispatch::Queued(ticket) = harness.dispatch("secret").await {
            finished(ticket).await;
        }
        let fired = harness
            .service
            .test_fire(&OverlayId::new(STAGE))
            .await
            .expect("every shipped look can be test-fired");

        let frames = harness.page.frames();
        assert!(
            frames.len() == 2
                && frames
                    .iter()
                    .all(|frame| frame.content.get(SPEECH).is_none())
                && !fired.content.contains_key(SPEECH),
            "a {kind} page received the speech text: {frames:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_look_that_applies_on_arrival_speaks_at_once_without_holding_the_page() {
    for kind in [CHAT_KIND, GOAL_KIND] {
        let harness = harness(kind, plays_for(NEVER));

        let dispatch = harness.dispatch("line").await;
        harness.until_spoken(1).await;

        assert!(
            matches!(dispatch, OverlayDispatch::Applied(_))
                && harness.speaker.spoken()[0].at == Duration::ZERO,
            "a {kind} send was held for its speech, or its speech was never requested"
        );
    }
}
