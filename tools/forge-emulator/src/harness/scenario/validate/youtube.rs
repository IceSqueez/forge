use super::Validator;
use crate::harness::scenario::step::StepAction;

impl<'a> Validator<'a> {
    pub(super) fn require_fake_youtube(&mut self, location: &str) {
        let scenario = self.scenario;
        if scenario.fixture.youtube.is_none() || scenario.fakes.youtube.is_none() {
            self.report(
                location,
                "needs a fake YouTube: add fixture.youtube and fakes.youtube",
            );
        }
    }

    pub(super) fn require_live_youtube_chat(&mut self, location: String) {
        if !self.youtube_live {
            self.report(
                location,
                "the fake YouTube broadcast is offline, so its chat takes no message: start it with youtube_broadcast first",
            );
        }
    }

    pub(super) fn check_youtube_author(
        &mut self,
        location: &str,
        author: &crate::fakes::youtube::YouTubeChatter,
    ) {
        self.not_blank(format!("{location}.author.channel_id"), &author.channel_id);
        self.not_blank(
            format!("{location}.author.display_name"),
            &author.display_name,
        );
    }

    pub(super) fn check_youtube_action(&mut self, location: &str, action: &StepAction) {
        match action {
            StepAction::YouTubeChatPolled { within_ms } => {
                self.check_within(location, *within_ms);
            }
            StepAction::YouTubeChat(message) => {
                self.not_blank(format!("{location}.text"), &message.text);
                self.check_youtube_author(location, &message.author);
                self.require_live_youtube_chat(location.to_owned());
            }
            StepAction::YouTubeChatEvent { author, snippet } => {
                self.check_youtube_author(location, author);
                let kind = snippet.get("type").and_then(|kind| kind.as_str());
                if !snippet.is_object() || kind.is_none_or(|kind| kind.trim().is_empty()) {
                    self.report(
                        format!("{location}.snippet"),
                        "must be a liveChatMessage snippet object with a non-blank `type`",
                    );
                }
                self.require_live_youtube_chat(location.to_owned());
            }
            StepAction::YouTubeBroadcast { live } => {
                if *live == self.youtube_live {
                    self.report(
                        format!("{location}.live"),
                        format!(
                            "the fake YouTube broadcast is already {}, so this step changes nothing",
                            if *live { "live" } else { "offline" }
                        ),
                    );
                }
                self.youtube_live = *live;
            }
            _ => {}
        }
    }
}
