use std::collections::BTreeSet;

const FIRST_COPY_NUMBER: u32 = 2;

#[derive(Debug, Default)]
pub(crate) struct NameBook {
    taken: BTreeSet<String>,
}

impl NameBook {
    pub(crate) fn new<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            taken: names.into_iter().map(str::to_owned).collect(),
        }
    }

    pub(crate) fn is_taken(&self, name: &str) -> bool {
        self.taken.contains(name)
    }

    pub(crate) fn claim(&mut self, name: &str) {
        self.taken.insert(name.to_owned());
    }

    pub(crate) fn claim_copy(&mut self, base: &str) -> String {
        let copy = (FIRST_COPY_NUMBER..=u32::MAX)
            .map(|number| format!("{base} ({number})"))
            .find(|candidate| !self.taken.contains(candidate))
            .unwrap_or_else(|| format!("{base} ({})", ulid::Ulid::generate()));
        self.claim(&copy);
        copy
    }
}
