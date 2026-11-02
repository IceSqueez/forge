#![expect(clippy::unwrap_used, reason = "test fixtures")]
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_overlay::{ChatAvatarSlot, RowAvatar};
use forge_platform_core::{AvatarLookup, ViewerAvatarSource};
use forge_runtime::{ChatAvatars, KNOWN_AVATARS_CAPACITY, KnownAvatar, WAITING_AVATARS_CAPACITY};
use forge_types::PlatformId;
use tokio::sync::Notify;

const IMAGE_BASE: &str = "https://static-cdn.jtvnw.net/jtv_user_pictures/";

#[derive(Clone, Copy)]
enum Answer {
    Images,
    Unavailable,
    Hangs,
}

struct FakeSource {
    limit: usize,
    answer: Answer,
    images: HashMap<String, String>,
    remembering: AtomicBool,
    asked: Mutex<Vec<Vec<String>>>,
    gate: Option<Arc<Notify>>,
    entered: Arc<Notify>,
}

impl FakeSource {
    fn new(limit: usize, answer: Answer) -> Self {
        Self {
            limit,
            answer,
            images: HashMap::new(),
            remembering: AtomicBool::new(false),
            asked: Mutex::new(Vec::new()),
            gate: None,
            entered: Arc::new(Notify::new()),
        }
    }

    fn with_image(mut self, user_id: &str, url: &str) -> Self {
        self.images.insert(user_id.to_owned(), url.to_owned());
        self
    }

    fn remembering(self) -> Self {
        self.remembering.store(true, Ordering::SeqCst);
        self
    }

    fn gated(mut self, gate: Arc<Notify>) -> Self {
        self.gate = Some(gate);
        self
    }

    fn asked(&self) -> Vec<Vec<String>> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait]
impl ViewerAvatarSource for FakeSource {
    fn remembered(&self, viewer_id: &str) -> Option<String> {
        self.remembering
            .load(Ordering::SeqCst)
            .then(|| image_of(viewer_id))
    }

    fn batch_limit(&self) -> usize {
        self.limit
    }

    async fn avatars(&self, viewer_ids: &[String]) -> AvatarLookup {
        self.asked.lock().unwrap().push(viewer_ids.to_vec());
        self.entered.notify_one();
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        match self.answer {
            Answer::Images => AvatarLookup::Answered(
                viewer_ids
                    .iter()
                    .filter_map(|id| Some((id.clone(), self.images.get(id)?.clone())))
                    .collect(),
            ),
            Answer::Unavailable => AvatarLookup::Unavailable,
            Answer::Hangs => std::future::pending().await,
        }
    }
}

fn image_of(user_id: &str) -> String {
    format!("{IMAGE_BASE}{user_id}.png")
}

fn twitch(user_id: &str) -> ChatAvatarSlot {
    ChatAvatarSlot {
        platform: PlatformId::Twitch,
        user_id: user_id.to_owned(),
    }
}

fn kick(user_id: &str) -> ChatAvatarSlot {
    ChatAvatarSlot {
        platform: PlatformId::Kick,
        user_id: user_id.to_owned(),
    }
}

fn with_twitch(source: FakeSource) -> (ChatAvatars, Arc<FakeSource>) {
    let source = Arc::new(source);
    let avatars = ChatAvatars::default();
    avatars.register(
        PlatformId::Twitch,
        Arc::clone(&source) as Arc<dyn ViewerAvatarSource>,
    );
    (avatars, source)
}

async fn drain(avatars: &ChatAvatars, platform: PlatformId) -> Vec<Vec<RowAvatar>> {
    let mut batches = Vec::new();
    while let Some(fills) = avatars.resolve_batch(platform).await {
        batches.push(fills);
    }
    batches
}

#[tokio::test]
async fn an_answered_image_is_filled_and_remembered() {
    let url = image_of("42");
    let (avatars, _) = with_twitch(FakeSource::new(100, Answer::Images).with_image("42", &url));
    avatars.ask(twitch("42"));

    let fills = avatars.resolve_batch(PlatformId::Twitch).await;

    assert_eq!(
        fills,
        Some(vec![RowAvatar {
            slot: twitch("42"),
            url: url.clone(),
        }])
    );
    assert_eq!(avatars.known(&twitch("42")), KnownAvatar::Image(url));
}

#[tokio::test]
async fn only_the_first_ask_while_nothing_drains_starts_a_drain() {
    let (avatars, _) = with_twitch(FakeSource::new(100, Answer::Images));

    let first = avatars.ask(twitch("1"));
    let second = avatars.ask(twitch("2"));
    drain(&avatars, PlatformId::Twitch).await;
    let after_drain = avatars.ask(twitch("3"));

    assert_eq!((first, second, after_drain), (true, false, true));
}

#[tokio::test]
async fn asking_for_the_same_viewer_twice_looks_them_up_once() {
    let (avatars, source) = with_twitch(FakeSource::new(100, Answer::Images));

    avatars.ask(twitch("42"));
    avatars.ask(twitch("42"));
    drain(&avatars, PlatformId::Twitch).await;

    assert_eq!(source.asked(), vec![vec!["42".to_owned()]]);
}

#[tokio::test]
async fn a_viewer_asked_again_while_their_lookup_is_in_flight_is_not_queued_twice() {
    let gate = Arc::new(Notify::new());
    let (avatars, source) =
        with_twitch(FakeSource::new(100, Answer::Images).gated(Arc::clone(&gate)));
    avatars.ask(twitch("42"));
    let in_flight = {
        let avatars = avatars.clone();
        tokio::spawn(async move { avatars.resolve_batch(PlatformId::Twitch).await })
    };
    source.entered.notified().await;

    avatars.ask(twitch("42"));
    gate.notify_one();
    in_flight.await.unwrap();

    assert_eq!(avatars.resolve_batch(PlatformId::Twitch).await, None);
    assert_eq!(source.asked().len(), 1);
}

#[tokio::test]
async fn the_waiting_queue_holds_its_capacity_and_drops_the_viewer_past_it() {
    let (avatars, source) = with_twitch(FakeSource::new(usize::MAX, Answer::Images));
    let past_cap = format!("{WAITING_AVATARS_CAPACITY:05}");

    for index in 0..WAITING_AVATARS_CAPACITY {
        avatars.ask(twitch(&format!("{index:05}")));
    }
    avatars.ask(twitch(&past_cap));
    drain(&avatars, PlatformId::Twitch).await;

    let asked = source.asked().concat();
    assert_eq!(asked.len(), WAITING_AVATARS_CAPACITY);
    assert!(!asked.contains(&past_cap));
}

#[tokio::test]
async fn a_full_queue_on_one_platform_does_not_refuse_another_platform() {
    let (avatars, _) = with_twitch(FakeSource::new(usize::MAX, Answer::Images));
    for index in 0..=WAITING_AVATARS_CAPACITY {
        avatars.ask(twitch(&index.to_string()));
    }

    assert!(
        avatars.ask(kick("7")),
        "a full Twitch queue refused a Kick viewer"
    );
}

#[tokio::test]
async fn each_lookup_asks_no_more_viewers_than_the_source_takes_at_once() {
    let (avatars, source) = with_twitch(FakeSource::new(2, Answer::Images));
    for user_id in ["1", "2", "3", "4", "5"] {
        avatars.ask(twitch(user_id));
    }

    drain(&avatars, PlatformId::Twitch).await;

    let sizes: Vec<usize> = source.asked().iter().map(Vec::len).collect();
    assert_eq!(sizes, vec![2, 2, 1]);
}

#[tokio::test]
async fn a_viewer_answered_without_a_usable_image_is_remembered_as_having_none() {
    for (answered, case) in [
        (None, "no image"),
        (Some("http://evil.example/x.png"), "an off-host image"),
    ] {
        let mut source = FakeSource::new(100, Answer::Images);
        if let Some(url) = answered {
            source = source.with_image("42", url);
        }
        let (avatars, _) = with_twitch(source);
        avatars.ask(twitch("42"));

        let fills = avatars.resolve_batch(PlatformId::Twitch).await;

        assert_eq!(fills, Some(Vec::new()), "{case} was pushed to the page");
        assert_eq!(avatars.known(&twitch("42")), KnownAvatar::NoImage, "{case}");
    }
}

#[tokio::test]
async fn an_unavailable_answer_is_not_remembered() {
    let (avatars, _) = with_twitch(FakeSource::new(100, Answer::Unavailable));
    avatars.ask(twitch("42"));

    avatars.resolve_batch(PlatformId::Twitch).await;

    assert_eq!(avatars.known(&twitch("42")), KnownAvatar::Unknown);
}

#[tokio::test]
async fn an_unavailable_answer_refuses_new_asks_for_that_platform() {
    let (avatars, source) = with_twitch(FakeSource::new(100, Answer::Unavailable));
    avatars.ask(twitch("42"));
    drain(&avatars, PlatformId::Twitch).await;

    let started = avatars.ask(twitch("43"));
    let later = avatars.resolve_batch(PlatformId::Twitch).await;

    assert!(!started);
    assert_eq!(later, None, "a paused platform still queued a viewer");
    assert_eq!(source.asked().len(), 1);
}

#[tokio::test]
async fn an_unavailable_platform_does_not_pause_another_platform() {
    let (avatars, _) = with_twitch(FakeSource::new(100, Answer::Unavailable));
    avatars.ask(twitch("42"));
    drain(&avatars, PlatformId::Twitch).await;

    assert!(avatars.ask(kick("7")));
}

#[tokio::test(start_paused = true)]
async fn a_source_that_never_answers_is_treated_as_unavailable() {
    let (avatars, _) = with_twitch(FakeSource::new(100, Answer::Hangs));
    avatars.ask(twitch("42"));

    let fills = avatars.resolve_batch(PlatformId::Twitch).await;

    assert_eq!(fills, Some(Vec::new()));
    assert_eq!(avatars.known(&twitch("42")), KnownAvatar::Unknown);
    assert!(
        !avatars.ask(twitch("43")),
        "a hung source did not pause lookups"
    );
}

fn remember_viewers(avatars: &ChatAvatars, count: usize) {
    for index in 0..count {
        assert!(matches!(
            avatars.known(&twitch(&index.to_string())),
            KnownAvatar::Image(_)
        ));
    }
}

#[tokio::test]
async fn the_remembered_avatars_keep_the_oldest_viewer_at_capacity() {
    let (avatars, source) = with_twitch(FakeSource::new(100, Answer::Images).remembering());
    remember_viewers(&avatars, KNOWN_AVATARS_CAPACITY);
    source.remembering.store(false, Ordering::SeqCst);

    assert_eq!(
        avatars.known(&twitch("0")),
        KnownAvatar::Image(image_of("0"))
    );
}

#[tokio::test]
async fn one_viewer_past_capacity_evicts_only_the_oldest_remembered_avatar() {
    let (avatars, source) = with_twitch(FakeSource::new(100, Answer::Images).remembering());
    remember_viewers(&avatars, KNOWN_AVATARS_CAPACITY + 1);
    source.remembering.store(false, Ordering::SeqCst);

    assert_eq!(avatars.known(&twitch("0")), KnownAvatar::Unknown);
    assert_eq!(
        avatars.known(&twitch("1")),
        KnownAvatar::Image(image_of("1"))
    );
}

#[tokio::test]
async fn an_avatar_the_source_already_remembers_fills_inline_without_a_lookup() {
    let (avatars, source) = with_twitch(FakeSource::new(100, Answer::Images).remembering());

    let known = avatars.known(&twitch("42"));

    assert_eq!(known, KnownAvatar::Image(image_of("42")));
    assert!(source.asked().is_empty());
}

#[tokio::test]
async fn a_remembered_off_host_avatar_is_not_used() {
    struct OffHost;

    #[async_trait]
    impl ViewerAvatarSource for OffHost {
        fn remembered(&self, _viewer_id: &str) -> Option<String> {
            Some("https://evil.example/x.png".to_owned())
        }

        fn batch_limit(&self) -> usize {
            1
        }

        async fn avatars(&self, _viewer_ids: &[String]) -> AvatarLookup {
            AvatarLookup::Unavailable
        }
    }
    let avatars = ChatAvatars::default();
    avatars.register(PlatformId::Twitch, Arc::new(OffHost));

    assert_eq!(avatars.known(&twitch("42")), KnownAvatar::Unknown);
}

#[tokio::test]
async fn unregistering_a_platform_forgets_the_viewers_waiting_on_it() {
    let (avatars, source) = with_twitch(FakeSource::new(100, Answer::Images));
    avatars.ask(twitch("42"));

    avatars.unregister(PlatformId::Twitch);
    avatars.register(
        PlatformId::Twitch,
        Arc::clone(&source) as Arc<dyn ViewerAvatarSource>,
    );

    assert_eq!(avatars.resolve_batch(PlatformId::Twitch).await, None);
    assert!(
        avatars.ask(twitch("42")),
        "a forgotten viewer could not be asked again"
    );
    assert!(source.asked().is_empty());
}

#[tokio::test]
async fn a_drain_for_a_platform_without_a_source_ends_and_forgets_its_queue() {
    let avatars = ChatAvatars::default();
    avatars.ask(twitch("42"));

    let drained = avatars.resolve_batch(PlatformId::Twitch).await;

    assert_eq!(drained, None);
    assert!(avatars.ask(twitch("42")));
}
