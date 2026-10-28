#![expect(unsafe_code, reason = "drives the COM speech API")]

use std::marker::PhantomData;
use std::sync::mpsc;

use forge_speech_types::PcmBuffer;
use forge_speech_types::{EngineId, TtsVoice};
use forge_tts_core::SynthesisRequest;
use windows::Win32::Media::Speech::{
    ISpDataKey, ISpObjectToken, ISpObjectTokenCategory, ISpVoice, SPCAT_VOICES,
    SpObjectTokenCategory, SpVoice,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::core::{HRESULT, PCWSTR, PWSTR, w};

use crate::error::SapiError;
use crate::{synth, voices};

const STA_THREAD_NAME: &str = "sapi-sta";
const SPERR_NOT_FOUND: HRESULT = HRESULT(0x8004_503A_u32 as i32);
const FULL_VOLUME: u16 = 100;

pub(crate) enum StaRequest {
    Synthesize {
        req: SynthesisRequest,
        tx: tokio::sync::oneshot::Sender<Result<PcmBuffer, SapiError>>,
    },
}

struct SapiVoiceToken {
    voice: TtsVoice,
    ssml_language: Option<&'static str>,
    token: ISpObjectToken,
}

struct ComApartment {
    bound_to_thread: PhantomData<*const ()>,
}

impl ComApartment {
    fn enter() -> Result<Self, SapiError> {
        // SAFETY: CoInitializeEx takes no pointers here (reserved = None); a successful return (S_OK or S_FALSE) obliges exactly one CoUninitialize on this thread, which Drop does.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| SapiError::ComInit(error.code().0))?;
        Ok(Self {
            bound_to_thread: PhantomData,
        })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: a ComApartment exists only after a successful CoInitializeEx, it is !Send so it drops on the thread that initialized, and it is declared before every COM interface on that thread, so all of them are released before this call.
        unsafe { CoUninitialize() };
    }
}

pub(crate) fn spawn_sta_worker(
    engine_id: EngineId,
) -> Result<(mpsc::Sender<StaRequest>, Vec<TtsVoice>), SapiError> {
    let (req_tx, req_rx) = mpsc::channel::<StaRequest>();
    let (init_tx, init_rx) = mpsc::sync_channel::<Result<Vec<TtsVoice>, SapiError>>(0);

    std::thread::Builder::new()
        .name(STA_THREAD_NAME.into())
        .spawn(move || sta_worker_main(req_rx, init_tx, engine_id))?;

    let voices = init_rx.recv().map_err(|_| SapiError::WorkerTerminated)??;
    Ok((req_tx, voices))
}

fn sta_worker_main(
    rx: mpsc::Receiver<StaRequest>,
    init_tx: mpsc::SyncSender<Result<Vec<TtsVoice>, SapiError>>,
    engine_id: EngineId,
) {
    let _apartment = match ComApartment::enter() {
        Ok(apartment) => apartment,
        Err(error) => {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the requester may stop waiting for the reply"
            )]
            let _ = init_tx.send(Err(error));
            return;
        }
    };

    // SAFETY: this thread entered an STA above; the ISpVoice is created, used and released only on this thread, before the apartment guard drops.
    let voice: ISpVoice = match unsafe { CoCreateInstance(&SpVoice, None, CLSCTX_INPROC_SERVER) } {
        Ok(voice) => voice,
        Err(error) => {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the requester may stop waiting for the reply"
            )]
            let _ = init_tx.send(Err(SapiError::catalog_call("CoCreateInstance(SpVoice)")(
                error,
            )));
            return;
        }
    };

    let tokens = match enumerate_voice_tokens(&engine_id) {
        Ok(tokens) => tokens,
        Err(error) => {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the requester may stop waiting for the reply"
            )]
            let _ = init_tx.send(Err(error));
            return;
        }
    };

    let catalog: Vec<TtsVoice> = tokens.iter().map(|entry| entry.voice.clone()).collect();
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the requester may stop waiting for the reply"
    )]
    let _ = init_tx.send(Ok(catalog));

    while let Ok(request) = rx.recv() {
        match request {
            StaRequest::Synthesize { req, tx } => {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the requester may stop waiting for the reply"
                )]
                let _ = tx.send(synthesize_on_sta(&voice, &tokens, &req));
            }
        }
    }
}

fn enumerate_voice_tokens(engine_id: &EngineId) -> Result<Vec<SapiVoiceToken>, SapiError> {
    // SAFETY: the calling thread holds an STA apartment guard; the category object is created, used and released inside this function on that thread.
    let category: ISpObjectTokenCategory =
        unsafe { CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_INPROC_SERVER) }.map_err(
            SapiError::catalog_call("CoCreateInstance(SpObjectTokenCategory)"),
        )?;

    // SAFETY: SPCAT_VOICES is a static NUL-terminated wide string; false means the voices registry key is opened, never created.
    unsafe { category.SetId(SPCAT_VOICES, false) }
        .map_err(SapiError::catalog_call("ISpObjectTokenCategory::SetId"))?;

    // SAFETY: two null PCWSTR arguments are the documented "no required / no optional attribute filter" values; the returned enumerator is an owned interface.
    let enum_tokens = unsafe { category.EnumTokens(PCWSTR::null(), PCWSTR::null()) }.map_err(
        SapiError::catalog_call("ISpObjectTokenCategory::EnumTokens"),
    )?;

    let mut voice_tokens = Vec::new();
    loop {
        let mut token: Option<ISpObjectToken> = None;
        let mut fetched: u32 = 0;
        // SAFETY: celt = 1 makes Next write at most one interface pointer into `token` (a single Option<ISpObjectToken> slot) and one count into `fetched`; both are live locals for the whole call.
        if let Err(error) = unsafe { enum_tokens.Next(1, &mut token, Some(&mut fetched)) } {
            tracing::warn!(
                %error,
                loaded = voice_tokens.len(),
                "SAPI voice enumeration stopped early"
            );
            break;
        }
        let Some(token) = token.filter(|_| fetched > 0) else {
            break;
        };
        match read_voice_token(token, engine_id) {
            Ok(voice_token) => voice_tokens.push(voice_token),
            Err(error) => tracing::warn!(%error, "skipping unreadable SAPI voice token"),
        }
    }

    Ok(voice_tokens)
}

fn take_co_task_string(value: PWSTR, call: &'static str) -> Result<String, SapiError> {
    if value.is_null() {
        return Err(SapiError::InvalidTokenString { call });
    }
    // SAFETY: `value` is non-null and was just returned by a SAPI getter, which hands the caller a NUL-terminated UTF-16 buffer; it is read once, before the free below.
    let text = unsafe { value.to_string() };
    // SAFETY: SAPI getters allocate the returned string with CoTaskMemAlloc and transfer ownership to the caller; this is the only free and `value` is not used afterwards.
    unsafe { CoTaskMemFree(Some(value.0.cast_const().cast())) };
    text.map_err(|_| SapiError::InvalidTokenString { call })
}

fn read_attribute(
    attributes: &ISpDataKey,
    name: PCWSTR,
    call: &'static str,
) -> Result<String, SapiError> {
    // SAFETY: attributes is a live ISpDataKey on this STA thread and `name` is a static NUL-terminated wide literal.
    let value =
        unsafe { attributes.GetStringValue(name) }.map_err(SapiError::catalog_call(call))?;
    take_co_task_string(value, call)
}

fn read_optional_attribute(attributes: &ISpDataKey, name: PCWSTR, call: &'static str) -> String {
    match read_attribute(attributes, name, call) {
        Ok(value) => value,
        Err(SapiError::Catalog { hr, .. }) if hr == SPERR_NOT_FOUND.0 => String::new(),
        Err(error) => {
            tracing::warn!(%error, "SAPI voice attribute unreadable; treating it as absent");
            String::new()
        }
    }
}

fn read_voice_token(
    token: ISpObjectToken,
    engine_id: &EngineId,
) -> Result<SapiVoiceToken, SapiError> {
    // SAFETY: token is a live ISpObjectToken on this STA thread; GetId returns an owned CoTaskMem string that take_co_task_string frees.
    let id_value =
        unsafe { token.GetId() }.map_err(SapiError::catalog_call("ISpObjectToken::GetId"))?;
    let id = take_co_task_string(id_value, "ISpObjectToken::GetId")?;

    // SAFETY: token is a live ISpObjectToken on this STA thread and the subkey name is a static NUL-terminated wide literal; the returned key is an owned interface.
    let attributes = unsafe { token.OpenKey(w!("Attributes")) }
        .map_err(SapiError::catalog_call("ISpDataKey::OpenKey(Attributes)"))?;

    let name = read_attribute(&attributes, w!("Name"), "ISpDataKey::GetStringValue(Name)")?;
    let language = read_optional_attribute(
        &attributes,
        w!("Language"),
        "ISpDataKey::GetStringValue(Language)",
    );
    let gender = read_optional_attribute(
        &attributes,
        w!("Gender"),
        "ISpDataKey::GetStringValue(Gender)",
    );

    Ok(SapiVoiceToken {
        ssml_language: voices::primary_language_tag(&language),
        voice: voices::build_tts_voice(id, name, &language, &gender, engine_id),
        token,
    })
}

fn synthesize_on_sta(
    voice: &ISpVoice,
    tokens: &[SapiVoiceToken],
    req: &SynthesisRequest,
) -> Result<PcmBuffer, SapiError> {
    let entry = tokens
        .iter()
        .find(|entry| entry.voice.id == req.voice_id)
        .ok_or_else(|| SapiError::VoiceNotFound(req.voice_id.clone()))?;

    // SAFETY: voice and entry.token are live interfaces created in this STA; SetVoice takes its own reference to the token.
    unsafe { voice.SetVoice(&entry.token) }
        .map_err(SapiError::synthesis_call("ISpVoice::SetVoice"))?;

    let rate_adjust = synth::rate_adj_from_multiplier(req.rate_multiplier);
    // SAFETY: voice is a live ISpVoice in this STA and SetRate takes a plain integer.
    unsafe { voice.SetRate(rate_adjust) }
        .map_err(SapiError::synthesis_call("ISpVoice::SetRate"))?;

    // SAFETY: voice is a live ISpVoice in this STA and SetVolume takes a plain integer.
    unsafe { voice.SetVolume(FULL_VOLUME) }
        .map_err(SapiError::synthesis_call("ISpVoice::SetVolume"))?;

    let speak_text = synth::prepare_speak_text(
        &req.text,
        req.pitch_semitones,
        req.ssml,
        entry.ssml_language,
    );

    synth::capture_pcm(voice, &speak_text)
}
