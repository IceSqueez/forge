use crate::error::TypesError;
use std::collections::BTreeSet;

use crate::message::code::{MessageCode, is_code_segment};

#[derive(Debug, Clone, Copy)]
pub struct MessageCatalog {
    pub namespace: &'static str,
    pub codes: &'static [MessageCode],
    pub en_ftl: &'static str,
    pub uk_ftl: &'static str,
}

impl MessageCatalog {
    pub fn check(&self) -> Result<(), TypesError> {
        if !is_code_segment(self.namespace) {
            return Err(TypesError::MalformedNamespace(self.namespace));
        }
        let mut seen = BTreeSet::new();
        for code in self.codes {
            if !code.is_well_formed() {
                return Err(TypesError::MalformedCode(code.clone()));
            }
            if code.namespace() != self.namespace {
                return Err(TypesError::ForeignCode {
                    namespace: self.namespace,
                    code: code.clone(),
                });
            }
            if !seen.insert(code) {
                return Err(TypesError::DuplicateCode(code.clone()));
            }
        }
        Ok(())
    }

    pub fn ftl_keys(&self) -> impl Iterator<Item = String> {
        self.codes.iter().map(MessageCode::ftl_key)
    }
}
