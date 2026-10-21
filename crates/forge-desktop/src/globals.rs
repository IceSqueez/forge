use forge_components::ForgePalette;
use forge_types::{Variant, VariantKind};
use gpui::{Rgba, SharedString};

pub fn variant_kind_color(kind: VariantKind, palette: &ForgePalette) -> Rgba {
    match kind {
        VariantKind::Int => palette.info,
        VariantKind::Float => palette.bits,
        VariantKind::Bool => palette.random,
        VariantKind::String => palette.success,
        VariantKind::Datetime => palette.accent_teal,
        VariantKind::Array => palette.theme_brand,
        VariantKind::Object => palette.accent_pink_light,
    }
}

#[derive(Clone, Debug)]
pub struct Global {
    pub name: SharedString,
    pub value: Variant,
    pub persisted: bool,
    pub reads: u64,
    pub writes: u64,
    pub modified: SharedString,
}

impl Global {
    pub fn kind(&self) -> VariantKind {
        VariantKind::from_variant(&self.value)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum GlobalsFilter {
    #[default]
    All,
    Persisted,
    Session,
}

impl GlobalsFilter {
    pub fn keeps(self, global: &Global) -> bool {
        match self {
            GlobalsFilter::All => true,
            GlobalsFilter::Persisted => global.persisted,
            GlobalsFilter::Session => !global.persisted,
        }
    }
}

pub struct Globals {
    entries: Vec<Global>,
}

impl Globals {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn set_all(&mut self, mut entries: Vec<Global>) {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        self.entries = entries;
    }

    pub fn entries(&self) -> &[Global] {
        &self.entries
    }

    pub fn total(&self) -> usize {
        self.entries.len()
    }

    pub fn persisted_count(&self) -> usize {
        self.entries.iter().filter(|g| g.persisted).count()
    }

    pub fn session_count(&self) -> usize {
        self.total() - self.persisted_count()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|g| g.name.as_ref() == name)
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{AccentColor, ThemeId};
    use forge_types::VariantKind;

    use super::variant_kind_color;

    const KINDS: [VariantKind; 7] = [
        VariantKind::Int,
        VariantKind::Float,
        VariantKind::Bool,
        VariantKind::String,
        VariantKind::Datetime,
        VariantKind::Array,
        VariantKind::Object,
    ];

    #[test]
    fn variant_type_colors_do_not_follow_the_accent() {
        for theme in ThemeId::ALL {
            let base = theme.palette();
            for accent in AccentColor::ALL {
                let accented = theme.accented_palette(accent);
                for kind in KINDS {
                    assert_eq!(
                        variant_kind_color(kind, &accented),
                        variant_kind_color(kind, &base),
                        "{theme:?} {accent:?} {kind:?}"
                    );
                }
            }
        }
    }
}
