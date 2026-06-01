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
        // SAFETY: AVSpeechSynthesizer is created on this dedicated worker thread and never
        // sent to another thread. Retained<AVSpeechSynthesizer> lives exclusively here for
        // the entire thread lifetime. The thread pumps no run loop; AVFoundation delivers
        // writeUtterance:toBufferCallback: callbacks on its own internal background thread,
        // so this call site is safe without a run loop.
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
                // SAFETY: (a) Apple's writeUtterance:toBufferCallback: always delivers
                // AVAudioPCMBuffer instances for speech synthesis; the downcast is sound.
                // (b) The raw_buf pointer is valid for the duration of this autoreleasepool
                // scope, guaranteed by AVFoundation's callback contract. (c) The shared Arc
                // is alive for at least as long as the RcBlock, which lives in run_synthesis's
                // stack frame across the condvar wait. (d) The Mutex guard is held only
                // within this closure invocation; no deadlock is possible since we never
                // re-enter the lock recursively.
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

                // SAFETY: floatChannelData returns a pointer to an array of `channelCount`
                // float-channel pointers, each valid for `frameLength` samples. The pointer
                // is non-null because we checked frame_count > 0 and the format is PCM
                // float (always the case for AVSpeech output). We read only channel 0 for
                // mono speech output.
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
        // SAFETY: NSString::from_str creates an immutable NSString from a valid UTF-8 str.
        // The returned Retained<NSString> is valid inside this autoreleasepool.
        let ns_text = NSString::from_str(&req.text);
        let utterance = unsafe { AVSpeechUtterance::speechUtteranceWithString(&ns_text) };

        let ns_voice_id = NSString::from_str(&voice_id.0);
        let voice_opt = unsafe { AVSpeechSynthesisVoice::voiceWithIdentifier(&ns_voice_id) };
        if let Some(voice) = voice_opt {
            // SAFETY: setVoice: takes an optional voice reference. voice is a valid
            // Retained<AVSpeechSynthesisVoice> alive within this autoreleasepool.
            unsafe { utterance.setVoice(Some(&voice)) };
        }

        let apple_rate = crate::synth::rate_apple_from_multiplier(req.rate_multiplier);
        let pitch_mult = crate::synth::pitch_mult_from_semitones(req.pitch_semitones);
        // SAFETY: setRate: and setPitchMultiplier: accept c_float values; both are simple
        // property setters on AVSpeechUtterance with no aliasing or lifetime concerns.
        unsafe { utterance.setRate(apple_rate) };
        unsafe { utterance.setPitchMultiplier(pitch_mult) };

        // SAFETY: (a) writeUtterance:toBufferCallback: must be called with a valid block
        // pointer. RcBlock::as_ptr returns a non-null, heap-allocated block pointer valid
        // for the RcBlock's lifetime. (b) The RcBlock lives in this function's stack frame
        // and is kept alive until after the condvar wait returns. (c) The callback is
        // invoked on Apple's internal audio-synthesis background thread; all captured state
        // uses Arc<Mutex<...>> which is Send + Sync, so the cross-thread access is sound.
        // (d) Only Vec<i16> and format scalars leave the callback via the Mutex; no ObjC
        // pointers cross the thread boundary.
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

/// Maps a rate multiplier ([0.25, 4.0]) to AVFoundation's [0.0, 1.0] speech rate scale.
///
/// `AVSpeechUtteranceDefaultSpeechRate` (0.5) maps to multiplier 1.0.
/// Multipliers above 1.0 push toward `AVSpeechUtteranceMaximumSpeechRate` (1.0);
/// multipliers below 1.0 push toward `AVSpeechUtteranceMinimumSpeechRate` (0.0).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn rate_apple_from_multiplier(multiplier: f32) -> f32 {
    const RATE_DEFAULT: f32 = 0.5;
    const RATE_MAX: f32 = 1.0;
    const RATE_MIN: f32 = 0.0;
    (RATE_DEFAULT + (multiplier - 1.0) * (RATE_MAX - RATE_DEFAULT)).clamp(RATE_MIN, RATE_MAX)
}

/// Maps pitch semitones ([-12.0, 12.0]) to AVFoundation pitchMultiplier ([0.5, 2.0]).
///
/// Uses the standard semitone-to-frequency-ratio formula: `2^(semitones / 12)`.
/// The range [-12, +12] maps exactly to [0.5, 2.0] before clamping.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn pitch_mult_from_semitones(semitones: f32) -> f32 {
    2.0_f32.powf(semitones / 12.0).clamp(0.5, 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_neutral_multiplier_gives_default() {
        let r = rate_apple_from_multiplier(1.0);
        assert!((r - 0.5).abs() < 1e-6, "expected 0.5, got {r}");
    }

    #[test]
    fn rate_quarter_speed_clamped_to_min() {
        let r = rate_apple_from_multiplier(0.25);
        assert!((r - 0.125).abs() < 1e-6, "expected 0.125, got {r}");
    }

    #[test]
    fn rate_half_speed() {
        let r = rate_apple_from_multiplier(0.5);
        assert!((r - 0.25).abs() < 1e-6, "expected 0.25, got {r}");
    }

    #[test]
    fn rate_double_speed_gives_max() {
        let r = rate_apple_from_multiplier(2.0);
        assert!((r - 1.0).abs() < 1e-6, "expected 1.0, got {r}");
    }

    #[test]
    fn rate_quad_speed_clamped_to_max() {
        let r = rate_apple_from_multiplier(4.0);
        assert!((r - 1.0).abs() < 1e-6, "expected 1.0 (clamped), got {r}");
    }

    #[test]
    fn pitch_zero_semitones_gives_one() {
        let p = pitch_mult_from_semitones(0.0);
        assert!((p - 1.0).abs() < 1e-6, "expected 1.0, got {p}");
    }

    #[test]
    fn pitch_minus_twelve_semitones_gives_half() {
        let p = pitch_mult_from_semitones(-12.0);
        assert!((p - 0.5).abs() < 1e-6, "expected 0.5, got {p}");
    }

    #[test]
    fn pitch_plus_twelve_semitones_gives_two() {
        let p = pitch_mult_from_semitones(12.0);
        assert!((p - 2.0).abs() < 1e-6, "expected 2.0, got {p}");
    }

    #[test]
    fn pitch_minus_six_semitones_approx_sqrt_half() {
        let p = pitch_mult_from_semitones(-6.0);
        let expected = 2.0_f32.powf(-0.5);
        assert!((p - expected).abs() < 1e-6, "expected ~{expected}, got {p}");
    }

    #[test]
    fn pitch_plus_six_semitones_approx_sqrt_two() {
        let p = pitch_mult_from_semitones(6.0);
        let expected = 2.0_f32.powf(0.5);
        assert!((p - expected).abs() < 1e-6, "expected ~{expected}, got {p}");
    }
}
