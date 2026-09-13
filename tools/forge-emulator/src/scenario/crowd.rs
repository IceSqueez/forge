use serde::{Deserialize, Serialize};

use crate::twitch::Viewer;

const FIRST_CROWD_USER_ID: u64 = 300_000_000;
const LOGIN_PLACEHOLDER: &str = "{login}";
const NUMBER_PLACEHOLDER: &str = "{n}";

/// Chatter templates may use `{login}` and `{n}` (the viewer's 1-based number).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crowd {
    pub viewers: u32,
    #[serde(default)]
    pub chatter: Vec<String>,
    #[serde(default = "one")]
    pub chatter_per_viewer: u32,
    /// How many of the viewers, spread evenly through the crowd, also send one command each.
    #[serde(default)]
    pub command_senders: u32,
    /// Message texts sent by the command senders, assigned in turn.
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub spacing_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrowdMessage {
    pub viewer: Viewer,
    pub text: String,
    pub line: CrowdLine,
}

/// Indexes into `Crowd::chatter` or `Crowd::commands`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrowdLine {
    Chatter(usize),
    Command(usize),
}

fn one() -> u32 {
    1
}

impl Crowd {
    pub fn chatter_count(&self) -> u64 {
        if self.chatter.is_empty() {
            0
        } else {
            u64::from(self.viewers) * u64::from(self.chatter_per_viewer)
        }
    }

    pub fn message_count(&self) -> u64 {
        let commands = if self.commands.is_empty() {
            0
        } else {
            self.command_senders.min(self.viewers)
        };
        self.chatter_count() + u64::from(commands)
    }

    /// Deterministic: the same crowd always yields the same messages in the same order.
    pub fn plan(&self) -> Vec<CrowdMessage> {
        let mut messages = Vec::new();
        let mut commands_sent = 0usize;
        for number in 1..=self.viewers {
            let viewer = crowd_viewer(number);
            if !self.chatter.is_empty() {
                for turn in 0..self.chatter_per_viewer {
                    let index =
                        (u64::from(number - 1) + u64::from(turn)) as usize % self.chatter.len();
                    messages.push(CrowdMessage {
                        text: render(&self.chatter[index], &viewer.login, number),
                        viewer: viewer.clone(),
                        line: CrowdLine::Chatter(index),
                    });
                }
            }
            if self.sends_command(number) && !self.commands.is_empty() {
                let index = commands_sent % self.commands.len();
                messages.push(CrowdMessage {
                    text: self.commands[index].clone(),
                    viewer,
                    line: CrowdLine::Command(index),
                });
                commands_sent += 1;
            }
        }
        messages
    }

    fn sends_command(&self, number: u32) -> bool {
        let senders = u64::from(self.command_senders.min(self.viewers));
        let viewers = u64::from(self.viewers);
        let number = u64::from(number);
        number * senders / viewers > (number - 1) * senders / viewers
    }
}

fn crowd_viewer(number: u32) -> Viewer {
    Viewer::new(
        (FIRST_CROWD_USER_ID + u64::from(number)).to_string(),
        format!("crowd_{number:04}"),
    )
}

fn render(template: &str, login: &str, number: u32) -> String {
    template
        .replace(LOGIN_PLACEHOLDER, login)
        .replace(NUMBER_PLACEHOLDER, &number.to_string())
}

pub(crate) fn template_problem(template: &str) -> Option<String> {
    let mut rest = template;
    while let Some(open) = rest.find(['{', '}']) {
        let tail = &rest[open..];
        if tail.starts_with('}') {
            return Some("has a `}` with no matching `{`".to_owned());
        }
        let Some(close) = tail.find('}') else {
            return Some("has a `{` with no matching `}`".to_owned());
        };
        let placeholder = &tail[..=close];
        if placeholder != LOGIN_PLACEHOLDER && placeholder != NUMBER_PLACEHOLDER {
            return Some(format!(
                "uses unknown placeholder `{placeholder}`; only {LOGIN_PLACEHOLDER} and {NUMBER_PLACEHOLDER} exist"
            ));
        }
        rest = &tail[close + 1..];
    }
    None
}
