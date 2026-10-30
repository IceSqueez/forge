use std::fmt;

use serde::{Deserialize, Serialize, Serializer};

use crate::redaction::Redacted;

macro_rules! define_secret {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Clone, PartialEq, Eq, Hash, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn expose(&self) -> &str {
                &self.0
            }

            pub fn serialize_exposed<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(serializer)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), Redacted)
            }
        }
    )+};
}

define_secret!(
    Secret,
    OAuthToken,
    ClipCapability,
    OverlayCredential,
    RemoteClipId,
);

#[cfg(test)]
mod tests {
    use super::*;

    const SENTINEL: &str = "SENTINEL_SECRET";

    trait ReachesASerializer<Marker> {
        fn probe() {}
    }

    impl<T: ?Sized> ReachesASerializer<()> for T {}
    impl<T: ?Sized + Serialize> ReachesASerializer<u8> for T {}

    macro_rules! refuse_to_compile_if_serializable {
        ($($family:ty),+ $(,)?) => {$(
            const _: fn() = || {
                <$family as ReachesASerializer<_>>::probe();
            };
        )+};
    }

    refuse_to_compile_if_serializable!(
        Secret,
        OAuthToken,
        ClipCapability,
        OverlayCredential,
        RemoteClipId,
    );

    #[derive(Serialize, Deserialize)]
    struct StoredField {
        #[serde(serialize_with = "Secret::serialize_exposed")]
        value: Secret,
    }

    #[test]
    fn every_family_member_debug_names_its_type_around_the_bare_marker() {
        for (rendered, name) in [
            (format!("{:?}", Secret::new(SENTINEL)), "Secret"),
            (format!("{:?}", OAuthToken::new(SENTINEL)), "OAuthToken"),
            (
                format!("{:?}", ClipCapability::new(SENTINEL)),
                "ClipCapability",
            ),
            (
                format!("{:?}", OverlayCredential::new(SENTINEL)),
                "OverlayCredential",
            ),
            (format!("{:?}", RemoteClipId::new(SENTINEL)), "RemoteClipId"),
        ] {
            assert_eq!(rendered, format!("{name}(<redacted>)"));
        }
    }

    #[test]
    fn a_stored_field_keeps_the_exact_bytes_of_a_plain_string_across_escapes_and_unicode() {
        for stored in [
            r#"{"value":"plain"}"#,
            r#"{"value":""}"#,
            r#"{"value":"quote\" back\\ line\n"}"#,
            r#"{"value":"токен-🔑"}"#,
        ] {
            let parsed: StoredField = serde_json::from_str(stored).expect("a saved field loads");

            assert_eq!(serde_json::to_string(&parsed).expect("serialize"), stored);
        }
    }

    #[test]
    fn a_stored_field_that_is_not_a_string_is_refused() {
        for stored in [r#"{"value":42}"#, r#"{"value":null}"#, r#"{"value":["x"]}"#] {
            assert!(
                serde_json::from_str::<StoredField>(stored).is_err(),
                "{stored}"
            );
        }
    }
}
