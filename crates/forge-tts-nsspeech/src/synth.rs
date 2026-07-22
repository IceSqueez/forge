#![allow(unsafe_code)]

#[cfg(target_os = "macos")]
use std::sync::mpsc;

#[cfg(target_os = "macos")]
use forge_tts_core::{PcmBuffer, SynthesisRequest, VoiceId};

#[cfg(target_os = "macos")]
use crate::error::NsSpeechError;

#[cfg(target_os = "macos")]
pub(crate) enum NsSpeechRequest {
    Synthesize {
        voice_id: VoiceId,
        req: SynthesisRequest,
        tx: tokio::sync::oneshot::Sender<Result<PcmBuffer, NsSpeechError>>,
    },
}

#[cfg(target_os = "macos")]
pub(crate) fn spawn_worker() -> mpsc::Sender<NsSpeechRequest> {
    let (tx, rx) = mpsc::channel::<NsSpeechRequest>();
    std::thread::spawn(move || {
        // SAFETY: created and used only on this dedicated thread; AVFoundation delivers
        // callbacks on its own internal thread, so no run loop is required here.
        let synth = unsafe { objc2_avf_audio::AVSpeechSynthesizer::new() };
        worker_loop(synth, rx);
    });
    tx
}

#[cfg(target_os = "macos")]
fn worker_loop(
    synth: objc2::rc::Retained<objc2_avf_audio::AVSpeechSynthesizer>,
    rx: mpsc::Receiver<NsSpeechRequest>,
) {
    while let Ok(req) = rx.recv() {
        match req {
            NsSpeechRequest::Synthesize { voice_id, req, tx } => {
                let result = run_synthesis(&synth, voice_id, req);
                let _ = tx.send(result);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn run_synthesis(
    synth: &objc2_avf_audio::AVSpeechSynthesizer,
    voice_id: VoiceId,
    req: SynthesisRequest,
) -> Result<PcmBuffer, NsSpeechError> {
    use std::sync::{Arc, Condvar, Mutex};

    use core::ptr::NonNull;
    use objc2_avf_audio::{
        AVAudioBuffer, AVAudioPCMBuffer, AVSpeechSynthesisVoice, AVSpeechUtterance,
    };
    use objc2_foundation::NSString;

    struct SynthState {
        samples: Vec<i16>,
        sample_rate: u32,
        channels: u32,
        done: bool,
    }

    let shared = Arc::new((
        Mutex::new(SynthState {
            samples: Vec::new(),
            sample_rate: 22_050,
            channels: 1,
            done: false,
        }),
        Condvar::new(),
    ));
    let shared_cb = Arc::clone(&shared);

    let block: block2::RcBlock<dyn Fn(NonNull<AVAudioBuffer>)> =
        block2::RcBlock::new(move |raw_buf: NonNull<AVAudioBuffer>| {
            objc2::rc::autoreleasepool(|_| {
                // SAFETY: writeUtterance:toBufferCallback: always delivers AVAudioPCMBuffer;
                // raw_buf is valid for this autoreleasepool scope, and shared_cb outlives the
                // RcBlock that owns this closure.
                let pcm: &AVAudioPCMBuffer =
                    unsafe { &*(raw_buf.as_ptr() as *const AVAudioPCMBuffer) };

                let frame_count = unsafe { pcm.frameLength() } as usize;

                let (mutex, cvar) = &*shared_cb;
                let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());

                if frame_count == 0 {
                    state.done = true;
                    cvar.notify_one();
                    return;
                }

                let fmt = unsafe { pcm.format() };
                let sr = unsafe { fmt.sampleRate() };
                let ch = unsafe { fmt.channelCount() };
                state.sample_rate = sr as u32;
                state.channels = ch;

                // SAFETY: non-null and valid for `frameLength` samples since frame_count > 0
                // was checked above; only channel 0 is read (mono speech output).
                let channel_ptrs = unsafe { pcm.floatChannelData() };
                if channel_ptrs.is_null() {
                    state.done = true;
                    cvar.notify_one();
                    return;
                }
                let first_channel = unsafe { (*channel_ptrs).as_ptr() };
                let float_slice = unsafe { std::slice::from_raw_parts(first_channel, frame_count) };
                state.samples.extend(float_slice.iter().map(|&s| {
                    (s * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16
                }));
            });
        });

    objc2::rc::autoreleasepool(|_| {
        let ns_text = NSString::from_str(&req.text);
        let utterance = unsafe { AVSpeechUtterance::speechUtteranceWithString(&ns_text) };

        let ns_voice_id = NSString::from_str(&voice_id.0);
        let voice_opt = unsafe { AVSpeechSynthesisVoice::voiceWithIdentifier(&ns_voice_id) };
        if let Some(voice) = voice_opt {
            // SAFETY: voice is a valid Retained value alive within this autoreleasepool.
            unsafe { utterance.setVoice(Some(&voice)) };
        }

        let apple_rate = crate::synth::rate_apple_from_multiplier(req.rate_multiplier);
        let pitch_mult = crate::synth::pitch_mult_from_semitones(req.pitch_semitones);
        // SAFETY: simple c_float property setters, no aliasing or lifetime concerns.
        unsafe { utterance.setRate(apple_rate) };
        unsafe { utterance.setPitchMultiplier(pitch_mult) };

        // SAFETY: the block pointer is valid for the RcBlock's lifetime, which outlives
        // the condvar wait below; captured state crosses into Apple's callback thread only
        // via Arc<Mutex<...>>, so no raw ObjC pointers cross the thread boundary.
        unsafe {
            synth.writeUtterance_toBufferCallback(&utterance, block2::RcBlock::as_ptr(&block));
        }
    });

    let (mutex, cvar) = &*shared;
    let guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let (final_state, timeout_result) = cvar
        .wait_timeout_while(guard, std::time::Duration::from_secs(30), |s| !s.done)
        .unwrap_or_else(|e| e.into_inner());

    if timeout_result.timed_out() {
        return Err(NsSpeechError::Timeout);
    }
    if final_state.samples.is_empty() {
        return Err(NsSpeechError::NoAudio);
    }

    Ok(PcmBuffer::new(
        final_state.samples.clone(),
        final_state.sample_rate,
        final_state.channels as u16,
    ))
}

/// Multiplier 1.0 maps to `AVSpeechUtteranceDefaultSpeechRate` (0.5), not the scale's midpoint.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn rate_apple_from_multiplier(multiplier: f32) -> f32 {
    const RATE_DEFAULT: f32 = 0.5;
    const RATE_MAX: f32 = 1.0;
    const RATE_MIN: f32 = 0.0;
    (RATE_DEFAULT + (multiplier - 1.0) * (RATE_MAX - RATE_DEFAULT)).clamp(RATE_MIN, RATE_MAX)
}

/// Semitone-to-frequency-ratio formula: `2^(semitones / 12)`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn pitch_mult_from_semitones(semitones: f32) -> f32 {
    2.0_f32.powf(semitones / 12.0).clamp(0.5, 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_maps_multiplier_with_clamping_at_quad_speed() {
        for (input, expected) in [
            (1.0, 0.5),
            (0.25, 0.125),
            (0.5, 0.25),
            (2.0, 1.0),
            (4.0, 1.0),
        ] {
            let got = rate_apple_from_multiplier(input);
            assert!(
                (got - expected).abs() < 1e-6,
                "input {input}: expected {expected}, got {got}"
            );
        }
    }

    #[test]
    fn pitch_maps_semitones_with_clamping_at_octave_extremes() {
        for (input, expected) in [
            (0.0, 1.0),
            (-12.0, 0.5),
            (12.0, 2.0),
            (-6.0, 2.0_f32.powf(-0.5)),
            (6.0, 2.0_f32.powf(0.5)),
        ] {
            let got = pitch_mult_from_semitones(input);
            assert!(
                (got - expected).abs() < 1e-6,
                "input {input}: expected {expected}, got {got}"
            );
        }
    }
}
