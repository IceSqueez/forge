use super::{
    MAX_CHATTER_PER_VIEWER, MAX_CROWD_MESSAGES, MAX_CROWD_SPACING_MS, MAX_CROWD_VIEWERS, Validator,
};
use crate::harness::scenario::crowd::{Crowd, CrowdLine, template_problem};
use crate::harness::scenario::step::ChatMessage;

impl<'a> Validator<'a> {
    pub(super) fn check_chat(&mut self, location: &str, message: &ChatMessage) {
        self.require_chat_subscription(location);
        self.not_blank(
            format!("{location}.viewer.user_id"),
            &message.viewer.user_id,
        );
        self.not_blank(format!("{location}.viewer.login"), &message.viewer.login);
        self.not_blank(format!("{location}.text"), &message.text);
        self.command_matches_sent += self.commands_fired_by(&message.text);
    }

    pub(super) fn check_crowd(&mut self, location: &str, crowd: &Crowd) {
        self.require_chat_subscription(location);
        let mut plannable = self.in_range(
            format!("{location}.viewers"),
            u64::from(crowd.viewers),
            1,
            u64::from(MAX_CROWD_VIEWERS),
        );
        if !crowd.chatter.is_empty() {
            plannable &= self.in_range(
                format!("{location}.chatter_per_viewer"),
                u64::from(crowd.chatter_per_viewer),
                1,
                u64::from(MAX_CHATTER_PER_VIEWER),
            );
        }
        self.in_range(
            format!("{location}.spacing_ms"),
            crowd.spacing_ms,
            0,
            MAX_CROWD_SPACING_MS,
        );
        self.check_crowd_commands(location, crowd);
        if crowd.chatter.is_empty() && crowd.command_senders == 0 {
            self.report(
                location,
                "sends no messages: give it chatter or command_senders",
            );
        }
        if crowd.message_count() > MAX_CROWD_MESSAGES {
            plannable = false;
            self.report(
                location,
                format!(
                    "sends {} messages, more than the limit of {MAX_CROWD_MESSAGES}",
                    crowd.message_count()
                ),
            );
        }
        for (index, template) in crowd.chatter.iter().enumerate() {
            let at = format!("{location}.chatter[{index}]");
            let problem = if template.trim().is_empty() {
                Some("must not be blank".to_owned())
            } else {
                template_problem(template)
            };
            if let Some(problem) = problem {
                plannable = false;
                self.report(at, problem);
            }
        }
        if plannable {
            self.check_crowd_plan(location, crowd);
        }
    }

    pub(super) fn check_crowd_commands(&mut self, location: &str, crowd: &Crowd) {
        if crowd.command_senders > crowd.viewers {
            self.report(
                format!("{location}.command_senders"),
                format!("exceeds the crowd's {} viewers", crowd.viewers),
            );
        }
        if crowd.command_senders > 0 && crowd.commands.is_empty() {
            self.report(
                format!("{location}.commands"),
                "must list at least one command when command_senders is set",
            );
        }
        if crowd.command_senders == 0 && !crowd.commands.is_empty() {
            self.report(
                format!("{location}.command_senders"),
                "must be at least 1 when commands are listed",
            );
        }
        for (index, command) in crowd.commands.iter().enumerate() {
            if self.commands_fired_by(command) == 0 {
                self.report(
                    format!("{location}.commands[{index}]"),
                    format!("`{command}` runs no chat command in the fixture"),
                );
            }
        }
    }

    pub(super) fn check_crowd_plan(&mut self, location: &str, crowd: &Crowd) {
        let mut reported = vec![false; crowd.chatter.len()];
        for message in crowd.plan() {
            self.command_matches_sent += self.commands_fired_by(&message.text);
            let CrowdLine::Chatter(index) = message.line else {
                continue;
            };
            if reported[index] {
                continue;
            }
            if let Some(phrase) = self.first_command_fired_by(&message.text) {
                reported[index] = true;
                self.report(
                    format!("{location}.chatter[{index}]"),
                    format!(
                        "renders `{}`, which starts with the fixture command `{phrase}`, so chatter would run it",
                        message.text
                    ),
                );
            }
        }
    }

    pub(super) fn command_phrases(&self) -> impl Iterator<Item = &'a str> + use<'a> {
        let fixture = &self.scenario.fixture;
        fixture
            .twitch
            .iter()
            .flat_map(|_| fixture.chat_commands.iter())
            .map(|command| command.phrase.as_str())
            .filter(|phrase| !phrase.is_empty())
    }

    pub(super) fn first_command_fired_by(&self, text: &str) -> Option<&'a str> {
        let text = text.to_lowercase();
        self.command_phrases()
            .find(|phrase| text.starts_with(&phrase.to_lowercase()))
    }

    pub(super) fn commands_fired_by(&self, text: &str) -> u64 {
        let text = text.to_lowercase();
        let fired = self
            .command_phrases()
            .filter(|phrase| text.starts_with(&phrase.to_lowercase()))
            .count();
        u64::try_from(fired).unwrap_or(u64::MAX)
    }
}
