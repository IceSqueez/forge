use forge_registry::FormField;
use forge_types::{ArgStack, Variant};

use crate::catalog::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::config::{ICON_OPTIONS_KEY, IMAGE, SOUND, SOUND_OPTIONS_KEY};
use crate::files::media::ResolvedMedia;

pub const SHOW_IMAGE: &str = "show_image";
pub const SHOW_SOUND: &str = "show_sound";
pub const SHOW_MEDIA_DIRECTORY: &str = "forge-show-media";

const PAGE_PATH_SEPARATOR: char = '/';

struct ShowMediaSlot {
    show_key: &'static str,
    overlay_key: &'static str,
    label: &'static str,
    options_key: &'static str,
}

const SHOW_MEDIA_SLOTS: [ShowMediaSlot; 2] = [
    ShowMediaSlot {
        show_key: SHOW_IMAGE,
        overlay_key: IMAGE,
        label: "Image for this show",
        options_key: ICON_OPTIONS_KEY,
    },
    ShowMediaSlot {
        show_key: SHOW_SOUND,
        overlay_key: SOUND,
        label: "Sound for this show",
        options_key: SOUND_OPTIONS_KEY,
    },
];

fn offered_slots(
    descriptor: &dyn OverlayKindDescriptor,
) -> impl Iterator<Item = &'static ShowMediaSlot> {
    let fields = descriptor.config_fields();
    SHOW_MEDIA_SLOTS.iter().filter(move |slot| {
        fields
            .iter()
            .any(|sectioned| sectioned.field.key() == slot.overlay_key)
    })
}

pub fn show_media_fields(descriptor: &dyn OverlayKindDescriptor) -> Vec<FormField> {
    offered_slots(descriptor)
        .map(|slot| FormField::DynamicSelect {
            key: slot.show_key,
            label: slot.label,
            options_key: slot.options_key,
        })
        .collect()
}

pub fn show_media_overrides(
    descriptor: &dyn OverlayKindDescriptor,
    supplied: &OverlayConfig,
    args: &ArgStack,
) -> OverlayConfig {
    offered_slots(descriptor)
        .filter_map(|slot| {
            let picked = args.interpolate(supplied.get(slot.show_key)?.as_str()?);
            let picked = picked.trim();
            (!picked.is_empty()).then(|| {
                (
                    slot.overlay_key.to_owned(),
                    Variant::String(picked.to_owned()),
                )
            })
        })
        .collect()
}

pub fn show_media_key(overlay_key: &str) -> Option<&'static str> {
    SHOW_MEDIA_SLOTS
        .iter()
        .find(|slot| slot.overlay_key == overlay_key)
        .map(|slot| slot.show_key)
}

pub fn show_media_page_path(media: &ResolvedMedia) -> String {
    format!(
        "{SHOW_MEDIA_DIRECTORY}{PAGE_PATH_SEPARATOR}{}",
        media.file_name()
    )
}

pub fn without_show_media(content: &OverlayConfig) -> OverlayConfig {
    content
        .iter()
        .filter(|(key, _)| !SHOW_MEDIA_SLOTS.iter().any(|slot| slot.show_key == *key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
