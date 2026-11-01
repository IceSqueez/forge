use std::collections::BTreeMap;

use forge_types::Variant;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewerAction {
    Shoutout,
    Whisper(String),
    Timeout { seconds: i64 },
    Ban,
    Unban,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewerRef<'a> {
    pub name: &'a str,
    pub viewer_id: Option<&'a str>,
}

impl ViewerRef<'_> {
    pub fn display_name_entry(&self, key: &str) -> (String, Variant) {
        (key.to_owned(), Variant::String(self.name.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewerStep {
    pub kind_id: &'static str,
    pub config: BTreeMap<String, Variant>,
}

impl ViewerStep {
    pub fn new<const N: usize>(kind_id: &'static str, config: [(String, Variant); N]) -> Self {
        Self {
            kind_id,
            config: BTreeMap::from(config),
        }
    }
}
