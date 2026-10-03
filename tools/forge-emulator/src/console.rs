use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::StatusCode;
use time::OffsetDateTime;

use crate::EmulatorError;
use crate::donatello::{DonatesFault, FakeDonatello, FakeDonation, donatello_wall_clock};
use crate::monobank::{FakeJar, FakeMonobank, FakeTransaction};

const DONATELLO_HELP: &str = "donate <donor> <amount> [message...] | fail 429 [retry_after_secs] | fail <status> | fail malformed | profile complete|incomplete";
const MONOBANK_HELP: &str = "top-up <jar> <amount_minor> <sender> [comment...]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DonatelloCommand {
    Donate {
        donor: String,
        amount: String,
        message: Option<String>,
    },
    Fail(DonatesFault),
    Profile {
        complete: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonobankCommand {
    TopUp {
        jar: String,
        amount_minor: i64,
        sender: String,
        comment: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleRefusal {
    pub reason: String,
}

impl ConsoleRefusal {
    fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

pub fn parse_donatello_command(line: &str) -> Result<Option<DonatelloCommand>, ConsoleRefusal> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let command = match words.as_slice() {
        [] => return Ok(None),
        ["donate", donor, amount, message @ ..] => DonatelloCommand::Donate {
            donor: (*donor).to_owned(),
            amount: decimal_amount(amount)?,
            message: joined(message),
        },
        ["fail", "malformed"] => DonatelloCommand::Fail(DonatesFault::MalformedJson),
        ["fail", status] => DonatelloCommand::Fail(status_fault(status, None)?),
        ["fail", status, retry] => {
            let fault = status_fault(status, Some(retry))?;
            DonatelloCommand::Fail(fault)
        }
        ["profile", "complete"] => DonatelloCommand::Profile { complete: true },
        ["profile", "incomplete"] => DonatelloCommand::Profile { complete: false },
        _ => return Err(ConsoleRefusal::new(format!("expected {DONATELLO_HELP}"))),
    };
    Ok(Some(command))
}

pub fn parse_monobank_command(line: &str) -> Result<Option<MonobankCommand>, ConsoleRefusal> {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        [] => Ok(None),
        ["top-up", jar, amount, sender, comment @ ..] => {
            let amount_minor = amount
                .parse::<i64>()
                .ok()
                .filter(|minor| *minor > 0)
                .ok_or_else(|| {
                    ConsoleRefusal::new(format!(
                        "amount `{amount}` is not a positive number of kopiykas"
                    ))
                })?;
            Ok(Some(MonobankCommand::TopUp {
                jar: (*jar).to_owned(),
                amount_minor,
                sender: (*sender).to_owned(),
                comment: joined(comment),
            }))
        }
        _ => Err(ConsoleRefusal::new(format!("expected {MONOBANK_HELP}"))),
    }
}

pub fn parse_jar(spec: &str) -> Result<FakeJar, ConsoleRefusal> {
    let (id, title) = spec.split_once('=').unwrap_or((spec, spec));
    let id = id.trim();
    if id.is_empty() {
        return Err(ConsoleRefusal::new(format!("jar `{spec}` has no id")));
    }
    let title = match title.trim() {
        "" => id,
        title => title,
    };
    Ok(FakeJar::new(id, title))
}

pub struct DonatelloConsole<'a> {
    fake: &'a FakeDonatello,
    issued: u64,
}

impl<'a> DonatelloConsole<'a> {
    pub fn new(fake: &'a FakeDonatello) -> Self {
        Self { fake, issued: 0 }
    }

    pub fn apply(&mut self, command: DonatelloCommand) -> Result<String, EmulatorError> {
        match command {
            DonatelloCommand::Donate {
                donor,
                amount,
                message,
            } => {
                self.issued += 1;
                let id = format!("console-{}", self.issued);
                let created_at = donatello_wall_clock(OffsetDateTime::now_utc())?;
                let mut donation = FakeDonation::new(&id, &donor, &amount, &created_at);
                if let Some(message) = &message {
                    donation = donation.with_message(message);
                }
                self.fake.donate(donation);
                Ok(format!("ok donation {id}: {donor} {amount} UAH"))
            }
            DonatelloCommand::Fail(fault) => {
                let ack = format!("ok next donation list fails with {fault:?}");
                self.fake.fail_next_donates(fault);
                Ok(ack)
            }
            DonatelloCommand::Profile { complete } => {
                self.fake.set_profile_complete(complete);
                Ok(format!(
                    "ok profile {}",
                    if complete { "complete" } else { "incomplete" }
                ))
            }
        }
    }
}

pub struct MonobankConsole<'a> {
    fake: &'a FakeMonobank,
    jars: Vec<String>,
    issued: u64,
}

impl<'a> MonobankConsole<'a> {
    pub fn new(fake: &'a FakeMonobank, jars: &[FakeJar]) -> Self {
        Self {
            fake,
            jars: jars.iter().map(|jar| jar.id.clone()).collect(),
            issued: 0,
        }
    }

    pub fn apply(&mut self, command: MonobankCommand) -> Result<String, ConsoleRefusal> {
        let MonobankCommand::TopUp {
            jar,
            amount_minor,
            sender,
            comment,
        } = command;
        if !self.jars.contains(&jar) {
            return Err(ConsoleRefusal::new(format!(
                "unknown jar `{jar}`; the fake serves {}",
                self.jars.join(", ")
            )));
        }
        self.issued += 1;
        let id = format!("console-{}", self.issued);
        let mut transaction = FakeTransaction::top_up(&id, unix_now(), amount_minor, &sender);
        if let Some(comment) = &comment {
            transaction = transaction.with_comment(comment);
        }
        self.fake.top_up(&jar, transaction);
        Ok(format!(
            "ok top-up {id}: {sender} {amount_minor} kopiykas into {jar}"
        ))
    }
}

fn decimal_amount(raw: &str) -> Result<String, ConsoleRefusal> {
    match raw.parse::<f64>() {
        Ok(amount) if amount.is_finite() && amount > 0.0 => Ok(raw.to_owned()),
        _ => Err(ConsoleRefusal::new(format!(
            "amount `{raw}` is not a positive number"
        ))),
    }
}

fn status_fault(status: &str, retry: Option<&&str>) -> Result<DonatesFault, ConsoleRefusal> {
    let status = status
        .parse::<u16>()
        .ok()
        .and_then(|code| StatusCode::from_u16(code).ok())
        .filter(|status| status.is_client_error() || status.is_server_error())
        .ok_or_else(|| ConsoleRefusal::new(format!("`{status}` is not an HTTP error status")))?;
    let retry_after_secs = retry
        .map(|retry| {
            retry
                .parse::<u32>()
                .map_err(|_| ConsoleRefusal::new(format!("`{retry}` is not a number of seconds")))
        })
        .transpose()?;
    match (status, retry_after_secs) {
        (StatusCode::TOO_MANY_REQUESTS, retry_after_secs) => {
            Ok(DonatesFault::TooManyRequests { retry_after_secs })
        }
        (status, None) => Ok(DonatesFault::ServerError {
            status: status.as_u16(),
        }),
        (status, Some(_)) => Err(ConsoleRefusal::new(format!(
            "only a 429 carries a retry delay, not {status}"
        ))),
    }
}

fn joined(words: &[&str]) -> Option<String> {
    (!words.is_empty()).then(|| words.join(" "))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}
