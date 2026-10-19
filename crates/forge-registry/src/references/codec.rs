use crate::references::namespace::{
    ExternalNamespace, InternalNamespace, LOCAL_FILE_NAMESPACE, ReferenceNamespace,
};

pub const OVERLAY_SOUND_OPTIONS_KEY: &str = "soundboard.clips";
pub const OVERLAY_ICON_OPTIONS_KEY: &str = "overlay.icons";
pub const CLIP_REFERENCE_PREFIX: &str = "clip:";
pub const IMAGE_REFERENCE_PREFIX: &str = "image:";

pub(crate) const UNTAGGED: &str = "";

pub(crate) struct DecodedReference<'a> {
    pub(crate) namespace: ReferenceNamespace,
    pub(crate) value: &'a str,
    pub(crate) tag: &'static str,
}

pub(crate) fn decode<'a>(options_key: &str, raw: &'a str) -> Option<DecodedReference<'a>> {
    if raw.is_empty() {
        return None;
    }
    let decoded = match options_key {
        OVERLAY_SOUND_OPTIONS_KEY => match raw.strip_prefix(CLIP_REFERENCE_PREFIX) {
            Some(clip) => tagged(
                InternalNamespace::SoundboardClip,
                clip,
                CLIP_REFERENCE_PREFIX,
            ),
            None => DecodedReference {
                namespace: ReferenceNamespace::External(ExternalNamespace::new(
                    LOCAL_FILE_NAMESPACE,
                )),
                value: raw,
                tag: UNTAGGED,
            },
        },
        OVERLAY_ICON_OPTIONS_KEY => tagged(
            InternalNamespace::ManagedMedia,
            raw.strip_prefix(IMAGE_REFERENCE_PREFIX)?,
            IMAGE_REFERENCE_PREFIX,
        ),
        _ => DecodedReference {
            namespace: ReferenceNamespace::for_options_key(options_key),
            value: raw,
            tag: UNTAGGED,
        },
    };
    (!decoded.value.is_empty()).then_some(decoded)
}

fn tagged<'a>(
    namespace: InternalNamespace,
    value: &'a str,
    tag: &'static str,
) -> DecodedReference<'a> {
    DecodedReference {
        namespace: ReferenceNamespace::Internal(namespace),
        value,
        tag,
    }
}
