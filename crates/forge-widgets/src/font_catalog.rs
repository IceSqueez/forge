use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamily {
    pub name: String,
    pub monospaced: bool,
}

/// Parses every installed font file (easily >100 ms) — run off the render thread.
pub fn enumerate_font_families() -> Vec<FontFamily> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    for bytes in crate::tokens::load_fonts() {
        db.load_font_data(bytes.into_owned());
    }

    // A family counts as monospaced when any of its faces carries the flag —
    // italic and variable faces are often left unflagged in font metadata.
    let mut families: BTreeMap<String, bool> = BTreeMap::new();
    for face in db.faces() {
        if let Some((name, _)) = face.families.first() {
            let monospaced = families.entry(name.clone()).or_insert(false);
            *monospaced = *monospaced || face.monospaced;
        }
    }

    families
        .into_iter()
        .map(|(name, monospaced)| FontFamily { name, monospaced })
        .collect()
}
