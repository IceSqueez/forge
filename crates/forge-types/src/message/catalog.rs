use std::collections::BTreeSet;

use thiserror::Error;

use crate::message::code::{MessageCode, is_code_segment};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MessageCatalogError {
    #[error("catalog namespace is not a lowercase snake_case segment: {0:?}")]
    MalformedNamespace(&'static str),
    #[error("catalog code is malformed: {0}")]
    MalformedCode(MessageCode),
    #[error("catalog {namespace:?} lists a code from another namespace: {code}")]
    ForeignCode {
        namespace: &'static str,
        code: MessageCode,
    },
    #[error("catalog lists a code twice: {0}")]
    DuplicateCode(MessageCode),
}

#[derive(Debug, Clone, Copy)]
pub struct MessageCatalog {
    pub namespace: &'static str,
    pub codes: &'static [MessageCode],
    pub en_ftl: &'static str,
    pub uk_ftl: &'static str,
}

impl MessageCatalog {
    pub fn check(&self) -> Result<(), MessageCatalogError> {
        if !is_code_segment(self.namespace) {
            return Err(MessageCatalogError::MalformedNamespace(self.namespace));
        }
        let mut seen = BTreeSet::new();
        for code in self.codes {
            if !code.is_well_formed() {
                return Err(MessageCatalogError::MalformedCode(code.clone()));
            }
            if code.namespace() != self.namespace {
                return Err(MessageCatalogError::ForeignCode {
                    namespace: self.namespace,
                    code: code.clone(),
                });
            }
            if !seen.insert(code) {
                return Err(MessageCatalogError::DuplicateCode(code.clone()));
            }
        }
        Ok(())
    }

    pub fn ftl_keys(&self) -> impl Iterator<Item = String> {
        self.codes.iter().map(MessageCode::ftl_key)
    }
}
