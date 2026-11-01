use super::*;

pub(super) fn build_tts_module(requester: Option<Arc<dyn SpeakRequester>>) -> ApiModule {
    let mut m = ApiModule::default();

    let r_speak = requester.clone();
    m.register(
        ScriptFn {
            name: "speak",
            params: ["text"],
            doc: None,
        },
        move |text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(available(&r_speak, SPEAK_UNAVAILABLE_REASON)?);
            let text_owned = text.to_string();
            Handle::current()
                .block_on(async move { r.speak(text_owned, None).await })
                .map_err(|e| -> Box<EvalAltResult> { format!("tts::speak: {e}").into() })
        },
    );

    let r_speak_as = requester.clone();
    m.register(
        ScriptFn {
            name: "speak_as",
            params: ["voice_id", "text"],
            doc: None,
        },
        move |voice: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(available(&r_speak_as, SPEAK_UNAVAILABLE_REASON)?);
            let voice_owned = voice.to_string();
            let text_owned = text.to_string();
            Handle::current()
                .block_on(async move { r.speak(text_owned, Some(voice_owned)).await })
                .map_err(|e| -> Box<EvalAltResult> { format!("tts::speak_as: {e}").into() })
        },
    );

    let r_skip = requester.clone();
    m.register(
        ScriptFn {
            name: "skip",
            params: [],
            doc: None,
        },
        move || -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(available(&r_skip, SPEAK_UNAVAILABLE_REASON)?);
            Handle::current().block_on(async move { r.skip().await });
            Ok(())
        },
    );

    let r_clear = requester;
    m.register(
        ScriptFn {
            name: "clear",
            params: [],
            doc: None,
        },
        move || -> Result<(), Box<EvalAltResult>> {
            let r = Arc::clone(available(&r_clear, SPEAK_UNAVAILABLE_REASON)?);
            Handle::current().block_on(async move { r.clear().await });
            Ok(())
        },
    );

    m
}
