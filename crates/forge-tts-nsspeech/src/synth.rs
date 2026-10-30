#![cfg_attr(
    target_os = "macos",
    expect(unsafe_code, reason = "drives the AppKit speech synthesizer")
)]

#[cfg(target_os = "macos")]
use std::sync::mpsc;

#[cfg(target_os = "macos")]
pub(crate) const SYNTHESIS_WAIT_TIMEOUT: std::time::Duration =
    forge_tts_core::ENGINE_SYNTHESIS_BUDGET;

#[cfg(target_os = "macos")]
use forge_speech_types::PcmBuffer;
#[cfg(target_os = "macos")]
use forge_tts_core::SynthesisRequest;

#[cfg(target_os = "macos")]
use crate::error::NsSpeechError;

#[cfg(target_os = "macos")]
pub(crate) enum NsSpeechRequest {
    Synthesize {
        req: SynthesisRequest,
        tx: tokio::sync::oneshot::Sender<Result<PcmBuffer, NsSpeechError>>,
    },
}

#[cfg(target_os = "macos")]
pub(crate) fn spawn_worker() -> mpsc::Sender<NsSpeechRequest> {
    let (tx, rx) = mpsc::channel::<NsSpeechRequest>();
    std::thread::spawn(move || {
        // SAFETY: the synthesizer is created and used only on this worker thread and has no init preconditions.
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
            NsSpeechRequest::Synthesize { req, tx } => {
                let result = run_synthesis(&synth, req);
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the requester may stop waiting for the reply"
                )]
                let _ = tx.send(result);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn run_synthesis(
    synth: &objc2_avf_audio::AVSpeechSynthesizer,
    req: SynthesisRequest,
) -> Result<PcmBuffer, NsSpeechError> {
    use std::sync::{Arc, Condvar, Mutex};

    use core::ptr::NonNull;
    use objc2_avf_audio::{
        AVAudioBuffer, AVAudioPCMBuffer, AVSpeechBoundary, AVSpeechSynthesisVoice,
        AVSpeechUtterance,
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
                // SAFETY: writeUtterance:toBufferCallback: always delivers an AVAudioPCMBuffer, alive for this call.
                let pcm: &AVAudioPCMBuffer =
                    unsafe { &*(raw_buf.as_ptr() as *const AVAudioPCMBuffer) };

                // SAFETY: `pcm` is a live buffer and frameLength is a plain property read.
                let frame_count = unsafe { pcm.frameLength() } as usize;

                let (mutex, cvar) = &*shared_cb;
                let mut state = mutex.lock().unwrap_or_else(|e| e.into_inner());

                if frame_count == 0 {
                    state.done = true;
                    cvar.notify_one();
                    return;
                }

                // SAFETY: `pcm` is a live buffer and format is a plain property read.
                let fmt = unsafe { pcm.format() };
                // SAFETY: `fmt` is retained above and sampleRate is a plain property read.
                let sr = unsafe { fmt.sampleRate() };
                // SAFETY: `fmt` is retained above and channelCount is a plain property read.
                let ch = unsafe { fmt.channelCount() };
                state.sample_rate = sr as u32;
                state.channels = ch;

                let channel_count = ch as usize;
                // SAFETY: `pcm` is a live buffer; a null result for a non-float format is checked below.
                let float_channels = unsafe { pcm.floatChannelData() };
                if !float_channels.is_null() {
                    for frame in 0..frame_count {
                        for channel in 0..channel_count {
                            // SAFETY: `channel` is below channelCount and `frame` below frameLength of this live buffer.
                            let sample =
                                unsafe { *(*float_channels.add(channel)).as_ptr().add(frame) };
                            state.samples.push(
                                (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32)
                                    as i16,
                            );
                        }
                    }
                    return;
                }
                // SAFETY: `pcm` is a live buffer; a null result for a non-int16 format is checked below.
                let int_channels = unsafe { pcm.int16ChannelData() };
                if int_channels.is_null() {
                    state.done = true;
                    cvar.notify_one();
                    return;
                }
                for frame in 0..frame_count {
                    for channel in 0..channel_count {
                        // SAFETY: `channel` is below channelCount and `frame` below frameLength of this live buffer.
                        let sample = unsafe { *(*int_channels.add(channel)).as_ptr().add(frame) };
                        state.samples.push(sample);
                    }
                }
            });
        });

    let prepared: Result<(), NsSpeechError> = objc2::rc::autoreleasepool(|_| {
        let ns_text = NSString::from_str(&req.text);
        // SAFETY: `ns_text` is a valid NSString alive across the call.
        let utterance = unsafe { AVSpeechUtterance::speechUtteranceWithString(&ns_text) };

        let ns_voice_id = NSString::from_str(&req.voice_id.0);
        // SAFETY: `ns_voice_id` is a valid NSString alive across the call.
        let voice_opt = unsafe { AVSpeechSynthesisVoice::voiceWithIdentifier(&ns_voice_id) };
        let Some(voice) = voice_opt else {
            return Err(NsSpeechError::InvalidVoice(req.voice_id.clone()));
        };
        // SAFETY: voice is a valid Retained value alive within this autoreleasepool.
        unsafe { utterance.setVoice(Some(&voice)) };

        let apple_rate = crate::synth::rate_apple_from_multiplier(req.rate_multiplier);
        let pitch_mult = crate::synth::pitch_mult_from_semitones(req.pitch_semitones);
        // SAFETY: plain float property setter on the utterance owned by this closure.
        unsafe { utterance.setRate(apple_rate) };
        // SAFETY: plain float property setter on the utterance owned by this closure.
        unsafe { utterance.setPitchMultiplier(pitch_mult) };

        // SAFETY: the synthesizer copies the block on write, and `block` stays alive until this function waits out completion.
        unsafe {
            synth.writeUtterance_toBufferCallback(&utterance, block2::RcBlock::as_ptr(&block));
        }
        Ok(())
    });
    prepared?;

    let (mutex, cvar) = &*shared;
    let guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let (final_state, timeout_result) = cvar
        .wait_timeout_while(guard, SYNTHESIS_WAIT_TIMEOUT, |s| !s.done)
        .unwrap_or_else(|e| e.into_inner());

    if timeout_result.timed_out() {
        // SAFETY: plain message send on the synthesizer owned by this worker thread.
        unsafe { synth.stopSpeakingAtBoundary(AVSpeechBoundary::Immediate) };
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

#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(dead_code, reason = "used only by the macos synthesis backend")
)]
pub(crate) fn rate_apple_from_multiplier(multiplier: f32) -> f32 {
    const RATE_DEFAULT: f32 = 0.5;
    const RATE_MAX: f32 = 1.0;
    const RATE_MIN: f32 = 0.0;
    (RATE_DEFAULT + (multiplier - 1.0) * (RATE_MAX - RATE_DEFAULT)).clamp(RATE_MIN, RATE_MAX)
}

#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(dead_code, reason = "used only by the macos synthesis backend")
)]
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
