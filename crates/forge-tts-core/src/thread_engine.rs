use std::sync::mpsc as blocking_mpsc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use forge_speech_types::{EngineId, PcmBuffer, TtsVoice};
use tokio::sync::{mpsc, oneshot};

use crate::{SynthesisRequest, TtsEngine, TtsError};

pub const ABANDON_POLL_INTERVAL: Duration = Duration::from_millis(100);

const QUEUED_REQUEST_LIMIT: usize = 16;
const THREAD_STOPPED_DETAIL: &str = "engine worker thread stopped";

type SynthesisOutcome = Result<PcmBuffer, TtsError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsmlInput {
    Accepted,
    Refused,
}

#[derive(Debug, Clone)]
pub struct ThreadEngineSpec {
    pub engine_id: EngineId,
    pub thread_name: &'static str,
    pub ssml_input: SsmlInput,
    pub synthesis_budget: Duration,
}

pub struct AbandonWatch<'a> {
    deadline: Option<Instant>,
    requester_left: &'a dyn Fn() -> bool,
}

impl AbandonWatch<'_> {
    pub fn is_abandoned(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
            || (self.requester_left)()
    }
}

struct QueuedSynthesis {
    request: SynthesisRequest,
    deadline: Option<Instant>,
    reply: oneshot::Sender<Option<SynthesisOutcome>>,
}

pub struct ThreadEngine {
    id: EngineId,
    ssml_input: SsmlInput,
    synthesis_budget: Duration,
    voice_catalog: Vec<TtsVoice>,
    queued: mpsc::Sender<QueuedSynthesis>,
}

impl ThreadEngine {
    pub fn spawn<State, Open, Synthesize>(
        spec: ThreadEngineSpec,
        open: Open,
        mut synthesize: Synthesize,
    ) -> Result<Self, TtsError>
    where
        State: 'static,
        Open: FnOnce() -> Result<(State, Vec<TtsVoice>), TtsError> + Send + 'static,
        Synthesize: FnMut(&mut State, SynthesisRequest, &AbandonWatch<'_>) -> SynthesisOutcome
            + Send
            + 'static,
    {
        let ThreadEngineSpec {
            engine_id,
            thread_name,
            ssml_input,
            synthesis_budget,
        } = spec;
        let (queued, received) = mpsc::channel(QUEUED_REQUEST_LIMIT);
        let (opened_tx, opened_rx) = blocking_mpsc::sync_channel(1);

        std::thread::Builder::new()
            .name(thread_name.to_owned())
            .spawn(move || {
                let (catalog, state) = match open() {
                    Ok((state, catalog)) => (Ok(catalog), Some(state)),
                    Err(error) => (Err(error), None),
                };
                if opened_tx.send(catalog).is_ok()
                    && let Some(state) = state
                {
                    serve_queue(state, received, &mut synthesize);
                }
            })?;

        let voice_catalog = opened_rx.recv().map_err(|_| thread_stopped(&engine_id))??;
        Ok(Self {
            id: engine_id,
            ssml_input,
            synthesis_budget,
            voice_catalog,
            queued,
        })
    }

    async fn run_on_thread(&self, request: SynthesisRequest) -> SynthesisOutcome {
        let budget = self.synthesis_budget;
        let (reply, reply_rx) = oneshot::channel();
        let queued = QueuedSynthesis {
            request,
            deadline: Instant::now().checked_add(budget),
            reply,
        };
        let exchange = async {
            self.queued
                .send(queued)
                .await
                .map_err(|_| thread_stopped(&self.id))?;
            reply_rx.await.map_err(|_| thread_stopped(&self.id))
        };

        tokio::time::timeout(budget, exchange)
            .await
            .map_err(|_| TtsError::timeout_after(budget))??
            .unwrap_or_else(|| Err(TtsError::timeout_after(budget)))
    }
}

fn serve_queue<State>(
    mut state: State,
    mut received: mpsc::Receiver<QueuedSynthesis>,
    synthesize: &mut impl FnMut(&mut State, SynthesisRequest, &AbandonWatch<'_>) -> SynthesisOutcome,
) {
    while let Some(queued) = received.blocking_recv() {
        let QueuedSynthesis {
            request,
            deadline,
            reply,
        } = queued;
        let requester_left = || reply.is_closed();
        let watch = AbandonWatch {
            deadline,
            requester_left: &requester_left,
        };
        let outcome = (!watch.is_abandoned()).then(|| synthesize(&mut state, request, &watch));
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the requester may stop waiting for the reply"
        )]
        let _ = reply.send(outcome);
    }
}

fn thread_stopped(id: &EngineId) -> TtsError {
    TtsError::EngineUnavailable {
        id: id.clone(),
        detail: THREAD_STOPPED_DETAIL.into(),
    }
}

#[async_trait]
impl TtsEngine for ThreadEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(self.voice_catalog.clone())
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if self.ssml_input == SsmlInput::Refused && request.text.is_ssml() {
            return Err(TtsError::SsmlUnsupported {
                id: self.id.clone(),
            });
        }
        if !self
            .voice_catalog
            .iter()
            .any(|voice| voice.id == request.voice_id)
        {
            return Err(TtsError::InvalidVoice {
                id: request.voice_id,
            });
        }
        self.run_on_thread(request).await
    }
}
