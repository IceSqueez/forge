use std::collections::BTreeMap;

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
/// a stringly-typed duplicate. This is a cached read stub — the real rows arrive
/// from the storage provider later; the topic seeds a representative sample so the
/// screen renders before any provider is wired.
#[derive(Clone, Debug)]
pub struct Global {
    pub name: SharedString,
    pub value: Variant,
    pub persisted: bool,
    pub reads: u32,
    pub writes: u32,
    /// Human "last modified" caption (e.g. "2 min ago"). A static display string
    /// in this stub; the provider will supply a real timestamp to format later.
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

/// Topic-scoped observable entity holding the manager's rows. Owns no runtime
/// state — only the rows it has been handed. Seeded at boot with a representative
/// sample spanning all seven `Variant` kinds so the screen renders visibly before
/// a storage provider is wired; edits mutate this in-memory cache directly (the
/// stub has no persistence — real writes go through the provider once it lands).
pub struct Globals {
    entries: Vec<Global>,
}

impl Globals {
    /// A representative starter set spanning the seven kinds and both persistence
    /// states (int, string, bool, float, array, object, datetime — a couple
    /// session-only). Clearly a stub; real rows stream from the storage provider.
    pub fn seeded() -> Self {
        let mut death_counters = BTreeMap::new();
        death_counters.insert("creeper".to_owned(), Variant::Int(12));
        death_counters.insert("lava".to_owned(), Variant::Int(7));
        death_counters.insert("fall".to_owned(), Variant::Int(3));
        death_counters.insert("mob".to_owned(), Variant::Int(1));

        let started_at = time::OffsetDateTime::parse(
            "2026-05-16T12:08:32Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);

        let entries = vec![
            Global {
                name: "quoteCounter".into(),
                value: Variant::Int(47),
                persisted: true,
                reads: 142,
                writes: 47,
                modified: "2 min ago".into(),
            },
            Global {
                name: "currentGame".into(),
                value: Variant::String("GTNH 2.8.4".to_owned()),
                persisted: true,
                reads: 38,
                writes: 3,
                modified: "14 min ago".into(),
            },
            Global {
                name: "streamLive".into(),
                value: Variant::Bool(true),
                persisted: true,
                reads: 284,
                writes: 2,
                modified: "2h 14m ago".into(),
            },
            Global {
                name: "lastViewer".into(),
                value: Variant::String("haash_".to_owned()),
                persisted: false,
                reads: 93,
                writes: 93,
                modified: "22s ago".into(),
            },
            Global {
                name: "soHistory".into(),
                value: Variant::Array((0..12).map(Variant::Int).collect()),
                persisted: true,
                reads: 28,
                writes: 12,
                modified: "1h 47m ago".into(),
            },
            Global {
                name: "avgChatRate".into(),
                value: Variant::Float(3.42),
                persisted: false,
                reads: 812,
                writes: 812,
                modified: "8s ago".into(),
            },
            Global {
                name: "deathCounters".into(),
                value: Variant::Object(death_counters),
                persisted: true,
                reads: 156,
                writes: 23,
                modified: "31 min ago".into(),
            },
            Global {
                name: "streamStartedAt".into(),
                value: Variant::Datetime(started_at),
                persisted: true,
                reads: 47,
                writes: 1,
                modified: "2h 14m ago".into(),
            },
            Global {
                name: "subTrain".into(),
                value: Variant::Int(0),
                persisted: true,
                reads: 18,
                writes: 8,
                modified: "2h ago".into(),
            },
            Global {
                name: "pendingTTS".into(),
                value: Variant::Array((0..3).map(Variant::Int).collect()),
                persisted: false,
                reads: 201,
                writes: 201,
                modified: "5s ago".into(),
            },
        ];
        Self { entries }
    }

    /// The rows, sorted by name ascending (the manager's fixed sort).
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

    /// Flips a row's persistence flag. A no-op if `name` is unknown.
    pub fn set_persisted(&mut self, name: &str, persisted: bool) {
        if let Some(g) = self.entries.iter_mut().find(|g| g.name.as_ref() == name) {
            g.persisted = persisted;
        }
    }

    /// Renames a row in place (telemetry survives), keeping the list sorted.
    /// Returns `false` — a no-op — when `new` is empty, unchanged, collides with a
    /// different row, or `old` is unknown.
    pub fn rename(&mut self, old: &str, new: &str) -> bool {
        let new = new.trim();
        if new.is_empty() || new == old {
            return false;
        }
        if self.contains(new) {
            return false;
        }
        let Some(g) = self.entries.iter_mut().find(|g| g.name.as_ref() == old) else {
            return false;
        };
        g.name = new.to_owned().into();
        self.sort();
        true
    }

    /// Removes and returns the row named `name` (the undo payload), or `None` when
    /// unknown.
    pub fn delete(&mut self, name: &str) -> Option<Global> {
        let idx = self.entries.iter().position(|g| g.name.as_ref() == name)?;
        Some(self.entries.remove(idx))
    }

    /// Creates a new row, keeping the list sorted. Overwrites any existing row of
    /// the same name (the create path guards duplicates before calling this).
    pub fn create(&mut self, name: &str, value: Variant, persisted: bool) {
        self.entries.retain(|g| g.name.as_ref() != name);
        self.entries.push(Global {
            name: name.to_owned().into(),
            value,
            persisted,
            reads: 0,
            writes: 0,
            modified: "just now".into(),
        });
        self.sort();
    }

    /// Updates an existing row's value + persistence in place (an edit), bumping
    /// its write counter and refreshing its caption. `old` may differ from `name`
    /// when the edit also renames; the row keeps its telemetry. A no-op if neither
    /// `old` nor `name` resolves to a row.
    pub fn update(&mut self, old: &str, name: &str, value: Variant, persisted: bool) {
        let Some(g) = self
            .entries
            .iter_mut()
            .find(|g| g.name.as_ref() == old || g.name.as_ref() == name)
        else {
            return;
        };
        g.name = name.to_owned().into();
        g.value = value;
        g.persisted = persisted;
        g.writes = g.writes.saturating_add(1);
        g.modified = "just now".into();
        self.sort();
    }

    fn sort(&mut self) {
        self.entries.sort_by(|a, b| a.name.cmp(&b.name));
    }
}
