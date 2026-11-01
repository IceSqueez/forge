use forge_components::{ForgePalette, Icon, tr};
use forge_soundboard::SoundCategory;
use gpui::{Rgba, SharedString};

use super::pads::SoundboardPads;

impl SoundboardPads {
    pub(super) fn categories_present(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for cat in SoundCategory::ALL {
            if self.clips.iter().any(|c| c.category == cat.as_str()) {
                seen.push(cat.as_str().to_owned());
            }
        }
        for clip in &self.clips {
            if !clip.category.is_empty() && !seen.iter().any(|c| c == &clip.category) {
                seen.push(clip.category.clone());
            }
        }
        seen
    }
}

struct CategoryLook {
    color: fn(&ForgePalette) -> Rgba,
    label: fn() -> String,
    glyph: Icon,
}

fn look_of(cat: SoundCategory) -> CategoryLook {
    match cat {
        SoundCategory::Memes => CategoryLook {
            color: |palette| palette.accent_peach,
            label: || tr!("soundboard_category_memes"),
            glyph: Icon::MoodSmile,
        },
        SoundCategory::Alerts => CategoryLook {
            color: |palette| palette.danger,
            label: || tr!("soundboard_category_alerts"),
            glyph: Icon::Star,
        },
        SoundCategory::Music => CategoryLook {
            color: |palette| palette.theme_brand,
            label: || tr!("soundboard_category_music"),
            glyph: Icon::Music,
        },
        SoundCategory::Voice => CategoryLook {
            color: |palette| palette.info,
            label: || tr!("soundboard_category_voice"),
            glyph: Icon::MessageCircle,
        },
    }
}

fn look_of_token(cat: &str) -> Option<CategoryLook> {
    SoundCategory::from_token(cat).map(look_of)
}

pub(crate) fn category_color(cat: &str, palette: &ForgePalette) -> Rgba {
    look_of_token(cat).map_or(palette.text_muted, |look| (look.color)(palette))
}

pub(crate) fn category_label(cat: &str) -> SharedString {
    look_of_token(cat)
        .map_or_else(|| cat.to_owned(), |look| (look.label)())
        .into()
}

pub(super) fn category_glyph(cat: SoundCategory) -> Icon {
    look_of(cat).glyph
}
