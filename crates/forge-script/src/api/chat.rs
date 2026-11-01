use super::*;

pub(super) fn coded_error(message: &MessageRef) -> Box<EvalAltResult> {
    coded_text(message).into()
}

pub(super) fn admit_broadcast(
    integrations: Option<&dyn IntegrationAvailability>,
) -> Result<(), Box<EvalAltResult>> {
    match integrations {
        Some(integrations) if !integrations.any_chat_platform_enabled() => {
            Err(coded_error(&MessageRef::new(NO_CHAT_PLATFORM_ENABLED)))
        }
        _ => Ok(()),
    }
}

pub(super) fn admit_chat_target(
    integrations: Option<&dyn IntegrationAvailability>,
    raw: &str,
) -> Result<Option<&'static str>, Box<EvalAltResult>> {
    let Some(platform) = resolve_chat_target(raw).map_err(|message| coded_error(&message))? else {
        admit_broadcast(integrations)?;
        return Ok(None);
    };
    let integration = IntegrationId::from_static(platform.as_str());
    if integrations.is_some_and(|integrations| integrations.is_disabled(&integration)) {
        return Err(coded_error(&integration_disabled_message(&integration)));
    }
    Ok(Some(platform.as_str()))
}

pub(super) fn admit_whisper_broadcast(
    integrations: Option<&dyn IntegrationAvailability>,
) -> Result<(), Box<EvalAltResult>> {
    admit_broadcast(integrations)?;
    match integrations {
        Some(integrations) if !integrations.any_whisper_platform_enabled() => {
            Err(coded_error(&MessageRef::new(NO_WHISPER_PLATFORM_ENABLED)))
        }
        _ => Ok(()),
    }
}

pub(super) fn admit_whisper_target(
    integrations: Option<&dyn IntegrationAvailability>,
    raw: &str,
) -> Result<Option<&'static str>, Box<EvalAltResult>> {
    let Some(platform) = resolve_chat_target(raw).map_err(|message| coded_error(&message))? else {
        admit_whisper_broadcast(integrations)?;
        return Ok(None);
    };
    if !platform.supports_whispers() {
        return Err(coded_error(&whispers_unsupported_message(
            platform.as_str(),
        )));
    }
    admit_chat_target(integrations, platform.as_str())
}

pub(super) fn script_sender(raw: &str) -> Result<SenderPreference, Box<EvalAltResult>> {
    raw.parse()
        .map_err(|error: PlatformError| error.to_string().into())
}

pub(super) fn chat_text(text: &str) -> ChatSendRequest {
    ChatSendRequest {
        message: text.to_owned(),
        ..ChatSendRequest::default()
    }
}

pub(super) fn chat_reply(to: &str, text: &str) -> ChatSendRequest {
    ChatSendRequest {
        reply_to_message_id: Some(to.to_owned()),
        ..chat_text(text)
    }
}

pub(super) fn chat_whisper(user: &str, text: &str) -> ChatSendRequest {
    ChatSendRequest {
        whisper_to_login: Some(user.to_owned()),
        ..chat_text(text)
    }
}

pub(super) fn aimed(target: Option<&'static str>, request: ChatSendRequest) -> ChatSendRequest {
    ChatSendRequest {
        target: target.map(str::to_owned),
        ..request
    }
}

pub(super) fn sent_by(sender: SenderPreference, request: ChatSendRequest) -> ChatSendRequest {
    ChatSendRequest {
        sender: (sender != SenderPreference::Default).then(|| sender.as_str().to_owned()),
        ..request
    }
}

#[derive(Clone)]
pub(super) struct ChatRequester {
    publisher: Arc<dyn EventPublisher>,
    caused_by: EventId,
    integrations: Option<Arc<dyn IntegrationAvailability>>,
}

impl ChatRequester {
    fn integrations(&self) -> Option<&dyn IntegrationAvailability> {
        self.integrations.as_deref()
    }

    fn request(&self, request: ChatSendRequest) -> Result<(), Box<EvalAltResult>> {
        request.publish(
            self.publisher.as_ref(),
            EventSource::Rhai,
            Some(self.caused_by),
        );
        Ok(())
    }
}

pub(super) fn build_chat_module(
    publisher: Arc<dyn EventPublisher>,
    caused_by: EventId,
    integrations: Option<Arc<dyn IntegrationAvailability>>,
) -> ApiModule {
    let mut m = ApiModule::default();
    let chat = ChatRequester {
        publisher,
        caused_by,
        integrations,
    };

    let send = chat.clone();
    m.register(
        ScriptFn {
            name: "send",
            params: ["text"],
            doc: None,
        },
        move |text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            admit_broadcast(send.integrations())?;
            send.request(chat_text(&text))
        },
    );

    let send = chat.clone();
    m.register(
        ScriptFn {
            name: "send",
            params: ["target", "text"],
            doc: Some(TARGET_DOC),
        },
        move |target: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let target = admit_chat_target(send.integrations(), &target)?;
            send.request(aimed(target, chat_text(&text)))
        },
    );

    let reply = chat.clone();
    m.register(
        ScriptFn {
            name: "reply",
            params: ["to", "text"],
            doc: None,
        },
        move |to: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            admit_broadcast(reply.integrations())?;
            reply.request(chat_reply(&to, &text))
        },
    );

    let reply = chat.clone();
    m.register(
        ScriptFn {
            name: "reply",
            params: ["target", "to", "text"],
            doc: Some(TARGET_DOC),
        },
        move |target: ImmutableString,
              to: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let target = admit_chat_target(reply.integrations(), &target)?;
            reply.request(aimed(target, chat_reply(&to, &text)))
        },
    );

    let whisper = chat.clone();
    m.register(
        ScriptFn {
            name: "whisper",
            params: ["user", "text"],
            doc: None,
        },
        move |user: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            admit_whisper_broadcast(whisper.integrations())?;
            whisper.request(chat_whisper(&user, &text))
        },
    );

    let whisper = chat.clone();
    m.register(
        ScriptFn {
            name: "whisper",
            params: ["target", "user", "text"],
            doc: Some(TARGET_DOC),
        },
        move |target: ImmutableString,
              user: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let target = admit_whisper_target(whisper.integrations(), &target)?;
            whisper.request(aimed(target, chat_whisper(&user, &text)))
        },
    );

    register_sender_chat_fns(&mut m, chat);

    m
}

pub(super) fn register_sender_chat_fns(m: &mut ApiModule, chat: ChatRequester) {
    let send = chat.clone();
    m.register(
        ScriptFn {
            name: "send_as",
            params: ["sender", "text"],
            doc: Some(SENDER_DOC),
        },
        move |sender: ImmutableString, text: ImmutableString| -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            admit_broadcast(send.integrations())?;
            send.request(sent_by(sender, chat_text(&text)))
        },
    );

    let send = chat.clone();
    m.register(
        ScriptFn {
            name: "send_as",
            params: ["sender", "target", "text"],
            doc: Some(SENDER_TARGET_DOC),
        },
        move |sender: ImmutableString,
              target: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            let target = admit_chat_target(send.integrations(), &target)?;
            send.request(sent_by(sender, aimed(target, chat_text(&text))))
        },
    );

    let reply = chat.clone();
    m.register(
        ScriptFn {
            name: "reply_as",
            params: ["sender", "to", "text"],
            doc: Some(SENDER_DOC),
        },
        move |sender: ImmutableString,
              to: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            admit_broadcast(reply.integrations())?;
            reply.request(sent_by(sender, chat_reply(&to, &text)))
        },
    );

    let reply = chat.clone();
    m.register(
        ScriptFn {
            name: "reply_as",
            params: ["sender", "target", "to", "text"],
            doc: Some(SENDER_TARGET_DOC),
        },
        move |sender: ImmutableString,
              target: ImmutableString,
              to: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            let target = admit_chat_target(reply.integrations(), &target)?;
            reply.request(sent_by(sender, aimed(target, chat_reply(&to, &text))))
        },
    );

    let whisper = chat.clone();
    m.register(
        ScriptFn {
            name: "whisper_as",
            params: ["sender", "user", "text"],
            doc: Some(SENDER_DOC),
        },
        move |sender: ImmutableString,
              user: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            admit_whisper_broadcast(whisper.integrations())?;
            whisper.request(sent_by(sender, chat_whisper(&user, &text)))
        },
    );

    let whisper = chat;
    m.register(
        ScriptFn {
            name: "whisper_as",
            params: ["sender", "target", "user", "text"],
            doc: Some(SENDER_TARGET_DOC),
        },
        move |sender: ImmutableString,
              target: ImmutableString,
              user: ImmutableString,
              text: ImmutableString|
              -> Result<(), Box<EvalAltResult>> {
            let sender = script_sender(&sender)?;
            let target = admit_whisper_target(whisper.integrations(), &target)?;
            whisper.request(sent_by(sender, aimed(target, chat_whisper(&user, &text))))
        },
    );
}
