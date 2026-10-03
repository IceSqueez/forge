use crate::EmulatorError;

use serde_json::{Value, json};

pub const FAKE_DONATELLO_TOKEN: &str = "fake-donatello-token";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DonatesOrder {
    NewestFirst,
    OldestFirst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeDonation {
    pub pub_id: String,
    pub client_name: Option<String>,
    pub message: Option<String>,
    pub amount: String,
    pub currency: String,
    pub is_published: bool,
    pub created_at: String,
}

impl FakeDonation {
    pub fn new(pub_id: &str, client_name: &str, amount: &str, created_at: &str) -> Self {
        Self {
            pub_id: pub_id.to_owned(),
            client_name: Some(client_name.to_owned()),
            message: None,
            amount: amount.to_owned(),
            currency: "UAH".to_owned(),
            is_published: true,
            created_at: created_at.to_owned(),
        }
    }

    pub fn with_message(mut self, message: &str) -> Self {
        self.message = Some(message.to_owned());
        self
    }

    pub fn wire(&self) -> Value {
        json!({
            "pubId": self.pub_id,
            "clientName": self.client_name,
            "message": self.message,
            "amount": self.amount,
            "currency": self.currency,
            "goal": "",
            "isPublished": self.is_published,
            "createdAt": self.created_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeDonatelloConfig {
    pub token: String,
    pub nickname: String,
    pub profile_complete: bool,
    pub order: DonatesOrder,
    pub donations: Vec<FakeDonation>,
}

impl Default for FakeDonatelloConfig {
    fn default() -> Self {
        Self {
            token: FAKE_DONATELLO_TOKEN.to_owned(),
            nickname: "fake-streamer".to_owned(),
            profile_complete: true,
            order: DonatesOrder::NewestFirst,
            donations: Vec::new(),
        }
    }
}

pub fn donatello_wall_clock(at: time::OffsetDateTime) -> Result<String, EmulatorError> {
    let invalid = |reason: String| EmulatorError::InvalidFakeConfig { reason };
    let instant = jiff::Timestamp::from_second(at.unix_timestamp())
        .map_err(|e| invalid(format!("instant {at} is out of range: {e}")))?;
    let zone = jiff::tz::TimeZone::get(forge_donatello::DONATELLO_ASSUMED_TIME_ZONE)
        .map_err(|e| invalid(format!("time zone is unknown: {e}")))?;
    Ok(instant
        .to_zoned(zone)
        .strftime("%Y-%m-%d %H:%M:%S")
        .to_string())
}
