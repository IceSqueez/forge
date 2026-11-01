#![expect(unsafe_code, reason = "drives the COM voice while speech runs")]

use forge_tts_core::{ABANDON_POLL_INTERVAL, AbandonWatch};
use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::Speech::{ISpVoice, SPF_PURGEBEFORESPEAK, SPVOICESTATUS};
use windows::core::{Interface, PCWSTR};

use crate::error::SapiError;

const POLL_SLICE_MS: u32 = ABANDON_POLL_INTERVAL.as_millis() as u32;
const WAIT_WITHOUT_LIMIT_MS: u32 = u32::MAX;

pub(crate) fn wait_for_speech(voice: &ISpVoice, watch: &AbandonWatch<'_>) -> Result<(), SapiError> {
    loop {
        match finished_within(voice, POLL_SLICE_MS) {
            Ok(true) => return last_speak_result(voice),
            Ok(false) if !watch.is_abandoned() => {}
            Ok(false) => return purge_speech(voice).and(Err(SapiError::Timeout)),
            Err(error) => return purge_speech(voice).and(Err(error)),
        }
    }
}

fn finished_within(voice: &ISpVoice, timeout_ms: u32) -> Result<bool, SapiError> {
    // SAFETY: voice is a live ISpVoice in this STA and the call takes a plain integer; the vtable is called directly because the generated wrapper folds S_FALSE, the "still speaking" answer, into Ok.
    let hr = unsafe { (voice.vtable().WaitUntilDone)(voice.as_raw(), timeout_ms) };
    hr.ok()
        .map_err(SapiError::synthesis_call("ISpVoice::WaitUntilDone"))?;
    Ok(hr == S_OK)
}

fn last_speak_result(voice: &ISpVoice) -> Result<(), SapiError> {
    let mut status = SPVOICESTATUS::default();
    // SAFETY: status is a live local for this call and a null bookmark pointer means the caller does not want the bookmark.
    unsafe { voice.GetStatus(&mut status, std::ptr::null_mut()) }
        .map_err(SapiError::synthesis_call("ISpVoice::GetStatus"))?;
    status
        .hrLastResult
        .ok()
        .map_err(SapiError::synthesis_call("ISpVoice::Speak"))
}

fn purge_speech(voice: &ISpVoice) -> Result<(), SapiError> {
    // SAFETY: a null text with SPF_PURGEBEFORESPEAK is the documented purge request and SAPI reads no text pointer for it.
    let purged = unsafe { voice.Speak(PCWSTR::null(), SPF_PURGEBEFORESPEAK.0 as u32, None) };
    if let Err(error) = purged {
        tracing::warn!(%error, "SAPI refused to purge abandoned speech");
    }
    match finished_within(voice, WAIT_WITHOUT_LIMIT_MS) {
        Ok(true) => Ok(()),
        Ok(false) => Err(SapiError::VoiceStillBusy),
        Err(error) => {
            tracing::warn!(%error, "SAPI wait after purge failed");
            Err(SapiError::VoiceStillBusy)
        }
    }
}
