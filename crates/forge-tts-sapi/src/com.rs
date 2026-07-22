#![allow(unsafe_code)]

use std::sync::mpsc;

use forge_audio::PcmBuffer;
use forge_tts_core::{EngineId, SynthesisRequest, TtsVoice, VoiceId};
use windows::Win32::Media::Speech::{
    IEnumSpObjectTokens, ISpObjectToken, ISpObjectTokenCategory, ISpVoice, SPCAT_VOICES,
    SpObjectTokenCategory, SpVoice,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};

use crate::error::SapiError;
use crate::{synth, voices};

#[allow(dead_code)]
pub(crate) enum StaRequest {
    ListVoices {
        tx: tokio::sync::oneshot::Sender<Result<Vec<TtsVoice>, SapiError>>,
    },
    Synthesize {
        voice_id: VoiceId,
        req: SynthesisRequest,
        tx: tokio::sync::oneshot::Sender<Result<PcmBuffer, SapiError>>,
    },
}

pub(crate) fn spawn_sta_worker(
    engine_id: EngineId,
) -> Result<(mpsc::Sender<StaRequest>, Vec<TtsVoice>), SapiError> {
    let (req_tx, req_rx) = mpsc::channel::<StaRequest>();
    let (init_tx, init_rx) = mpsc::sync_channel::<Result<Vec<TtsVoice>, SapiError>>(0);

    std::thread::spawn(move || {
        sta_worker_main(req_rx, init_tx, engine_id);
    });

    let voices = init_rx.recv().map_err(|_| SapiError::WorkerTerminated)??;
    Ok((req_tx, voices))
}

fn sta_worker_main(
    rx: mpsc::Receiver<StaRequest>,
    init_tx: mpsc::SyncSender<Result<Vec<TtsVoice>, SapiError>>,
    engine_id: EngineId,
) {
    // SAFETY: this dedicated STA thread creates, uses, and drops every SAPI COM pointer;
    // only plain heap data (Vec<TtsVoice>, PcmBuffer) crosses the channel.
    if let Err(e) = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok() {
        let _ = init_tx.send(Err(SapiError::ComInit(e.code().0)));
        return;
    }

    // SAFETY: called after CoInitializeEx above; ISpVoice is owned exclusively by this thread.
    let voice: ISpVoice = match unsafe { CoCreateInstance(&SpVoice, None, CLSCTX_INPROC_SERVER) } {
        Ok(v) => v,
        Err(e) => {
            let _ = init_tx.send(Err(SapiError::ComInit(e.code().0)));
            // SAFETY: balances the CoInitializeEx above, on the same thread.
            unsafe { CoUninitialize() };
            return;
        }
    };

    let tokens = match enumerate_voice_tokens(&voice, &engine_id) {
        Ok(t) => t,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            // SAFETY: balances the CoInitializeEx above, on the same thread.
            unsafe { CoUninitialize() };
            return;
        }
    };

    let catalog: Vec<TtsVoice> = tokens.iter().map(|(v, _)| v.clone()).collect();
    let _ = init_tx.send(Ok(catalog));

    while let Ok(req) = rx.recv() {
        match req {
            StaRequest::ListVoices { tx } => {
                let v: Vec<TtsVoice> = tokens.iter().map(|(v, _)| v.clone()).collect();
                let _ = tx.send(Ok(v));
            }
            StaRequest::Synthesize { voice_id, req, tx } => {
                let result = synthesize_on_sta(&voice, &tokens, voice_id, req);
                let _ = tx.send(result);
            }
        }
    }

    // SAFETY: balances the CoInitializeEx called at thread start.
    unsafe { CoUninitialize() };
}

fn enumerate_voice_tokens(
    _voice: &ISpVoice,
    engine_id: &EngineId,
) -> Result<Vec<(TtsVoice, ISpObjectToken)>, SapiError> {
    // SAFETY: this STA thread has already called CoInitializeEx; no COM pointer escapes
    // this function beyond the return value, which stays on this thread.
    let category: ISpObjectTokenCategory =
        unsafe { CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| SapiError::ComInit(e.code().0))?;

    // SAFETY: SetId selects the HKLM voice registry key; false means do not create it.
    unsafe { category.SetId(SPCAT_VOICES, false) }.map_err(|e| SapiError::ComInit(e.code().0))?;

    // SAFETY: null PCWSTR arguments mean no attribute filter is applied.
    let enum_tokens: IEnumSpObjectTokens = unsafe {
        category.EnumTokens(windows::core::PCWSTR::null(), windows::core::PCWSTR::null())
    }
    .map_err(|e| SapiError::ComInit(e.code().0))?;

    let mut result = Vec::new();
    loop {
        let mut token: Option<ISpObjectToken> = None;
        let mut fetched: u32 = 0;
        // SAFETY: writes at most 1 token pointer into `token`; both out-params are
        // stack-allocated on this STA thread.
        let hr = unsafe { enum_tokens.Next(1, &mut token, Some(&mut fetched)) };
        if hr.is_err() || fetched == 0 {
            break;
        }
        if let Some(tok) = token
            && let Ok(tv) = token_to_tts_voice(&tok, engine_id)
        {
            result.push((tv, tok));
        }
    }

    Ok(result)
}

fn get_attribute(token: &ISpObjectToken, name: windows::core::PCWSTR) -> Result<String, SapiError> {
    // SAFETY: token is a valid COM object on this STA thread.
    let attrs = unsafe { token.OpenKey(windows::core::w!("Attributes")) }
        .map_err(|e| SapiError::ComInit(e.code().0))?;
    // SAFETY: GetStringValue returns a CoTaskMem-allocated, valid, non-null UTF-16 PWSTR.
    let pwstr =
        unsafe { attrs.GetStringValue(name) }.map_err(|e| SapiError::ComInit(e.code().0))?;
    unsafe { pwstr.to_string() }.map_err(|_| SapiError::ComInit(-1))
}

fn get_id(token: &ISpObjectToken) -> Result<String, SapiError> {
    // SAFETY: GetId returns a CoTaskMem-allocated, valid, non-null UTF-16 PWSTR.
    let pwstr = unsafe { token.GetId() }.map_err(|e| SapiError::ComInit(e.code().0))?;
    unsafe { pwstr.to_string() }.map_err(|_| SapiError::ComInit(-1))
}

fn token_to_tts_voice(token: &ISpObjectToken, engine_id: &EngineId) -> Result<TtsVoice, SapiError> {
    let id_str = get_id(token)?;
    let name = get_attribute(token, windows::core::w!("Name"))?;
    let lang_hex = get_attribute(token, windows::core::w!("Language")).unwrap_or_default();
    let gender_str = get_attribute(token, windows::core::w!("Gender")).unwrap_or_default();
    Ok(voices::build_tts_voice(
        id_str,
        name,
        &lang_hex,
        &gender_str,
        engine_id,
    ))
}

fn synthesize_on_sta(
    voice: &ISpVoice,
    tokens: &[(TtsVoice, ISpObjectToken)],
    voice_id: VoiceId,
    req: SynthesisRequest,
) -> Result<PcmBuffer, SapiError> {
    let token = tokens
        .iter()
        .find(|(tv, _)| tv.id == voice_id)
        .map(|(_, tok)| tok)
        .ok_or(SapiError::ComInit(0))?;

    // SAFETY: voice and token are both valid COM objects owned by this STA thread.
    unsafe { voice.SetVoice(token) }.map_err(|e| SapiError::ComInit(e.code().0))?;

    let rate_adj = synth::rate_adj_from_multiplier(req.rate_multiplier);
    if rate_adj != 0 {
        // SAFETY: plain i32 value, no aliasing or lifetime concerns.
        unsafe { voice.SetRate(rate_adj) }.map_err(|e| SapiError::ComInit(e.code().0))?;
    }

    let (text, use_xml) = synth::prepare_speak_text(&req.text, req.pitch_semitones, req.ssml);

    synth::capture_pcm(voice, &text, use_xml)
}
