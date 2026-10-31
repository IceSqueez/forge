#![cfg(unix)]
#![allow(clippy::panic, clippy::expect_used)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use forge_speech_types::VoiceId;
use forge_tts_core::{SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError};
use forge_tts_espeak::EspeakEngineFactory;

const SAMPLE_RATE: u32 = 22_050;
const SAMPLES: [i16; 3] = [1, -2, 300];
const STUB_VOICE: &str = "stub/voice";
const VOICES_LISTING: &str = " 5  en-us  --/F  Stub_Voice  stub/voice";

struct Stubs {
    _root: tempfile::TempDir,
    working: PathBuf,
    refuses_version: PathBuf,
    no_voice_listing: PathBuf,
    fails_synthesis: PathBuf,
}

fn wav_bytes() -> Vec<u8> {
    let data: Vec<u8> = SAMPLES.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    wav
}

fn write_program(dir: &Path, name: &str, version: &str, voices: &str, synthesis: &str) -> PathBuf {
    let path = dir.join(name);
    let script = format!(
        "#!/bin/sh\ncase \"$1\" in\n--version) {version} ;;\n--voices) {voices} ;;\n*) {synthesis} ;;\nesac\n"
    );
    std::fs::write(&path, script).expect("write the stub");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("mark the stub executable");
    path
}

fn stubs() -> &'static Stubs {
    static STUBS: OnceLock<Stubs> = OnceLock::new();
    STUBS.get_or_init(|| {
        let root = tempfile::tempdir().expect("a temp dir");
        let dir = root.path().join("program files");
        std::fs::create_dir(&dir).expect("a folder with a space");
        let wav = dir.join("speech.wav");
        std::fs::write(&wav, wav_bytes()).expect("write the wav");
        let list = format!("echo '{VOICES_LISTING}'");
        let speak = format!("cat > /dev/null; cat '{}'", wav.display());
        Stubs {
            working: write_program(&dir, "working", "exit 0", &list, &speak),
            refuses_version: write_program(&dir, "refuses", "exit 3", &list, &speak),
            no_voice_listing: write_program(&dir, "no-voices", "exit 0", "exit 4", &speak),
            fails_synthesis: write_program(
                &dir,
                "fails",
                "exit 0",
                &list,
                "cat > /dev/null; echo refused >&2; exit 5",
            ),
            _root: root,
        }
    })
}

fn engine_at(program: &Path) -> Box<dyn TtsEngine> {
    EspeakEngineFactory::new(program.to_path_buf())
        .create()
        .unwrap_or_else(|e| panic!("create: {e:?}"))
}

fn request(voice: &str) -> SynthesisRequest {
    SynthesisRequest {
        text: "hello".to_owned(),
        voice_id: VoiceId(voice.to_owned()),
        pitch_semitones: 0.0,
        rate_multiplier: 1.0,
        ssml: false,
    }
}

#[test]
fn a_program_that_cannot_answer_for_its_version_gives_no_engine() {
    let stubs = stubs();
    let missing = stubs.working.with_file_name("not-installed");

    for program in [&missing, &stubs.refuses_version] {
        let created = EspeakEngineFactory::new(program.clone()).create();
        assert!(
            matches!(created, Err(TtsError::EngineUnavailable { .. })),
            "{}",
            program.display()
        );
    }
}

#[tokio::test]
async fn the_voices_come_from_the_program_the_factory_was_given() {
    let voices = engine_at(&stubs().working)
        .list_voices()
        .await
        .expect("a listing");

    let ids: Vec<&str> = voices.iter().map(|v| v.id.0.as_str()).collect();
    assert_eq!(ids, [STUB_VOICE]);
}

#[tokio::test]
async fn the_speech_comes_from_the_program_the_factory_was_given() {
    let pcm = engine_at(&stubs().working)
        .synthesize(request(STUB_VOICE))
        .await
        .expect("speech");

    assert_eq!(
        (pcm.samples.as_slice(), pcm.sample_rate, pcm.channels),
        (SAMPLES.as_slice(), SAMPLE_RATE, 1)
    );
}

#[tokio::test]
async fn a_voice_the_program_does_not_list_is_refused_before_it_is_asked_to_speak() {
    let refused = engine_at(&stubs().fails_synthesis)
        .synthesize(request("other/voice"))
        .await;

    assert!(matches!(refused, Err(TtsError::InvalidVoice { .. })));
}

#[tokio::test]
async fn a_program_that_fails_to_speak_reports_its_own_words() {
    let refused = engine_at(&stubs().fails_synthesis)
        .synthesize(request(STUB_VOICE))
        .await;

    assert!(
        matches!(&refused, Err(TtsError::SynthesisFailed { detail, .. }) if detail.contains("refused")),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_program_that_cannot_list_voices_makes_the_engine_unavailable() {
    let listing = engine_at(&stubs().no_voice_listing).list_voices().await;

    assert!(matches!(listing, Err(TtsError::EngineUnavailable { .. })));
}
