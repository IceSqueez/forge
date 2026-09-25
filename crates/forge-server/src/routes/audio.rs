use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

use crate::audio_clips::ClipOutcome;
use crate::routes::overlays::cors_header_value;
use crate::server::AppState;

const CLIP_PATH_PREFIX: &str = "/audio/v1/clip/";
const REPORT_PATH_PREFIX: &str = "/audio/v1/report/";
const CAPABILITY_PATTERN: &str = "{capability}";

const MAX_REPORT_BODY_BYTES: usize = 2 * 1024;
const MAX_REASON_CHARS: usize = 200;
const NO_STORE: &str = "no-store";

pub(crate) fn clip_path(capability: &str) -> String {
    format!("{CLIP_PATH_PREFIX}{capability}")
}

pub(crate) fn report_path(capability: &str) -> String {
    format!("{REPORT_PATH_PREFIX}{capability}")
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{CLIP_PATH_PREFIX}{CAPABILITY_PATTERN}"),
            get(serve_clip),
        )
        .route(
            &format!("{REPORT_PATH_PREFIX}{CAPABILITY_PATTERN}"),
            post(accept_report).layer(DefaultBodyLimit::max(MAX_REPORT_BODY_BYTES)),
        )
}

async fn serve_clip(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(capability): Path<String>,
) -> Response {
    let Some(payload) = state.audio_clips.take_clip(&capability) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut response_headers = cross_origin_headers(&state, &headers);
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(payload.media_type.as_str()),
    );
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(NO_STORE));

    (StatusCode::OK, response_headers, Body::from(payload.bytes)).into_response()
}

#[derive(Deserialize)]
#[serde(tag = "verdict", rename_all = "camelCase")]
enum VerdictBody {
    Played,
    Refused {
        #[serde(default)]
        reason: String,
    },
}

async fn accept_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(capability): Path<String>,
    Json(body): Json<VerdictBody>,
) -> Response {
    let outcome = match body {
        VerdictBody::Played => ClipOutcome::Played,
        VerdictBody::Refused { reason } => ClipOutcome::Refused {
            reason: reason.chars().take(MAX_REASON_CHARS).collect(),
        },
    };

    if !state.audio_clips.record_verdict(&capability, outcome) {
        return StatusCode::NOT_FOUND.into_response();
    }

    (
        StatusCode::NO_CONTENT,
        cross_origin_headers(&state, &headers),
    )
        .into_response()
}

fn cross_origin_headers(state: &AppState, request_headers: &HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        cors_header_value(state, request_headers),
    );
    headers.insert(header::VARY, HeaderValue::from_name(header::ORIGIN));
    headers
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::net::SocketAddr;
    use std::path::{Path as FsPath, PathBuf};
    use std::sync::Arc;

    use forge_registry::SubActionRegistry;
    use forge_runtime::{ActionCancelRegistry, EventBus, NullEventLogRepo, spawn_action_engine};
    use forge_storage::{DataProvider, GlobalsRepo, OverlayId, SettingsRepo, UserGlobalsRepo};
    use reqwest::StatusCode;
    use tokio::net::TcpListener;
    use tokio::sync::watch;
    use tracing::Level;

    use super::*;
    use crate::ServerHandle;
    use crate::audio_clips::{ClipMediaType, ClipOffer, ClipOutcomeHandle, ClipTicket};
    use crate::auth::AuthState;
    use crate::bus_adapter::BusAdapter;
    use crate::origin::build_allowed_origins;
    use crate::server::{AppState, serve_on_with_shutdown};
    use crate::server_info::ServerInfo;
    use crate::test_helpers::{MemSettings, log_capture, test_creds, test_dp};

    const OWNER: &str = "audio-player";
    const CLIP_BYTES: &[u8] = b"RIFF\0\0\0\0WAVEfmt ";
    const CLIP_DURATION_MS: u64 = 1_200;
    const BEARER_TOKEN: &str = "a-token-no-audio-request-ever-carries";
    const JSON_MEDIA_TYPE: &str = "application/json";
    const LOOPBACK_ANY_PORT: &str = "127.0.0.1:0";
    const FOREIGN_ORIGIN: &str = "https://evil.example";
    const API_INFO_PATH: &str = "/api/v1/info";
    const OVERLAY_IDENTITY: &str = "probe";
    const OVERLAY_ASSET: &str = "behaviour.js";
    const UNKNOWN_CAPABILITY: &str = "SMNhcGFiaWxpdHlUaGF0V2FzTmV2ZXJNaW50ZWRBdEFsbA";
    const MALFORMED_CAPABILITY: &str = "~~not-base64~~";
    const TRAVERSING_CAPABILITY: &str = "..%2F..%2Fetc%2Fpasswd";
    const SHORT_REASON: &str = "NotAllowedError";
    const MULTIBYTE_CHAR: &str = "\u{1f3a7}";
    const OVERFLOW_CHARS: usize = 40;
    const DATE_HEADER: &str = "date";
    const AUTH_REQUIRED_FOR_READS: bool = true;
    const CORS_MIRRORS_ANY_ORIGIN: bool = true;

    struct Fixture {
        handle: ServerHandle,
        addr: SocketAddr,
    }

    impl Fixture {
        async fn offer(&self) -> (ClipTicket, ClipOutcomeHandle) {
            self.handle
                .offer_audio_clip(
                    &OverlayId::new(OWNER),
                    ClipOffer {
                        bytes: CLIP_BYTES.to_vec(),
                        media_type: ClipMediaType::Wave,
                        duration_ms: CLIP_DURATION_MS,
                    },
                )
                .await
                .expect("offer")
        }

        fn url(&self, path: &str) -> String {
            format!("http://{}{path}", self.addr)
        }

        fn own_origin(&self) -> String {
            format!("http://{}", self.addr)
        }

        async fn fetch(&self, path: &str) -> reqwest::Response {
            self.fetch_as(path, None).await
        }

        async fn fetch_as(&self, path: &str, origin: Option<&str>) -> reqwest::Response {
            let mut request = reqwest::Client::new().get(self.url(path));
            if let Some(origin) = origin {
                request = request.header(reqwest::header::ORIGIN, origin);
            }
            request.send().await.expect("clip request")
        }

        async fn report(&self, path: &str, body: String) -> reqwest::Response {
            self.report_as(path, body, None).await
        }

        async fn report_as(
            &self,
            path: &str,
            body: String,
            origin: Option<&str>,
        ) -> reqwest::Response {
            let mut request = reqwest::Client::new()
                .post(self.url(path))
                .header(reqwest::header::CONTENT_TYPE, JSON_MEDIA_TYPE)
                .body(body);
            if let Some(origin) = origin {
                request = request.header(reqwest::header::ORIGIN, origin);
            }
            request.send().await.expect("report request")
        }
    }

    fn played_body() -> String {
        serde_json::json!({ "verdict": "played" }).to_string()
    }

    fn refused_body(reason: &str) -> String {
        serde_json::json!({ "verdict": "refused", "reason": reason }).to_string()
    }

    fn header_of(response: &reqwest::Response, name: reqwest::header::HeaderName) -> String {
        response
            .headers()
            .get(&name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    }

    fn cross_origin_pair(response: &reqwest::Response) -> (String, String) {
        (
            header_of(response, reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN),
            header_of(response, reqwest::header::VARY),
        )
    }

    fn comparable_headers(response: &reqwest::Response) -> BTreeMap<String, String> {
        response
            .headers()
            .iter()
            .filter(|(name, _)| name.as_str() != DATE_HEADER)
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect()
    }

    async fn serve_default() -> Fixture {
        serve(
            false,
            CORS_MIRRORS_ANY_ORIGIN,
            PathBuf::new(),
            MemSettings::new(),
        )
        .await
    }

    async fn serve(
        auth_required_for_reads: bool,
        overlay_cors_any_origin: bool,
        overlay_root: PathBuf,
        settings: Arc<MemSettings>,
    ) -> Fixture {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        bus_adapter.spawn();
        let dp: Arc<dyn DataProvider> = test_dp();
        let action_engine = Arc::new(spawn_action_engine(
            Arc::clone(&bus),
            forge_runtime::Catalog::new(
                dp.action_repo(),
                dp.trigger_instance_repo(),
                dp.catalog_revision(),
            ),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        ));

        let listener = TcpListener::bind(LOOPBACK_ANY_PORT).await.expect("bind");
        let addr = listener.local_addr().expect("local addr");

        let state = AppState {
            auth: AuthState::for_test(auth_required_for_reads, BEARER_TOKEN),
            bus,
            bus_adapter,
            actions: dp.action_repo(),
            globals: Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            user_globals: Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
            overlays: dp.overlay_repo(),
            credentials: test_creds(),
            settings: settings as Arc<dyn SettingsRepo>,
            server_info: ServerInfo::new(),
            action_engine,
            audio_clips: crate::audio_clips::AudioClipStore::new(),
            overlay_root: Arc::new(overlay_root),
            overlay_cors_any_origin,
            bind_addr: addr,
            allowed_origins: Arc::new(build_allowed_origins(addr, &[])),
        };

        let (run_state_tx, _run_state_rx) = watch::channel(true);
        let (reported_tx, reported_rx) = watch::channel(true);
        let (join, shutdown_tx) = serve_on_with_shutdown(listener, state.clone(), reported_tx);
        let handle = ServerHandle::new(join, shutdown_tx, state, addr, run_state_tx);
        handle.adopt_generation(reported_rx);

        Fixture { handle, addr }
    }

    async fn overlay_root_with_an_asset(root: &FsPath) {
        let dir = root.join(OVERLAY_IDENTITY);
        tokio::fs::create_dir_all(&dir).await.expect("overlay dir");
        tokio::fs::write(dir.join(OVERLAY_ASSET), b"export {};")
            .await
            .expect("overlay asset");
    }

    #[tokio::test]
    async fn the_first_fetch_hands_over_the_clip_typed_and_uncacheable() {
        let fixture = serve_default().await;
        let (ticket, _outcome) = fixture.offer().await;

        let response = fixture.fetch(ticket.clip_path()).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            header_of(&response, reqwest::header::CONTENT_TYPE),
            ClipMediaType::Wave.as_str()
        );
        assert_eq!(
            header_of(&response, reqwest::header::CACHE_CONTROL),
            NO_STORE
        );
        assert_eq!(response.bytes().await.expect("body"), CLIP_BYTES);

        fixture.handle.abort();
    }

    #[tokio::test]
    async fn every_miss_answers_alike_so_nothing_tells_a_spent_capability_from_an_unknown_one() {
        let fixture = serve_default().await;

        let (spent, _spent_outcome) = fixture.offer().await;
        fixture.fetch(spent.clip_path()).await;

        let (reported, _reported_outcome) = fixture.offer().await;
        fixture.fetch(reported.clip_path()).await;
        fixture.report(reported.report_path(), played_body()).await;

        let mut misses = Vec::new();
        for (case, path) in [
            ("already fetched", spent.clip_path().to_owned()),
            ("already reported", reported.clip_path().to_owned()),
            ("never minted", clip_path(UNKNOWN_CAPABILITY)),
            ("not a capability at all", clip_path(MALFORMED_CAPABILITY)),
            (
                "shaped like a path traversal",
                clip_path(TRAVERSING_CAPABILITY),
            ),
        ] {
            let response = fixture.fetch(&path).await;
            misses.push((
                case,
                response.status(),
                comparable_headers(&response),
                response.bytes().await.expect("body"),
            ));
        }

        let (first_case, first_status, first_headers, first_body) = &misses[0];
        assert_eq!(*first_status, StatusCode::NOT_FOUND);
        assert!(
            first_body.is_empty(),
            "{first_case}: a miss must carry no body to read"
        );
        for (case, status, headers, body) in &misses[1..] {
            assert_eq!(status, first_status, "{case} answers differently");
            assert_eq!(headers, first_headers, "{case} answers differently");
            assert_eq!(body, first_body, "{case} answers differently");
        }

        fixture.handle.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exactly_one_of_two_simultaneous_fetches_gets_the_clip() {
        let fixture = serve_default().await;
        let (ticket, _outcome) = fixture.offer().await;

        let (first, second) = tokio::join!(
            fixture.fetch(ticket.clip_path()),
            fixture.fetch(ticket.clip_path())
        );

        let mut statuses = [first.status(), second.status()];
        statuses.sort_unstable();
        assert_eq!(
            statuses,
            [StatusCode::OK, StatusCode::NOT_FOUND],
            "a capability that two fetches race for is still single use"
        );

        fixture.handle.abort();
    }

    #[tokio::test]
    async fn two_admitted_pages_both_fetch_the_clip_and_one_refusal_cannot_undo_the_other_playing()
    {
        let fixture = serve_default().await;
        let (ticket, outcome) = fixture.offer().await;
        assert!(
            fixture
                .handle
                .admit_audio_players(ticket.capability(), 2)
                .await
        );

        let first = fixture.fetch(ticket.clip_path()).await;
        let second = fixture.fetch(ticket.clip_path()).await;
        let third = fixture.fetch(ticket.clip_path()).await;
        let refused = fixture
            .report(ticket.report_path(), refused_body(SHORT_REASON))
            .await;
        let played = fixture.report(ticket.report_path(), played_body()).await;

        assert_eq!(
            [first.status(), second.status(), third.status()],
            [StatusCode::OK, StatusCode::OK, StatusCode::NOT_FOUND]
        );
        assert_eq!(
            [refused.status(), played.status()],
            [StatusCode::NO_CONTENT, StatusCode::NO_CONTENT]
        );
        assert_eq!(outcome.recv().await, ClipOutcome::Played);

        fixture.handle.abort();
    }

    #[tokio::test]
    async fn a_played_verdict_resolves_the_offer_and_a_second_report_is_a_miss() {
        let fixture = serve_default().await;
        let (ticket, outcome) = fixture.offer().await;
        fixture.fetch(ticket.clip_path()).await;

        let accepted = fixture.report(ticket.report_path(), played_body()).await;
        let repeated = fixture.report(ticket.report_path(), played_body()).await;

        assert_eq!(accepted.status(), StatusCode::NO_CONTENT);
        assert_eq!(repeated.status(), StatusCode::NOT_FOUND);
        assert_eq!(outcome.recv().await, ClipOutcome::Played);

        fixture.handle.abort();
    }

    #[tokio::test]
    async fn a_refusal_reason_reaches_the_caller_cut_on_a_character_boundary() {
        for (case, reason, expected) in [
            (
                "a short reason",
                SHORT_REASON.to_owned(),
                SHORT_REASON.to_owned(),
            ),
            ("no reason at all", String::new(), String::new()),
            (
                "a reason past the cap",
                "x".repeat(MAX_REASON_CHARS + OVERFLOW_CHARS),
                "x".repeat(MAX_REASON_CHARS),
            ),
            (
                "a multibyte reason past the cap",
                MULTIBYTE_CHAR.repeat(MAX_REASON_CHARS + OVERFLOW_CHARS),
                MULTIBYTE_CHAR.repeat(MAX_REASON_CHARS),
            ),
        ] {
            let fixture = serve_default().await;
            let (ticket, outcome) = fixture.offer().await;
            fixture.fetch(ticket.clip_path()).await;

            let response = fixture
                .report(ticket.report_path(), refused_body(&reason))
                .await;

            assert_eq!(response.status(), StatusCode::NO_CONTENT, "{case}");
            assert_eq!(
                outcome.recv().await,
                ClipOutcome::Refused { reason: expected },
                "{case}"
            );

            fixture.handle.abort();
        }
    }

    #[tokio::test]
    async fn a_report_the_route_cannot_read_leaves_the_clip_waiting_for_a_real_one() {
        for (case, body, expected) in [
            ("not JSON at all", "{".to_owned(), StatusCode::BAD_REQUEST),
            (
                "a verdict the route does not know",
                serde_json::json!({ "verdict": "exploded" }).to_string(),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                "a body past the route's own limit",
                refused_body(&"x".repeat(MAX_REPORT_BODY_BYTES)),
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
        ] {
            let fixture = serve_default().await;
            let (ticket, outcome) = fixture.offer().await;
            fixture.fetch(ticket.clip_path()).await;

            let rejected = fixture.report(ticket.report_path(), body).await;
            assert_eq!(rejected.status(), expected, "{case}");

            let accepted = fixture.report(ticket.report_path(), played_body()).await;
            assert_eq!(
                accepted.status(),
                StatusCode::NO_CONTENT,
                "{case}: the clip must outlive a report the route could not read"
            );
            assert_eq!(outcome.recv().await, ClipOutcome::Played, "{case}");

            fixture.handle.abort();
        }
    }

    #[tokio::test]
    async fn both_audio_routes_answer_unauthenticated_while_the_api_still_demands_a_token() {
        let fixture = serve(
            AUTH_REQUIRED_FOR_READS,
            CORS_MIRRORS_ANY_ORIGIN,
            PathBuf::new(),
            MemSettings::new(),
        )
        .await;
        let (ticket, _outcome) = fixture.offer().await;

        let clip = fixture.fetch(ticket.clip_path()).await;
        let report = fixture.report(ticket.report_path(), played_body()).await;
        let api = fixture.fetch(API_INFO_PATH).await;

        assert_eq!(clip.status(), StatusCode::OK);
        assert_eq!(report.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            api.status(),
            StatusCode::UNAUTHORIZED,
            "the capability guards the audio routes and nothing else"
        );

        fixture.handle.abort();
    }

    #[tokio::test]
    async fn a_cross_site_report_is_served_and_no_foreign_origin_is_echoed_back() {
        let fixture = serve(false, false, PathBuf::new(), MemSettings::new()).await;
        let (ticket, outcome) = fixture.offer().await;
        fixture.fetch(ticket.clip_path()).await;

        let response = fixture
            .report_as(ticket.report_path(), played_body(), Some(FOREIGN_ORIGIN))
            .await;

        assert_eq!(
            response.status(),
            StatusCode::NO_CONTENT,
            "the Origin rule decides what is echoed, never whether the request is served"
        );
        assert_eq!(
            header_of(&response, reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN),
            fixture.own_origin()
        );
        assert_eq!(outcome.recv().await, ClipOutcome::Played);

        fixture.handle.abort();
    }

    #[tokio::test]
    async fn the_audio_routes_echo_the_same_cross_origin_headers_as_the_overlay_route() {
        for (mode, any_origin) in [
            ("mirroring every origin", true),
            ("echoing only accepted origins", false),
        ] {
            let root = tempfile::tempdir().expect("tempdir");
            overlay_root_with_an_asset(root.path()).await;
            let fixture = serve(
                false,
                any_origin,
                root.path().to_path_buf(),
                MemSettings::new(),
            )
            .await;
            let own_origin = fixture.own_origin();

            for (case, origin) in [
                ("no Origin at all", None),
                ("the page's own origin", Some(own_origin.as_str())),
                ("a foreign origin", Some(FOREIGN_ORIGIN)),
            ] {
                let (ticket, _outcome) = fixture.offer().await;
                let overlay = fixture
                    .fetch_as(
                        &format!("/overlays/{OVERLAY_IDENTITY}/{OVERLAY_ASSET}"),
                        origin,
                    )
                    .await;
                let clip = fixture.fetch_as(ticket.clip_path(), origin).await;

                assert_eq!(overlay.status(), StatusCode::OK, "{mode} / {case}");
                assert_eq!(clip.status(), StatusCode::OK, "{mode} / {case}");
                assert_eq!(
                    cross_origin_pair(&clip),
                    cross_origin_pair(&overlay),
                    "{mode} / {case}: the audio route parted ways with the overlay route"
                );
            }

            fixture.handle.abort();
        }
    }

    #[tokio::test]
    async fn no_response_hands_the_capability_back_to_whoever_asked() {
        let fixture = serve_default().await;
        let (ticket, _outcome) = fixture.offer().await;
        let capability = ticket.capability().expose().to_owned();

        let hit = fixture.fetch(ticket.clip_path()).await;
        let miss = fixture.fetch(ticket.clip_path()).await;

        for (case, response) in [
            ("the fetch that succeeds", hit),
            ("the fetch that misses", miss),
        ] {
            for (name, value) in response.headers() {
                assert!(
                    !String::from_utf8_lossy(value.as_bytes()).contains(&capability),
                    "{case}: the {name} header carries the capability"
                );
            }
            let body = response.bytes().await.expect("body");
            assert!(
                !String::from_utf8_lossy(&body).contains(&capability),
                "{case}: the body carries the capability"
            );
        }

        fixture.handle.abort();
    }

    #[tokio::test]
    async fn a_server_that_goes_away_resolves_every_clip_it_still_holds() {
        for (case, graceful) in [("a graceful stop", true), ("an abort", false)] {
            let fixture = serve_default().await;
            let (unfetched, unfetched_outcome) = fixture.offer().await;
            let (fetched, fetched_outcome) = fixture.offer().await;
            fixture.fetch(fetched.clip_path()).await;

            if graceful {
                fixture.handle.stop().await.expect("stop");
            } else {
                fixture.handle.abort();
            }

            assert_eq!(
                unfetched_outcome.recv().await,
                ClipOutcome::ServerStopped,
                "{case}"
            );
            assert_eq!(
                fetched_outcome.recv().await,
                ClipOutcome::ServerStopped,
                "{case}"
            );
            assert!(
                !fixture
                    .handle
                    .revoke_audio_clip(unfetched.capability())
                    .await,
                "{case}: a discarded clip is already gone"
            );
        }
    }

    #[tokio::test]
    async fn a_restart_discards_the_clips_it_held_and_keeps_serving_new_ones() {
        let settings = MemSettings::new();
        let fixture = serve(
            false,
            CORS_MIRRORS_ANY_ORIGIN,
            PathBuf::new(),
            Arc::clone(&settings),
        )
        .await;
        crate::config::ServerSettings::save_bind_address(&*settings, "127.0.0.1")
            .await
            .expect("save address");
        crate::config::ServerSettings::save_port(&*settings, 0)
            .await
            .expect("save port");

        let (_announced, outcome) = fixture.offer().await;
        fixture.handle.restart().await.expect("restart");

        assert_eq!(outcome.recv().await, ClipOutcome::ServerStopped);

        let rebound = Fixture {
            handle: fixture.handle.clone(),
            addr: fixture.handle.bind_addr().await,
        };
        let (fresh, _fresh_outcome) = rebound.offer().await;
        assert_eq!(
            rebound.fetch(fresh.clip_path()).await.status(),
            StatusCode::OK,
            "the clip store must be carried over into the rebound server"
        );

        rebound.handle.abort();
    }

    #[test]
    fn no_request_at_any_level_logs_the_capability_or_either_path() {
        let (secrets, lines) = log_capture::capture_blocking(Level::TRACE, async {
            let fixture = serve_default().await;
            let (ticket, _outcome) = fixture.offer().await;
            let secrets = vec![
                ticket.capability().expose().to_owned(),
                ticket.clip_path().to_owned(),
                ticket.report_path().to_owned(),
            ];

            fixture.fetch(ticket.clip_path()).await;
            fixture.fetch(ticket.clip_path()).await;
            fixture.report(ticket.report_path(), played_body()).await;
            fixture.report(ticket.report_path(), played_body()).await;
            fixture.fetch(&clip_path(UNKNOWN_CAPABILITY)).await;

            let (held, _held_outcome) = fixture.offer().await;
            let mut secrets = secrets;
            secrets.push(held.capability().expose().to_owned());
            fixture.handle.abort();
            tokio::task::yield_now().await;

            secrets
        });

        assert!(
            lines.iter().any(log_capture::CapturedLine::from_forge),
            "the capture saw none of the server's own lines, so it proves nothing"
        );
        for secret in &secrets {
            assert!(
                !lines.iter().any(|line| line.mentions(secret)),
                "a request event named {secret}"
            );
        }
    }
}
