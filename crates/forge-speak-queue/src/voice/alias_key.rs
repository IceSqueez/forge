use forge_types::PlatformId;

const PLATFORM_SEPARATOR: char = ':';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasKey {
    pub platform: Option<PlatformId>,
    pub handle: String,
}

impl AliasKey {
    pub fn new(platform: Option<PlatformId>, handle: &str) -> Self {
        Self {
            platform,
            handle: handle.to_owned(),
        }
    }

    pub fn parse(stored: &str) -> Self {
        let (platform, handle) = split_stored(stored);
        Self::new(platform, handle)
    }

    pub fn to_stored(&self) -> String {
        match self.platform {
            Some(platform) => format!("{}{PLATFORM_SEPARATOR}{}", platform.as_str(), self.handle),
            None => self.handle.clone(),
        }
    }
}

pub(crate) fn split_stored(stored: &str) -> (Option<PlatformId>, &str) {
    stored
        .split_once(PLATFORM_SEPARATOR)
        .and_then(|(token, handle)| {
            PlatformId::parse(token).map(|platform| (Some(platform), handle))
        })
        .unwrap_or((None, stored))
}
