use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::EmulatorError;
use crate::donatello::{FakeDonation, donatello_wall_clock};
use crate::monobank::FakeTransaction;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DonatelloGift {
    pub id: String,
    #[serde(default)]
    pub donor: Option<String>,
    pub amount: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub minutes_ago: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonobankGift {
    pub id: String,
    pub sender: String,
    pub amount_minor: i64,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub minutes_ago: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum OfflineGift {
    Donatello(DonatelloGift),
    Monobank(MonobankGift),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FakeDonatelloSetup {
    #[serde(default)]
    pub history: Vec<DonatelloGift>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FakeMonobankSetup {
    #[serde(default)]
    pub history: Vec<MonobankGift>,
}

fn made_at(now: OffsetDateTime, minutes_ago: u32) -> OffsetDateTime {
    now - Duration::minutes(i64::from(minutes_ago))
}

impl DonatelloGift {
    pub fn to_fake(&self, now: OffsetDateTime) -> Result<FakeDonation, EmulatorError> {
        let created_at = donatello_wall_clock(made_at(now, self.minutes_ago))?;
        let mut donation = FakeDonation::new(&self.id, "", &self.amount, &created_at);
        donation.client_name = self.donor.clone();
        donation.message = self.message.clone();
        Ok(donation)
    }
}

impl MonobankGift {
    pub fn to_fake(&self, now: OffsetDateTime) -> FakeTransaction {
        let at = made_at(now, self.minutes_ago).unix_timestamp();
        let transaction = FakeTransaction::top_up(&self.id, at, self.amount_minor, &self.sender);
        match &self.comment {
            Some(comment) => transaction.with_comment(comment),
            None => transaction,
        }
    }
}

impl OfflineGift {
    pub fn id(&self) -> &str {
        match self {
            Self::Donatello(gift) => &gift.id,
            Self::Monobank(gift) => &gift.id,
        }
    }
}
