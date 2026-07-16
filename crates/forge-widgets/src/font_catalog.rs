use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamily {
    pub name: String,
    pub monospaced: bool,
}

/// Parses every installed font file (easily >100 ms) - run off the render thread.
pub fn enumerate_font_families() -> Vec<FontFamily> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    for bytes in crate::tokens::load_fonts() {
        db.load_font_data(bytes.into_owned());
    }

    // A family counts as monospaced when any of its faces carries the flag -
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_always_contains_bundled_families_sorted_by_name() {
        // Bundled fonts are loaded explicitly, so these hold on any system.
        let families = enumerate_font_families();
        let inter = families.iter().find(|f| f.name == "Inter");
        let mono = families.iter().find(|f| f.name == "JetBrains Mono");
        assert!(inter.is_some(), "bundled Inter missing from catalog");
        assert!(
            mono.is_some_and(|f| f.monospaced),
            "bundled JetBrains Mono missing or not flagged monospaced"
        );
        assert!(
            families.windows(2).all(|w| w[0].name <= w[1].name),
            "catalog must be sorted for the picker"
        );
    }
}
