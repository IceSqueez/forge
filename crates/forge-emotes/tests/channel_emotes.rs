#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
use std::ffi::OsString;
use std::time::Duration;

use forge_emotes::endpoint_surfaces::ENDPOINT_SURFACES;
use forge_emotes::{ChannelEmotes, FAILED_SOURCE_RETRY, REFRESH_INTERVAL};
use forge_platform_core::PlatformEndpoints;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BROADCASTER: &str = "12826";
const HANG_GUARD: Duration = Duration::from_secs(20);
const POLL_STEP: Duration = Duration::from_millis(5);
const FIRST_ANSWER: u8 = 1;
const LATER_ANSWERS: u8 = 2;
const USUAL_ANSWER: u8 = 3;

const SEVEN_TV_CHANNEL: &str = "/users/twitch/12826";
const SEVEN_TV_GLOBAL: &str = "/emote-sets/global";
const BETTER_TTV_CHANNEL: &str = "/cached/users/twitch/12826";
const BETTER_TTV_GLOBAL: &str = "/cached/emotes/global";
const FRANKER_FACE_Z_CHANNEL: &str = "/room/id/12826";
const FRANKER_FACE_Z_GLOBAL: &str = "/set/global";

fn seven_tv_set(code: &str) -> Value {
    json!({
        "id": "62cdd34e72a832540de95857",
        "name": "Set",
        "emotes": [{
            "id": "60ae958e229664e8667aea38",
            "name": code,
            "data": {
                "id": "60ae958e229664e8667aea38",
                "name": code,
                "host": {
                    "url": "//cdn.7tv.app/emote/60ae958e229664e8667aea38",
                    "files": [{ "name": "1x.webp" }, { "name": "2x.webp" }],
                },
            },
        }],
    })
}

fn seven_tv_user(code: &str) -> Value {
    json!({ "id": "01FCJ7QDJ80006EHDKGM5M9T6V", "platform": "TWITCH", "emote_set": seven_tv_set(code) })
}

fn better_ttv_list(code: &str) -> Value {
    json!([{ "id": "54fa925e01e468494b85b54d", "code": code, "imageType": "png", "animated": false }])
}

fn better_ttv_user(code: &str) -> Value {
    json!({ "id": "5f1b2d3c4a5e6f7a8b9c0d1e", "bots": [], "channelEmotes": better_ttv_list(code), "sharedEmotes": [] })
}

fn franker_face_z_set(code: &str) -> Value {
    json!({ "id": 3, "emoticons": [{ "id": 25927, "name": code, "urls": { "2": "https://cdn.frankerfacez.com/emote/25927/2" } }] })
}

fn franker_face_z_global(code: &str) -> Value {
    json!({ "default_sets": [3], "sets": { "3": franker_face_z_set(code) } })
}

fn franker_face_z_room(code: &str) -> Value {
    json!({ "room": { "_id": 1, "twitch_id": 12826, "set": 3 }, "sets": { "3": franker_face_z_set(code) } })
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

async fn instead(server: &MockServer, route: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(response)
        .with_priority(LATER_ANSWERS)
        .mount(server)
        .await;
}

async fn usual(server: &MockServer, route: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(response)
        .with_priority(USUAL_ANSWER)
        .mount(server)
        .await;
}

async fn first(server: &MockServer, route: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(response)
        .up_to_n_times(1)
        .with_priority(FIRST_ANSWER)
        .mount(server)
        .await;
}

async fn every_layer_answers(server: &MockServer) {
    usual(server, SEVEN_TV_CHANNEL, ok(seven_tv_user("SevenChannel"))).await;
    usual(server, SEVEN_TV_GLOBAL, ok(seven_tv_set("SevenGlobal"))).await;
    usual(
        server,
        BETTER_TTV_CHANNEL,
        ok(better_ttv_user("BetterChannel")),
    )
    .await;
    usual(
        server,
        BETTER_TTV_GLOBAL,
        ok(better_ttv_list("BetterGlobal")),
    )
    .await;
    usual(
        server,
        FRANKER_FACE_Z_CHANNEL,
        ok(franker_face_z_room("FrankerChannel")),
    )
    .await;
    usual(
        server,
        FRANKER_FACE_Z_GLOBAL,
        ok(franker_face_z_global("FrankerGlobal")),
    )
    .await;
}

fn emotes_on(server: &MockServer) -> ChannelEmotes {
    let uri = server.uri();
    let endpoints =
        PlatformEndpoints::resolve(ENDPOINT_SURFACES, |_| Some(OsString::from(uri.clone())))
            .unwrap();
    ChannelEmotes::new(&endpoints).unwrap()
}

fn has(emotes: &ChannelEmotes, code: &str) -> bool {
    emotes.book().emote_image(code).is_some()
}

async fn until(what: &str, condition: impl Fn() -> bool) {
    let waited = tokio::time::timeout(HANG_GUARD, async {
        while !condition() {
            tokio::time::sleep(POLL_STEP).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "never saw {what}");
}

async fn travel(by: Duration) {
    tokio::time::pause();
    tokio::time::advance(by).await;
    tokio::time::resume();
}

async fn asked(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == route)
        .count()
}

fn follow(emotes: &ChannelEmotes) -> JoinHandle<()> {
    tokio::spawn(emotes.follow(BROADCASTER.to_owned()))
}

#[tokio::test]
async fn the_catalog_carries_every_providers_channel_and_global_emotes() {
    let server = MockServer::start().await;
    every_layer_answers(&server).await;
    let emotes = emotes_on(&server);
    let task = follow(&emotes);

    until("the first catalog", || emotes.shown().is_some()).await;
    task.abort();

    let codes = [
        "SevenChannel",
        "SevenGlobal",
        "BetterChannel",
        "BetterGlobal",
        "FrankerChannel",
        "FrankerGlobal",
    ];
    let missing: Vec<&str> = codes
        .into_iter()
        .filter(|code| !has(&emotes, code))
        .collect();
    assert!(missing.is_empty(), "missing {missing:?}");
}

#[tokio::test]
async fn a_shown_emote_image_points_at_the_provider_cdn() {
    let server = MockServer::start().await;
    every_layer_answers(&server).await;
    let emotes = emotes_on(&server);
    let task = follow(&emotes);

    until("the first catalog", || emotes.shown().is_some()).await;
    task.abort();

    assert_eq!(
        emotes.book().emote_image("BetterGlobal").as_deref(),
        Some("https://cdn.betterttv.net/emote/54fa925e01e468494b85b54d/2x")
    );
}

#[tokio::test]
async fn a_channel_unknown_to_a_provider_is_an_empty_layer_not_retried_before_the_refresh() {
    let server = MockServer::start().await;
    instead(&server, SEVEN_TV_CHANNEL, ResponseTemplate::new(404)).await;
    first(
        &server,
        BETTER_TTV_CHANNEL,
        ok(better_ttv_user("BetterChannel")),
    )
    .await;
    instead(
        &server,
        BETTER_TTV_CHANNEL,
        ok(better_ttv_user("Refreshed")),
    )
    .await;
    every_layer_answers(&server).await;
    let emotes = emotes_on(&server);
    let task = follow(&emotes);
    until("the first catalog", || emotes.shown().is_some()).await;
    let first_catalog_had_globals = has(&emotes, "SevenGlobal") && !has(&emotes, "SevenChannel");

    travel(FAILED_SOURCE_RETRY).await;
    travel(REFRESH_INTERVAL - FAILED_SOURCE_RETRY).await;
    until("the refreshed catalog", || has(&emotes, "Refreshed")).await;
    task.abort();

    assert!(first_catalog_had_globals);
    assert_eq!(asked(&server, SEVEN_TV_CHANNEL).await, 2);
}

#[tokio::test]
async fn a_failing_layer_is_asked_again_after_the_failure_pause_while_the_others_stay_shown() {
    for failure in [
        ResponseTemplate::new(500),
        ResponseTemplate::new(503),
        ResponseTemplate::new(200).set_body_string("<html>maintenance</html>"),
    ] {
        let server = MockServer::start().await;
        first(&server, SEVEN_TV_CHANNEL, failure).await;
        every_layer_answers(&server).await;
        let emotes = emotes_on(&server);
        let task = follow(&emotes);
        until("the first catalog", || emotes.shown().is_some()).await;
        let first_catalog = (has(&emotes, "BetterChannel"), has(&emotes, "SevenChannel"));

        travel(FAILED_SOURCE_RETRY).await;
        until("the retried layer", || has(&emotes, "SevenChannel")).await;
        task.abort();

        assert_eq!(first_catalog, (true, false));
        assert_eq!(
            asked(&server, BETTER_TTV_CHANNEL).await,
            1,
            "a layer that answered was asked again on the failure retry"
        );
    }
}

#[tokio::test]
async fn a_layer_failing_on_a_refresh_keeps_its_last_good_list() {
    let server = MockServer::start().await;
    first(&server, SEVEN_TV_CHANNEL, ok(seven_tv_user("SevenChannel"))).await;
    instead(&server, SEVEN_TV_CHANNEL, ResponseTemplate::new(500)).await;
    first(
        &server,
        BETTER_TTV_CHANNEL,
        ok(better_ttv_user("BetterChannel")),
    )
    .await;
    instead(
        &server,
        BETTER_TTV_CHANNEL,
        ok(better_ttv_user("Refreshed")),
    )
    .await;
    every_layer_answers(&server).await;
    let emotes = emotes_on(&server);
    let task = follow(&emotes);
    until("the first catalog", || has(&emotes, "SevenChannel")).await;

    travel(REFRESH_INTERVAL).await;
    until("the refreshed catalog", || has(&emotes, "Refreshed")).await;
    task.abort();

    assert!(has(&emotes, "SevenChannel"));
}

#[tokio::test]
async fn a_refresh_replaces_an_answered_layer_with_its_new_list() {
    let server = MockServer::start().await;
    first(
        &server,
        BETTER_TTV_CHANNEL,
        ok(better_ttv_user("BetterChannel")),
    )
    .await;
    instead(
        &server,
        BETTER_TTV_CHANNEL,
        ok(better_ttv_user("Refreshed")),
    )
    .await;
    every_layer_answers(&server).await;
    let emotes = emotes_on(&server);
    let task = follow(&emotes);
    until("the first catalog", || has(&emotes, "BetterChannel")).await;

    travel(REFRESH_INTERVAL).await;
    until("the refreshed catalog", || has(&emotes, "Refreshed")).await;
    task.abort();

    assert!(!has(&emotes, "BetterChannel"));
}

#[tokio::test]
async fn a_broadcaster_id_that_is_not_a_twitch_user_id_never_reaches_a_provider() {
    let server = MockServer::start().await;
    every_layer_answers(&server).await;
    let emotes = emotes_on(&server);
    let past_cap = "1".repeat(33);

    for id in [
        "",
        "12a",
        "../12826",
        "12826/../1",
        "%2e%2e",
        "-1",
        " 12826",
        "١٢٣",
        past_cap.as_str(),
    ] {
        let finished = tokio::time::timeout(HANG_GUARD, emotes.follow(id.to_owned())).await;

        assert!(finished.is_ok(), "{id:?} was followed");
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    assert!(emotes.shown().is_none());
}

#[tokio::test]
async fn a_broadcaster_id_at_the_length_cap_is_followed() {
    let server = MockServer::start().await;
    every_layer_answers(&server).await;
    let at_cap = "1".repeat(32);
    let emotes = emotes_on(&server);
    let task = tokio::spawn(emotes.follow(at_cap.clone()));

    until("the first catalog", || emotes.shown().is_some()).await;
    task.abort();

    assert_eq!(asked(&server, &format!("/users/twitch/{at_cap}")).await, 1);
}

#[tokio::test]
async fn forgetting_the_channel_clears_the_shown_catalog() {
    let server = MockServer::start().await;
    every_layer_answers(&server).await;
    let emotes = emotes_on(&server);
    let task = follow(&emotes);
    until("the first catalog", || has(&emotes, "SevenGlobal")).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());

    emotes.forget();

    assert_eq!(
        (emotes.shown(), emotes.book().emote_image("SevenGlobal")),
        (None, None)
    );
}
