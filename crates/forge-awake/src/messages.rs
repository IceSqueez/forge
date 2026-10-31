use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail};

use crate::error::{AwakeError, AwakeService};

const NAMESPACE: &str = "awake";

const SERVICE_ARG: &str = "service";

const SERVICE_MISSING: MessageCode = MessageCode::from_static(NAMESPACE, "service_missing");
const UNREACHABLE: MessageCode = MessageCode::from_static(NAMESPACE, "unreachable");
const REFUSED: MessageCode = MessageCode::from_static(NAMESPACE, "refused");
const UNSUPPORTED: MessageCode = MessageCode::from_static(NAMESPACE, "unsupported");
const STOPPED: MessageCode = MessageCode::from_static(NAMESPACE, "stopped");
const SERVICE_SESSION_BUS: MessageCode = MessageCode::from_static(NAMESPACE, "service_session_bus");
const SERVICE_SYSTEM_BUS: MessageCode = MessageCode::from_static(NAMESPACE, "service_system_bus");
const SERVICE_SCREEN_SAVER: MessageCode =
    MessageCode::from_static(NAMESPACE, "service_screen_saver");
const SERVICE_LOGIN_MANAGER: MessageCode =
    MessageCode::from_static(NAMESPACE, "service_login_manager");
const SERVICE_POWER_MANAGER: MessageCode =
    MessageCode::from_static(NAMESPACE, "service_power_manager");

pub const AWAKE_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: NAMESPACE,
    codes: &[
        SERVICE_MISSING,
        UNREACHABLE,
        REFUSED,
        UNSUPPORTED,
        STOPPED,
        SERVICE_SESSION_BUS,
        SERVICE_SYSTEM_BUS,
        SERVICE_SCREEN_SAVER,
        SERVICE_LOGIN_MANAGER,
        SERVICE_POWER_MANAGER,
    ],
    en_ftl: include_str!("../locales/en/messages.ftl"),
    uk_ftl: include_str!("../locales/uk/messages.ftl"),
};

impl AwakeService {
    fn name(self) -> MessageRef {
        MessageRef::new(match self {
            Self::SessionBus => SERVICE_SESSION_BUS,
            Self::SystemBus => SERVICE_SYSTEM_BUS,
            Self::ScreenSaver => SERVICE_SCREEN_SAVER,
            Self::LoginManager => SERVICE_LOGIN_MANAGER,
            Self::PowerManager => SERVICE_POWER_MANAGER,
        })
    }
}

impl AwakeError {
    pub fn message(&self) -> MessageRef {
        match self {
            Self::ServiceMissing { service } => {
                MessageRef::new(SERVICE_MISSING).with_arg(SERVICE_ARG, service.name())
            }
            Self::Unreachable { service, reason } => MessageRef::new(UNREACHABLE)
                .with_arg(SERVICE_ARG, service.name())
                .with_detail(UpstreamDetail::from_redacted(reason.as_str())),
            Self::Refused { service, reason } => MessageRef::new(REFUSED)
                .with_arg(SERVICE_ARG, service.name())
                .with_detail(UpstreamDetail::from_redacted(reason.as_str())),
            Self::Unsupported => MessageRef::new(UNSUPPORTED),
            Self::Stopped => MessageRef::new(STOPPED),
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_types::MessageArg;

    use super::*;

    #[test]
    fn a_failure_names_the_service_by_code_and_keeps_the_cause_as_detail() {
        let message = AwakeError::Refused {
            service: AwakeService::LoginManager,
            reason: "access denied".into(),
        }
        .message();

        assert_eq!(message.code(), &REFUSED);
        assert_eq!(
            message.arg(SERVICE_ARG),
            Some(&MessageArg::from(MessageRef::new(SERVICE_LOGIN_MANAGER)))
        );
        assert_eq!(message.detail().map(|d| d.as_str()), Some("access denied"));
    }

    #[test]
    fn a_failure_without_a_cause_has_no_detail() {
        for error in [
            AwakeError::ServiceMissing {
                service: AwakeService::ScreenSaver,
            },
            AwakeError::Unsupported,
            AwakeError::Stopped,
        ] {
            assert!(error.message().detail().is_none(), "{error:?}");
        }
    }

    #[test]
    fn the_catalog_is_well_formed_and_both_languages_cover_every_code() {
        assert!(AWAKE_MESSAGES.check().is_ok());
        for code in AWAKE_MESSAGES.codes {
            let key = format!("{} =", code.ftl_key());
            assert!(AWAKE_MESSAGES.en_ftl.contains(&key), "{code} missing in en");
            assert!(AWAKE_MESSAGES.uk_ftl.contains(&key), "{code} missing in uk");
        }
    }
}
