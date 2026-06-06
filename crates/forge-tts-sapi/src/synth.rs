#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn rate_adj_from_multiplier(multiplier: f32) -> i32 {
    ((multiplier.log2() * 10.0).round() as i32).clamp(-10, 10)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn pitch_pct_from_semitones(semitones: f32) -> i32 {
    ((2.0_f32.powf(semitones / 12.0) - 1.0) * 100.0).round() as i32
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn wrap_with_pitch_prosody(text: &str, pitch_pct: i32) -> String {
    format!("<speak><prosody pitch=\"{pitch_pct:+}%\">{text}</prosody></speak>")
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn prepare_speak_text(
    text: &str,
    pitch_semitones: f32,
    is_ssml: bool,
) -> (String, bool) {
    if is_ssml {
        (text.to_owned(), true)
    } else if pitch_semitones.abs() > f32::EPSILON {
        let pct = pitch_pct_from_semitones(pitch_semitones);
        (wrap_with_pitch_prosody(text, pct), true)
    } else {
        (text.to_owned(), false)
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
pub(crate) fn capture_pcm(
    voice: &windows::Win32::Media::Speech::ISpVoice,
    text: &str,
    use_xml: bool,
) -> Result<forge_audio::PcmBuffer, crate::error::SapiError> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::core::BOOL;
    use windows::Win32::Media::Audio::WAVEFORMATEX;
    use windows::Win32::Media::Speech::{ISpStream, SPF_DEFAULT, SPF_PARSE_SSML, SpStream};
    use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, IStream, STREAM_SEEK_SET,
    };

    const SPDFID_WAVE_FORMAT_EX: windows::core::GUID =
        windows::core::GUID::from_u128(0xc31adbae_527f_4ff5_a230_f62bb61ff70c);

    // SAFETY: CreateStreamOnHGlobal allocates an in-memory COM stream. HGLOBAL(null)
    // instructs COM to allocate the global memory itself. fDeleteOnRelease=BOOL(1) frees the
    // HGLOBAL when the IStream's reference count reaches zero. All operations on the
    // returned stream occur on this STA thread before the stream is released.
    let com_stream: IStream =
        unsafe { CreateStreamOnHGlobal(HGLOBAL(std::ptr::null_mut()), BOOL(1)) }
            .map_err(|e| crate::error::SapiError::ComInit(e.code().0))?;

    let wfex = WAVEFORMATEX {
        wFormatTag: 1,
        nChannels: 1,
        nSamplesPerSec: 22_050,
        nAvgBytesPerSec: 44_100,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };

    // SAFETY: CoCreateInstance is called on the STA thread. The returned ISpStream
    // is used and dropped here before any other COM call on the STA thread.
    let sp_stream: ISpStream = unsafe { CoCreateInstance(&SpStream, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| crate::error::SapiError::ComInit(e.code().0))?;

    // SAFETY: SetBaseStream binds the IStream to the ISpStream with a fixed PCM format.
    // Both pointers are valid for the duration of this function on the STA thread.
    unsafe { sp_stream.SetBaseStream(&com_stream, &SPDFID_WAVE_FORMAT_EX, &wfex) }
        .map_err(|e| crate::error::SapiError::ComInit(e.code().0))?;

    // SAFETY: SetOutput directs synthesized audio to sp_stream. voice and sp_stream
    // are both valid COM objects on this STA thread.
    unsafe { voice.SetOutput(&sp_stream, false) }
        .map_err(|e| crate::error::SapiError::ComInit(e.code().0))?;

    let speak_flags = if use_xml { SPF_PARSE_SSML } else { SPF_DEFAULT };
    let text_utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let pcwstr = windows::core::PCWSTR(text_utf16.as_ptr());

    // SAFETY: Speak takes a null-terminated UTF-16 string, which text_utf16 provides.
    // The Vec remains alive for the duration of this call. SPF_DEFAULT means synchronous
    // synthesis — Speak blocks until all audio is written to sp_stream.
    unsafe { voice.Speak(pcwstr, speak_flags.0 as u32, None) }
        .map_err(|e| crate::error::SapiError::Speak(e.code().0))?;

    // SAFETY: Seek repositions the COM stream to the beginning so we can read the PCM bytes.
    unsafe { com_stream.Seek(0i64, STREAM_SEEK_SET, None) }
        .map_err(|e| crate::error::SapiError::ComInit(e.code().0))?;

    let mut pcm_bytes = Vec::<u8>::new();
    let mut buf = [0u8; 4096];
    loop {
        let mut read: u32 = 0;
        // SAFETY: Read fills `buf` up to buf.len() bytes and writes the actual count into `read`.
        let hr =
            unsafe { com_stream.Read(buf.as_mut_ptr().cast(), buf.len() as u32, Some(&mut read)) };
        if hr.is_err() || read == 0 {
            break;
        }
        pcm_bytes.extend_from_slice(&buf[..read as usize]);
    }

    let samples: Vec<i16> = pcm_bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();

    Ok(forge_audio::PcmBuffer::new(samples, 22_050, 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_adj_neutral() {
        assert_eq!(rate_adj_from_multiplier(1.0), 0);
    }

    #[test]
    fn rate_adj_double_speed() {
        assert_eq!(rate_adj_from_multiplier(2.0), 10);
    }

    #[test]
    fn rate_adj_half_speed() {
        assert_eq!(rate_adj_from_multiplier(0.5), -10);
    }

    #[test]
    fn rate_adj_quarter_speed_clamped() {
        assert_eq!(rate_adj_from_multiplier(0.25), -10);
    }

    #[test]
    fn rate_adj_quad_speed_clamped() {
        assert_eq!(rate_adj_from_multiplier(4.0), 10);
    }

    #[test]
    fn pitch_pct_neutral() {
        assert_eq!(pitch_pct_from_semitones(0.0), 0);
    }

    #[test]
    fn pitch_pct_octave_up() {
        assert_eq!(pitch_pct_from_semitones(12.0), 100);
    }

    #[test]
    fn pitch_pct_octave_down() {
        assert_eq!(pitch_pct_from_semitones(-12.0), -50);
    }

    #[test]
    fn pitch_pct_six_semitones_up() {
        let pct = pitch_pct_from_semitones(6.0);
        assert!(pct > 30 && pct < 50, "expected ~41, got {pct}");
    }

    #[test]
    fn pitch_pct_six_semitones_down() {
        let pct = pitch_pct_from_semitones(-6.0);
        assert!(pct < -20 && pct > -40, "expected ~-29, got {pct}");
    }

    #[test]
    fn wrap_prosody_positive() {
        let xml = wrap_with_pitch_prosody("hello", 200);
        assert_eq!(
            xml,
            r#"<speak><prosody pitch="+200%">hello</prosody></speak>"#
        );
    }

    #[test]
    fn wrap_prosody_negative() {
        let xml = wrap_with_pitch_prosody("world", -50);
        assert_eq!(
            xml,
            r#"<speak><prosody pitch="-50%">world</prosody></speak>"#
        );
    }

    #[test]
    fn prepare_plain_no_pitch_passthrough() {
        let (text, use_xml) = prepare_speak_text("hello", 0.0, false);
        assert_eq!(text, "hello");
        assert!(!use_xml);
    }

    #[test]
    fn prepare_ssml_passthrough() {
        let ssml = "<speak>hello</speak>";
        let (text, use_xml) = prepare_speak_text(ssml, 0.0, true);
        assert_eq!(text, ssml);
        assert!(use_xml);
    }

    #[test]
    fn prepare_plain_with_pitch_wraps_ssml() {
        let (text, use_xml) = prepare_speak_text("hi", 12.0, false);
        assert!(use_xml);
        assert!(text.contains("<speak>"));
        assert!(text.contains("<prosody pitch=\"+100%\">hi</prosody>"));
    }
}
