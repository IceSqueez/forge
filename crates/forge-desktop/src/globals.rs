use forge_components::ForgePalette;
use forge_types::{Variant, VariantKind};
use gpui::{Rgba, SharedString};

/// Semantic ink for each of the seven `Variant` kinds — the fixed per-kind color
/// contract (each value kind carries one stable hue across the UI). Resolved from
/// the active theme so a kind pill re-tints on theme switch. The seven arms map
/// int→info, float→peach(bits), bool→random, string→success, datetime→teal,
/// array→brand, object→pink, matching the design's type legend.
pub fn variant_kind_color(kind: VariantKind, palette: &ForgePalette) -> Rgba {
    match kind {
        VariantKind::Int => palette.info,
        VariantKind::Float => palette.bits,
        VariantKind::Bool => palette.random,
        VariantKind::String => palette.success,
        VariantKind::Datetime => palette.accent_teal,
        VariantKind::Array => palette.brand,
        VariantKind::Object => palette.accent_pink_light,
    }
}

/// One presentation row of the globals manager: a named value plus its persistence
/// flag and read/write telemetry and a human "last modified" caption. Carries a
/// real [`Variant`] as its value so the kind, the semantic color and the preview
/// text all derive from the single core value type (the seven-kind invariant), NOT
/// a stringly-typed duplicate. A cached read folded from a `GlobalsRepo::list` pull;
/// the storage provider is the source of truth.
#[derive(Clone, Debug)]
pub struct Global {
    pub name: SharedString,
    pub value: Variant,
    pub persisted: bool,
    pub reads: u64,
    pub writes: u64,
    /// Human "last modified" caption (e.g. "2 min ago"), pre-formatted at pull time.
    pub modified: SharedString,
}

impl Global {
    pub fn kind(&self) -> VariantKind {
        VariantKind::from_variant(&self.value)
    }
}

/// Which slice of the manager a filter tab keeps. `All` passes everything;
/// `Persisted` keeps only rows written through to storage; `Session` keeps the
/// in-memory-only rows. A pure predicate over `Global::persisted`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum GlobalsFilter {
    #[default]
    All,
    Persisted,
    Session,
}

impl GlobalsFilter {
    /// Whether `global` survives this filter.
    pub fn keeps(self, global: &Global) -> bool {
        match self {
            GlobalsFilter::All => true,
            GlobalsFilter::Persisted => global.persisted,
            GlobalsFilter::Session => !global.persisted,
        }
    }
}

/// Observable entity holding the manager's rows — a cached read folded from the
/// storage provider's `list`. Owns no runtime state: the screen reconciles it by a
/// full re-pull after every write, so it never holds a view-minted placeholder.
pub struct Globals {
    entries: Vec<Global>,
}

impl Globals {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Replaces every row with a freshly pulled set, sorted by name ascending (the
    /// manager's fixed sort). The reconcile sink for a `list` re-pull.
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

    /// True when a row with `name` already exists — the create/rename collision
    /// guard.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|g| g.name.as_ref() == name)
    }
}
