#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Stdio;
use std::time::Duration;

use forge_emulator::console::{
    DonatelloCommand, DonatelloConsole, MonobankCommand, MonobankConsole, parse_donatello_command,
    parse_jar, parse_monobank_command,
};
use forge_emulator::donatello::{
    DonatesFault, FAKE_DONATELLO_TOKEN, FakeDonatello, FakeDonatelloConfig,
};
use forge_emulator::monobank::{FAKE_MONOBANK_TOKEN, FakeJar, FakeMonobank, FakeMonobankConfig};
use forge_platform_core::EndpointSurface;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};

const WAIT: Duration = Duration::from_secs(10);
const JAR: &str = "jar-main";

fn donate(donor: &str, amount: &str, message: Option<&str>) -> DonatelloCommand {
    DonatelloCommand::Donate {
        donor: donor.to_owned(),
        amount: amount.to_owned(),
        message: message.map(str::to_owned),
    }
}

async fn donations(base_url: &str) -> Value {
    reqwest::Client::new()
        .get(format!("{base_url}/donates?page=0&size=20"))
        .header("X-Token", FAKE_DONATELLO_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

async fn jar_statement(base_url: &str, jar: &str) -> Value {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    reqwest::Client::new()
        .get(format!(
            "{base_url}/personal/statement/{jar}/{}/{}",
            now - 3600,
            now + 60
        ))
        .header("X-Token", FAKE_MONOBANK_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[test]
fn donatello_commands_parse_into_what_the_fake_should_do() {
    let cases = [
        ("", None),
        ("   ", None),
        ("donate Alice 50", Some(donate("Alice", "50", None))),
        (
            "  donate Bob 12.5 keep it up  ",
            Some(donate("Bob", "12.5", Some("keep it up"))),
        ),
        (
            "fail 429",
            Some(DonatelloCommand::Fail(DonatesFault::TooManyRequests {
                retry_after_secs: None,
            })),
        ),
        (
            "fail 429 30",
            Some(DonatelloCommand::Fail(DonatesFault::TooManyRequests {
                retry_after_secs: Some(30),
            })),
        ),
        (
            "fail 503",
            Some(DonatelloCommand::Fail(DonatesFault::ServerError {
                status: 503,
            })),
        ),
        (
            "fail malformed",
            Some(DonatelloCommand::Fail(DonatesFault::MalformedJson)),
        ),
        (
            "profile incomplete",
            Some(DonatelloCommand::Profile { complete: false }),
        ),
        (
            "profile complete",
            Some(DonatelloCommand::Profile { complete: true }),
        ),
    ];

    for (line, expected) in cases {
        assert_eq!(parse_donatello_command(line), Ok(expected), "{line:?}");
    }
}

#[test]
fn donatello_commands_the_fake_cannot_act_on_are_refused() {
    for line in [
        "donate Alice",
        "donate Alice fifty",
        "donate Alice 0",
        "donate Alice -5",
        "donate Alice NaN",
        "fail 200",
        "fail 600",
        "fail teapot",
        "fail 503 30",
        "fail 429 soon",
        "profile maybe",
        "tip Alice 50",
    ] {
        assert!(
            parse_donatello_command(line).is_err(),
            "{line:?} was accepted"
        );
    }
}

#[test]
fn monobank_top_ups_parse_with_an_optional_comment() {
    let cases = [
        ("", None),
        (
            "top-up jar-main 5000 Olena",
            Some(MonobankCommand::TopUp {
                jar: JAR.to_owned(),
                amount_minor: 5000,
                sender: "Olena".to_owned(),
                comment: None,
            }),
        ),
        (
            "top-up jar-main 1 Taras for the stream",
            Some(MonobankCommand::TopUp {
                jar: JAR.to_owned(),
                amount_minor: 1,
                sender: "Taras".to_owned(),
                comment: Some("for the stream".to_owned()),
            }),
        ),
    ];

    for (line, expected) in cases {
        assert_eq!(parse_monobank_command(line), Ok(expected), "{line:?}");
    }
}

#[test]
fn monobank_top_ups_without_a_positive_whole_amount_or_a_sender_are_refused() {
    for line in [
        "top-up jar-main 0 Olena",
        "top-up jar-main -100 Olena",
        "top-up jar-main 12.5 Olena",
        "top-up jar-main 5000",
        "donate jar-main 5000 Olena",
    ] {
        assert!(
            parse_monobank_command(line).is_err(),
            "{line:?} was accepted"
        );
    }
}

#[test]
fn a_jar_spec_names_its_id_and_falls_back_to_the_id_as_title() {
    let cases = [
        ("jar-main=Stream jar", "jar-main", "Stream jar"),
        ("jar-main", "jar-main", "jar-main"),
        ("jar-main=", "jar-main", "jar-main"),
    ];

    for (spec, id, title) in cases {
        let jar = parse_jar(spec).unwrap();
        assert_eq!((jar.id.as_str(), jar.title.as_str()), (id, title), "{spec}");
    }
    assert!(parse_jar("=Stream jar").is_err());
}

#[tokio::test]
async fn a_console_donation_is_served_with_a_fresh_id_each_time() {
    let fake = FakeDonatello::start(FakeDonatelloConfig::default())
        .await
        .unwrap();
    let mut console = DonatelloConsole::new(&fake);

    console
        .apply(donate("Alice", "50", Some("hello there")))
        .unwrap();
    console.apply(donate("Alice", "50", None)).unwrap();

    let page = donations(fake.base_url()).await;
    let items = page["content"].as_array().unwrap();
    let ids: Vec<&str> = items
        .iter()
        .map(|item| item["pubId"].as_str().unwrap())
        .collect();
    assert_eq!(items.len(), 2, "{page}");
    assert_ne!(ids[0], ids[1], "two donations share one id");
    assert!(
        items
            .iter()
            .any(|item| item["message"] == "hello there" && item["clientName"] == "Alice"),
        "{page}"
    );
    fake.shutdown().await;
}

#[tokio::test]
async fn a_console_top_up_lands_only_in_a_jar_the_fake_serves() {
    let jars = vec![FakeJar::new(JAR, "Stream jar")];
    let fake = FakeMonobank::start(FakeMonobankConfig {
        jars: jars.clone(),
        ..FakeMonobankConfig::default()
    })
    .await
    .unwrap();
    let mut console = MonobankConsole::new(&fake, &jars);

    let refused = console.apply(MonobankCommand::TopUp {
        jar: "jar-unknown".to_owned(),
        amount_minor: 100,
        sender: "Olena".to_owned(),
        comment: None,
    });
    console
        .apply(MonobankCommand::TopUp {
            jar: JAR.to_owned(),
            amount_minor: 5000,
            sender: "Olena".to_owned(),
            comment: Some("for the stream".to_owned()),
        })
        .unwrap();

    assert!(refused.is_err(), "a top-up into an unknown jar was taken");
    let items = jar_statement(fake.base_url(), JAR).await;
    let items = items.as_array().unwrap();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["amount"], 5000);
    assert_eq!(items[0]["comment"], "for the stream");
    fake.shutdown().await;
}

struct Running {
    child: Child,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Running {
    async fn start(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_forge-emulator"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        Self { child, lines }
    }

    async fn line(&mut self) -> String {
        tokio::time::timeout(WAIT, self.lines.next_line())
            .await
            .expect("the fake printed nothing")
            .unwrap()
            .expect("the fake closed its output")
    }

    async fn send(&mut self, command: &str) -> String {
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin
            .write_all(format!("{command}\n").as_bytes())
            .await
            .unwrap();
        stdin.flush().await.unwrap();
        self.line().await
    }

    fn value_of(line: &str, variable: &str) -> String {
        line.strip_prefix(&format!("{variable}="))
            .unwrap_or_else(|| panic!("expected {variable}=..., got {line}"))
            .to_owned()
    }
}

#[tokio::test]
async fn fake_donatello_mode_prints_its_address_and_serves_donations_typed_on_stdin() {
    let mut fake = Running::start(&["fake-donatello"]).await;
    let base_url = Running::value_of(&fake.line().await, EndpointSurface::DonatelloApi.env_var());
    let token = Running::value_of(&fake.line().await, "DONATELLO_TOKEN");

    let ack = fake.send("donate Alice 50 hello there").await;

    assert_eq!(token, FAKE_DONATELLO_TOKEN);
    assert!(ack.starts_with("ok "), "{ack}");
    let page = donations(&base_url).await;
    assert_eq!(page["content"][0]["clientName"], "Alice", "{page}");
}

#[tokio::test]
async fn fake_monobank_mode_prints_its_address_and_jars_and_serves_typed_top_ups() {
    let mut fake = Running::start(&["fake-monobank", "--jar", "jar-main=Stream jar"]).await;
    let base_url = Running::value_of(&fake.line().await, EndpointSurface::MonobankApi.env_var());
    let token = Running::value_of(&fake.line().await, "MONOBANK_TOKEN");
    let jar = Running::value_of(&fake.line().await, "MONOBANK_JAR");

    let ack = fake.send("top-up jar-main 5000 Olena").await;

    assert_eq!((token.as_str(), jar.as_str()), (FAKE_MONOBANK_TOKEN, JAR));
    assert!(ack.starts_with("ok "), "{ack}");
    let items = jar_statement(&base_url, JAR).await;
    assert_eq!(items[0]["amount"], 5000, "{items}");
}
