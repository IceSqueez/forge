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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_takes_the_first_free_number_from_two() {
        for (taken, expected) in [
            (vec!["Alert"], "Alert (2)"),
            (vec!["Alert", "Alert (2)"], "Alert (3)"),
            (vec!["Alert", "Alert (3)"], "Alert (2)"),
            (vec!["Alert", "Alert (2)", "Alert (3)"], "Alert (4)"),
        ] {
            let mut book = NameBook::new(taken.iter().copied());
            assert_eq!(book.claim_copy("Alert"), expected, "taken: {taken:?}");
        }
    }

    #[test]
    fn a_claimed_copy_is_not_handed_out_twice() {
        let mut book = NameBook::new(["Alert"]);
        let first = book.claim_copy("Alert");
        let second = book.claim_copy("Alert");
        assert_ne!(first, second);
        assert!(book.is_taken(&first));
    }

    #[test]
    fn a_copy_of_a_numbered_name_numbers_the_whole_name() {
        let mut book = NameBook::new(["Alert (2)"]);
        assert_eq!(book.claim_copy("Alert (2)"), "Alert (2) (2)");
    }
}
