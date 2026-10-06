use async_trait::async_trait;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanPageToken(String);

impl BanPageToken {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanDuration {
    Permanent,
    Until(OffsetDateTime),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnbanRefusal {
    BannedOutsideForge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnbanAbility {
    Allowed,
    Refused(UnbanRefusal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanEntry {
    pub viewer_id: String,
    pub name: String,
    pub reason: Option<String>,
    pub moderator: Option<String>,
    pub created_at: Option<OffsetDateTime>,
    pub duration: BanDuration,
    pub unban: UnbanAbility,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanPage {
    pub entries: Vec<BanEntry>,
    pub next: Option<BanPageToken>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanListUnavailable {
    NotConnected,
    MissingScope,
    QuotaExhausted,
    Transport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BanListOutcome {
    Page(BanPage),
    Unavailable(BanListUnavailable),
}

#[async_trait]
pub trait BanListSource: Send + Sync {
    async fn list_bans(&self, after: Option<&BanPageToken>) -> BanListOutcome;
}
