use std::time::Duration;

use serde_json::{Map, Value, json};

pub const FAKE_MONOBANK_TOKEN: &str = "fake-monobank-token";

const UAH_NUMERIC: i64 = 980;
const SENDER_PREFIX: &str = "Від: ";
const TOP_UP_MCC: i64 = 4829;
const BANK_CALL_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeJar {
    pub id: String,
    pub send_id: String,
    pub title: String,
    pub currency_code: i64,
    pub goal: Option<i64>,
    pub balance: i64,
}

impl FakeJar {
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.to_owned(),
            send_id: format!("jar/{id}"),
            title: title.to_owned(),
            currency_code: UAH_NUMERIC,
            goal: None,
            balance: 0,
        }
    }

    pub fn with_goal(mut self, goal_minor_units: i64) -> Self {
        self.goal = Some(goal_minor_units);
        self
    }

    pub fn wire(&self) -> Value {
        json!({
            "id": self.id,
            "sendId": self.send_id,
            "title": self.title,
            "description": "",
            "currencyCode": self.currency_code,
            "balance": self.balance,
            "goal": self.goal,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeTransaction {
    pub id: String,
    pub time: i64,
    pub amount: i64,
    pub currency_code: i64,
    pub description: String,
    pub counter_name: Option<String>,
    pub comment: Option<String>,
}

impl FakeTransaction {
    pub fn top_up(id: &str, time: i64, amount_minor_units: i64, sender: &str) -> Self {
        Self {
            id: id.to_owned(),
            time,
            amount: amount_minor_units,
            currency_code: UAH_NUMERIC,
            description: format!("{SENDER_PREFIX}{sender}"),
            counter_name: None,
            comment: None,
        }
    }

    pub fn outgoing(id: &str, time: i64, amount_minor_units: i64) -> Self {
        Self {
            id: id.to_owned(),
            time,
            amount: -amount_minor_units.abs(),
            currency_code: UAH_NUMERIC,
            description: "Переказ на картку".to_owned(),
            counter_name: None,
            comment: None,
        }
    }

    pub fn with_comment(mut self, comment: &str) -> Self {
        self.comment = Some(comment.to_owned());
        self
    }

    pub fn with_counter_name(mut self, name: &str) -> Self {
        self.counter_name = Some(name.to_owned());
        self
    }

    pub fn with_currency(mut self, iso_numeric: i64) -> Self {
        self.currency_code = iso_numeric;
        self
    }

    pub fn wire(&self) -> Value {
        let mut item = Map::new();
        item.insert("id".to_owned(), json!(self.id));
        item.insert("time".to_owned(), json!(self.time));
        item.insert("description".to_owned(), json!(self.description));
        item.insert("mcc".to_owned(), json!(TOP_UP_MCC));
        item.insert("originalMcc".to_owned(), json!(TOP_UP_MCC));
        item.insert("hold".to_owned(), json!(false));
        item.insert("amount".to_owned(), json!(self.amount));
        item.insert("operationAmount".to_owned(), json!(self.amount));
        item.insert("currencyCode".to_owned(), json!(self.currency_code));
        item.insert("commissionRate".to_owned(), json!(0));
        item.insert("cashbackAmount".to_owned(), json!(0));
        item.insert("balance".to_owned(), json!(0));
        if let Some(comment) = &self.comment {
            item.insert("comment".to_owned(), json!(comment));
        }
        if let Some(name) = &self.counter_name {
            item.insert("counterName".to_owned(), json!(name));
        }
        Value::Object(item)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeMonobankConfig {
    pub token: String,
    pub client_name: String,
    pub jars: Vec<FakeJar>,
    pub transactions: Vec<(String, FakeTransaction)>,
    pub call_window: Duration,
}

impl Default for FakeMonobankConfig {
    fn default() -> Self {
        Self {
            token: FAKE_MONOBANK_TOKEN.to_owned(),
            client_name: "Fake Streamer".to_owned(),
            jars: Vec::new(),
            transactions: Vec::new(),
            call_window: BANK_CALL_WINDOW,
        }
    }
}
