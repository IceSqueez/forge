use forge_tts_core::xml_escape;

pub(crate) const SAMPLE_RATE_HZ: u32 = 22_050;

const RATE_ADJUST_MIN: i32 = -10;
const RATE_ADJUST_MAX: i32 = 10;
const RATE_STEPS_PER_DOUBLING: f32 = 10.0;
const SEMITONES_PER_OCTAVE: f32 = 12.0;
const PERCENT: f32 = 100.0;
const SSML_VERSION: &str = "1.0";
const SSML_NAMESPACE: &str = "http://www.w3.org/2001/10/synthesis";

#[cfg_attr(
    not(any(target_os = "windows", test)),
    expect(dead_code, reason = "used only by the windows synthesis backend")
)]
pub(crate) enum SpeakText {
    Plain(String),
    Ssml(String),
}

#[cfg_attr(
    not(any(target_os = "windows", test)),
    expect(dead_code, reason = "used only by the windows synthesis backend")
)]
pub(crate) fn rate_adj_from_multiplier(multiplier: f32) -> i32 {
    ((multiplier.log2() * RATE_STEPS_PER_DOUBLING).round() as i32)
        .clamp(RATE_ADJUST_MIN, RATE_ADJUST_MAX)
}

#[cfg_attr(
    not(any(target_os = "windows", test)),
    expect(dead_code, reason = "used only by the windows synthesis backend")
)]
pub(crate) fn pitch_pct_from_semitones(semitones: f32) -> i32 {
    ((2.0_f32.powf(semitones / SEMITONES_PER_OCTAVE) - 1.0) * PERCENT).round() as i32
}

#[cfg_attr(
    not(any(target_os = "windows", test)),
    expect(dead_code, reason = "used only by the windows synthesis backend")
)]
pub(crate) fn wrap_with_pitch_prosody(text: &str, pitch_pct: i32, language: &str) -> String {
    format!(
        r#"<speak version="{SSML_VERSION}" xmlns="{SSML_NAMESPACE}" xml:lang="{language}"><prosody pitch="{pitch_pct:+}%">{text}</prosody></speak>"#,
        language = xml_escape(language),
        text = xml_escape(text),
    )
}

#[cfg_attr(
    not(any(target_os = "windows", test)),
    expect(dead_code, reason = "used only by the windows synthesis backend")
)]
pub(crate) fn prepare_speak_text(
    text: &str,
    pitch_semitones: f32,
    is_ssml: bool,
    ssml_language: Option<&str>,
) -> SpeakText {
    if is_ssml {
        return SpeakText::Ssml(text.to_owned());
    }
    let pitch_pct = pitch_pct_from_semitones(pitch_semitones);
    if pitch_pct == 0 {
        return SpeakText::Plain(text.to_owned());
    }
    match ssml_language {
        Some(language) => SpeakText::Ssml(wrap_with_pitch_prosody(text, pitch_pct, language)),
        None => {
            tracing::warn!(
                pitch_pct,
                "SAPI voice language is unknown; speaking without the pitch change"
            );
            SpeakText::Plain(text.to_owned())
        }
    }
}

#[cfg(target_os = "windows")]
#[expect(unsafe_code, reason = "drives the COM voice and its audio stream")]
pub(crate) fn capture_pcm(
    voice: &windows::Win32::Media::Speech::ISpVoice,
    speak_text: &SpeakText,
) -> Result<forge_speech_types::PcmBuffer, crate::error::SapiError> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::Media::Audio::{WAVE_FORMAT_PCM, WAVEFORMATEX};
    use windows::Win32::Media::Speech::{
        ISpStream, SPF_IS_NOT_XML, SPF_IS_XML, SPF_PARSE_SSML, SpStream,
    };
    use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, IStream, STREAM_SEEK_SET,
    };

    use crate::error::SapiError;

    const SPDFID_WAVE_FORMAT_EX: windows::core::GUID =
        windows::core::GUID::from_u128(0xc31adbae_527f_4ff5_a230_f62bb61ff70c);
    const CHANNELS: u16 = 1;
    const BITS_PER_SAMPLE: u16 = i16::BITS as u16;
    const SAMPLE_BYTES: usize = size_of::<i16>();
    const BLOCK_ALIGN_BYTES: u16 = CHANNELS * SAMPLE_BYTES as u16;
    const READ_CHUNK_BYTES: usize = 4096;

    // SAFETY: a null HGLOBAL makes CreateStreamOnHGlobal allocate its own memory, and fDeleteOnRelease = true frees it when the returned IStream is released.
    let com_stream: IStream = unsafe { CreateStreamOnHGlobal(HGLOBAL(std::ptr::null_mut()), true) }
        .map_err(SapiError::synthesis_call("CreateStreamOnHGlobal"))?;

    let wave_format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: CHANNELS,
        nSamplesPerSec: SAMPLE_RATE_HZ,
        nAvgBytesPerSec: SAMPLE_RATE_HZ * u32::from(BLOCK_ALIGN_BYTES),
        nBlockAlign: BLOCK_ALIGN_BYTES,
        wBitsPerSample: BITS_PER_SAMPLE,
        cbSize: 0,
    };

    // SAFETY: this thread entered a COM apartment before calling here (the STA worker holds its apartment guard for its whole lifetime).
    let sp_stream: ISpStream = unsafe { CoCreateInstance(&SpStream, None, CLSCTX_INPROC_SERVER) }
        .map_err(SapiError::synthesis_call("CoCreateInstance(SpStream)"))?;

    // SAFETY: com_stream is a live IStream and both format pointers are valid for this call; SetBaseStream copies the GUID and WAVEFORMATEX into the stream's own format block, so nothing dangles after this frame.
    unsafe { sp_stream.SetBaseStream(&com_stream, &SPDFID_WAVE_FORMAT_EX, &wave_format) }
        .map_err(SapiError::synthesis_call("ISpStream::SetBaseStream"))?;

    // SAFETY: voice and sp_stream are live interfaces created in this apartment; the voice takes its own reference to the stream.
    unsafe { voice.SetOutput(&sp_stream, false) }
        .map_err(SapiError::synthesis_call("ISpVoice::SetOutput"))?;

    let (text, speak_flags) = match speak_text {
        SpeakText::Plain(text) => (text, SPF_IS_NOT_XML.0),
        SpeakText::Ssml(text) => (text, SPF_IS_XML.0 | SPF_PARSE_SSML.0),
    };
    let text_utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let pcwstr = windows::core::PCWSTR(text_utf16.as_ptr());

    // SAFETY: pcwstr points into text_utf16, which is NUL-terminated and outlives this synchronous call (no SPF_ASYNC, so SAPI does not keep the pointer after returning).
    unsafe { voice.Speak(pcwstr, speak_flags as u32, None) }
        .map_err(SapiError::synthesis_call("ISpVoice::Speak"))?;

    // SAFETY: com_stream is a live IStream owned by this function; the new-position out-parameter is None.
    unsafe { com_stream.Seek(0i64, STREAM_SEEK_SET, None) }
        .map_err(SapiError::synthesis_call("IStream::Seek"))?;

    let mut pcm_bytes = Vec::<u8>::new();
    let mut chunk = [0u8; READ_CHUNK_BYTES];
    loop {
        let mut read: u32 = 0;
        // SAFETY: chunk is a writable buffer of exactly chunk.len() bytes and `read` is a valid u32 out-parameter; both live on this stack frame for the whole call.
        let hr = unsafe {
            com_stream.Read(
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                Some(&mut read),
            )
        };
        if hr.is_err() {
            return Err(SapiError::Synthesis {
                call: "IStream::Read",
                hr: hr.0,
            });
        }
        if read == 0 {
            break;
        }
        pcm_bytes.extend_from_slice(&chunk[..read as usize]);
    }

    let (sample_bytes, _) = pcm_bytes.as_chunks::<SAMPLE_BYTES>();
    let samples: Vec<i16> = sample_bytes
        .iter()
        .map(|bytes| i16::from_le_bytes(*bytes))
        .collect();

    Ok(forge_speech_types::PcmBuffer::new(
        samples,
        SAMPLE_RATE_HZ,
        CHANNELS,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_adj_maps_multiplier_with_clamping() {
        for (mult, expected) in [(1.0, 0), (2.0, 10), (0.5, -10), (0.25, -10), (4.0, 10)] {
            assert_eq!(rate_adj_from_multiplier(mult), expected, "mult={mult}");
        }
    }

    #[test]
    fn pitch_pct_maps_semitones_with_clamping() {
        assert_eq!(pitch_pct_from_semitones(0.0), 0);
        assert_eq!(pitch_pct_from_semitones(12.0), 100);
        assert_eq!(pitch_pct_from_semitones(-12.0), -50);
        let up6 = pitch_pct_from_semitones(6.0);
        assert!(up6 > 30 && up6 < 50, "expected ~41, got {up6}");
        let down6 = pitch_pct_from_semitones(-6.0);
        assert!(down6 < -20 && down6 > -40, "expected ~-29, got {down6}");
    }

    fn unpack(speak_text: SpeakText) -> (bool, String) {
        match speak_text {
            SpeakText::Plain(text) => (false, text),
            SpeakText::Ssml(text) => (true, text),
        }
    }

    #[test]
    fn wrap_prosody_builds_full_ssml_root_with_escaped_text() {
        assert_eq!(
            wrap_with_pitch_prosody(r#"Tom & <Jerry> "hi""#, -50, "en-US"),
            r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="en-US"><prosody pitch="-50%">Tom &amp; &lt;Jerry&gt; &quot;hi&quot;</prosody></speak>"#
        );
    }

    #[test]
    fn wrap_prosody_keeps_injected_markup_inside_the_prosody_text() {
        let xml = wrap_with_pitch_prosody(
            r#"</prosody><voice required="Gender=Female">pwned"#,
            100,
            "en-US",
        );
        assert_eq!(xml.matches("</prosody>").count(), 1);
        assert!(
            !xml.contains("<voice"),
            "markup escaped the text node: {xml}"
        );
    }

    #[test]
    fn prepare_speak_text_picks_plain_or_ssml_per_pitch_language_and_input_kind() {
        let caller_ssml = r#"<speak version="1.0">hi & bye</speak>"#;
        for (text, semitones, is_ssml, language, expected_ssml, expected_text) in [
            ("a < b", 0.0, false, Some("en-US"), false, "a < b"),
            ("a < b", 0.01, false, Some("en-US"), false, "a < b"),
            (
                "hi",
                12.0,
                false,
                Some("uk-UA"),
                true,
                r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="uk-UA"><prosody pitch="+100%">hi</prosody></speak>"#,
            ),
            ("a < b", 12.0, false, None, false, "a < b"),
            (caller_ssml, 0.0, true, None, true, caller_ssml),
            (caller_ssml, 12.0, true, Some("en-US"), true, caller_ssml),
        ] {
            let (is_xml, spoken) = unpack(prepare_speak_text(text, semitones, is_ssml, language));
            assert_eq!(
                (is_xml, spoken.as_str()),
                (expected_ssml, expected_text),
                "text={text:?} semitones={semitones} is_ssml={is_ssml} language={language:?}"
            );
        }
    }
}
