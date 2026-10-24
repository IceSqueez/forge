use std::time::Duration;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

const INTERACTIVE_CONNECT_BUDGET: Duration = Duration::from_secs(5);
const INTERACTIVE_TOTAL_BUDGET: Duration = Duration::from_secs(10);
const INTERACTIVE_BODY_CAP: u64 = 4 * MIB;

const OAUTH_CONNECT_BUDGET: Duration = Duration::from_secs(5);
const OAUTH_TOTAL_BUDGET: Duration = Duration::from_secs(15);
const OAUTH_BODY_CAP: u64 = 64 * KIB;

const CATALOG_CONNECT_BUDGET: Duration = Duration::from_secs(5);
const CATALOG_TOTAL_BUDGET: Duration = Duration::from_secs(20);
const CATALOG_BODY_CAP: u64 = 8 * MIB;

const MEDIA_CONNECT_BUDGET: Duration = Duration::from_secs(10);
const MEDIA_TOTAL_BUDGET: Duration = Duration::from_secs(60);
const MEDIA_BODY_CAP: u64 = 32 * MIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallClass {
    Interactive,
    OAuth,
    Catalog,
    Media,
}

impl CallClass {
    pub const fn connect_budget(self) -> Duration {
        match self {
            Self::Interactive => INTERACTIVE_CONNECT_BUDGET,
            Self::OAuth => OAUTH_CONNECT_BUDGET,
            Self::Catalog => CATALOG_CONNECT_BUDGET,
            Self::Media => MEDIA_CONNECT_BUDGET,
        }
    }

    pub const fn total_budget(self) -> Duration {
        match self {
            Self::Interactive => INTERACTIVE_TOTAL_BUDGET,
            Self::OAuth => OAUTH_TOTAL_BUDGET,
            Self::Catalog => CATALOG_TOTAL_BUDGET,
            Self::Media => MEDIA_TOTAL_BUDGET,
        }
    }

    pub const fn body_cap(self) -> u64 {
        match self {
            Self::Interactive => INTERACTIVE_BODY_CAP,
            Self::OAuth => OAUTH_BODY_CAP,
            Self::Catalog => CATALOG_BODY_CAP,
            Self::Media => MEDIA_BODY_CAP,
        }
    }
}
