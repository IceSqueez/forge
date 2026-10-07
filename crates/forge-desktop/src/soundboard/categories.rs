use forge_components::{ForgePalette, Icon, tr};
use gpui::{Rgba, SharedString};

use super::SoundboardView;

pub(crate) const CATEGORY_ORDER: &[&str] = &["memes", "alerts", "music", "voice"];

impl SoundboardView {
    pub(super) fn categories_present(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for cat in CATEGORY_ORDER {
            if self.clips.iter().any(|c| c.category == *cat) {
                seen.push((*cat).to_owned());
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

pub(crate) fn category_color(cat: &str, palette: &ForgePalette) -> Rgba {
    match cat {
        "memes" => palette.bits,
        "alerts" => palette.random,
        "music" => palette.brand,
        "voice" => palette.info,
        _ => palette.text_muted,
    }
}

pub(crate) fn category_label(cat: &str) -> SharedString {
    match cat {
        "memes" => tr!("soundboard_category_memes").into(),
        "alerts" => tr!("soundboard_category_alerts").into(),
        "music" => tr!("soundboard_category_music").into(),
        "voice" => tr!("soundboard_category_voice").into(),
        other => other.to_owned().into(),
    }
}

pub(super) fn category_glyph(cat: &str) -> Icon {
    match cat {
        "memes" => Icon::MoodSmile,
        "alerts" => Icon::Star,
        "music" => Icon::Music,
        "voice" => Icon::MessageCircle,
        _ => Icon::Music,
    }
}
