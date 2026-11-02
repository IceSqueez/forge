#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::time::Duration;

use forge_emotes::ChannelEmotes;
use forge_emulator::fakes::emotes::FakeEmotes;

mod support;
use support::{endpoints_from, http_status};

const WAIT: Duration = Duration::from_secs(30);
const POLL_STEP: Duration = Duration::from_millis(10);
const BROADCASTER: &str = "12826";

#[tokio::test]
async fn forge_loads_an_empty_emote_catalog_from_the_fake_providers() {
    let fake = FakeEmotes::start().await.unwrap();
    let emotes = ChannelEmotes::new(&endpoints_from(&fake.endpoint_overrides())).unwrap();
    let follower = tokio::spawn(emotes.follow(BROADCASTER.to_owned()));

    let catalog = tokio::time::timeout(WAIT, async {
        loop {
            if let Some(catalog) = emotes.shown() {
                return catalog;
            }
            tokio::time::sleep(POLL_STEP).await;
        }
    })
    .await
    .unwrap();
    follower.abort();

    assert!(catalog.is_empty());
    let mut answered: Vec<(String, u16)> = fake
        .requests()
        .into_iter()
        .map(|request| (request.path, request.status))
        .collect();
    answered.sort();
    assert_eq!(
        answered,
        [
            ("/7tv/v3/emote-sets/global".to_owned(), http_status::OK),
            (
                "/7tv/v3/users/twitch/12826".to_owned(),
                http_status::NOT_FOUND
            ),
            ("/bttv/3/cached/emotes/global".to_owned(), http_status::OK),
            (
                "/bttv/3/cached/users/twitch/12826".to_owned(),
                http_status::NOT_FOUND
            ),
            ("/ffz/v1/room/id/12826".to_owned(), http_status::NOT_FOUND),
            ("/ffz/v1/set/global".to_owned(), http_status::OK),
        ]
    );
    fake.shutdown().await;
}
