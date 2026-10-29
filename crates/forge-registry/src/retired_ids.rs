use std::collections::BTreeMap;

use crate::error::RegistryError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdSuccession {
    pub current: &'static str,
    pub retired: &'static [&'static str],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RetiredIds {
    current_by_retired: BTreeMap<String, String>,
}

impl RetiredIds {
    pub(crate) fn declare(&mut self, succession: &IdSuccession) -> Result<(), RegistryError> {
        for retired in succession.retired {
            if self.current_by_retired.contains_key(*retired) {
                return Err(RegistryError::DuplicateId((*retired).to_owned()));
            }
        }
        for retired in succession.retired {
            self.current_by_retired
                .insert((*retired).to_owned(), succession.current.to_owned());
        }
        Ok(())
    }

    pub(crate) fn current_id(&self, id: &str) -> Option<&str> {
        self.current_by_retired.get(id).map(String::as_str)
    }

    pub(crate) fn successions(&self) -> impl Iterator<Item = (&str, &str)> {
        self.current_by_retired
            .iter()
            .map(|(retired, current)| (retired.as_str(), current.as_str()))
    }
}
