use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use forge_emotes::endpoint_surfaces::{BETTER_TTV_API, FRANKER_FACE_Z_API, SEVEN_TV_API};
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::EmulatorError;
use crate::fakes::support::{Observed, ServerTasks, bind_loopback, local_addr, serve_http};

const SEVEN_TV_PREFIX: &str = "/7tv/v3";
const BETTER_TTV_PREFIX: &str = "/bttv/3";
const FRANKER_FACE_Z_PREFIX: &str = "/ffz/v1";

const SEVEN_TV_GLOBAL_PATH: &str = "/7tv/v3/emote-sets/global";
const BETTER_TTV_GLOBAL_PATH: &str = "/bttv/3/cached/emotes/global";
const FRANKER_FACE_Z_GLOBAL_PATH: &str = "/ffz/v1/set/global";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedEmotesRequest {
    pub path: String,
    pub status: u16,
}

type Shared = Observed<Vec<RecordedEmotesRequest>>;

pub struct FakeEmotes {
    shared: Arc<Shared>,
    origin: String,
    server: ServerTasks,
}

impl FakeEmotes {
    pub async fn start() -> Result<Self, EmulatorError> {
        let listener = bind_loopback().await?;
        let origin = format!("http://{}", local_addr(&listener)?);
        let shared = Arc::new(Observed::new(Vec::new()));
        let (shutdown, shutdown_rx) = watch::channel(false);
        let router = Router::new()
            .fallback(handle)
            .with_state(Arc::clone(&shared));
        let task = serve_http(listener, router, shutdown_rx);
        Ok(Self {
            shared,
            origin,
            server: ServerTasks::new(shutdown, vec![task]),
        })
    }

    pub fn endpoint_overrides(&self) -> [(&'static str, String); 3] {
        [
            (
                SEVEN_TV_API.env_var(),
                format!("{}{SEVEN_TV_PREFIX}", self.origin),
            ),
            (
                BETTER_TTV_API.env_var(),
                format!("{}{BETTER_TTV_PREFIX}", self.origin),
            ),
            (
                FRANKER_FACE_Z_API.env_var(),
                format!("{}{FRANKER_FACE_Z_PREFIX}", self.origin),
            ),
        ]
    }

    pub fn requests(&self) -> Vec<RecordedEmotesRequest> {
        self.shared.read(Clone::clone)
    }

    pub async fn shutdown(self) {
        self.server.shutdown().await;
    }
}

async fn handle(State(shared): State<Arc<Shared>>, uri: Uri) -> Response {
    let (status, body) = answer(uri.path());
    shared.mutate(|requests| {
        requests.push(RecordedEmotesRequest {
            path: uri.path().to_owned(),
            status: status.as_u16(),
        });
    });
    match body {
        Some(body) => (status, Json(body)).into_response(),
        None => status.into_response(),
    }
}

fn answer(path: &str) -> (StatusCode, Option<Value>) {
    match path {
        SEVEN_TV_GLOBAL_PATH => (
            StatusCode::OK,
            Some(json!({ "id": "global", "name": "Global Emotes", "emotes": [] })),
        ),
        BETTER_TTV_GLOBAL_PATH => (StatusCode::OK, Some(json!([]))),
        FRANKER_FACE_Z_GLOBAL_PATH => (
            StatusCode::OK,
            Some(json!({ "default_sets": [], "sets": {} })),
        ),
        _ => (StatusCode::NOT_FOUND, None),
    }
}
