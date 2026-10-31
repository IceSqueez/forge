use super::{MAX_WAIT_MS, Validator};

impl<'a> Validator<'a> {
    fn pair<F, S>(
        &mut self,
        service: &str,
        fixture: Option<F>,
        fake: Option<S>,
        without_fake: &str,
        without_fixture: &str,
    ) -> Option<(F, S)> {
        match (fixture, fake) {
            (Some(_), None) => {
                self.report(service, without_fake);
                None
            }
            (None, Some(_)) => {
                self.report(service, without_fixture);
                None
            }
            (fixture, fake) => fixture.zip(fake),
        }
    }

    pub(super) fn check_fakes(&mut self) {
        self.check_fake_twitch();
        self.check_fake_kick();
        self.check_fake_youtube();
        self.check_fake_obs();
        self.check_fake_vtube();
        self.check_fake_donations();
        let scenario = self.scenario;
        self.pair(
            "fakes.discord",
            (!scenario.fixture.discord_webhooks.is_empty()).then_some(()),
            scenario.fakes.discord.as_ref(),
            "is required because the fixture seeds Discord webhooks; without it they have no address to post to",
            "needs fixture.discord_webhooks: the fake answers only the webhooks the fixture seeds",
        );
    }

    fn check_fake_twitch(&mut self) {
        let scenario = self.scenario;
        let paired = self.pair(
            "fakes.twitch",
            scenario.fixture.twitch.as_ref(),
            scenario.fakes.twitch.as_ref(),
            "is required because the fixture seeds a Twitch account; without it forge would reach the real Twitch",
            "needs fixture.twitch: the fake accepts only the credentials the fixture seeds",
        );
        if let Some((_, setup)) = paired {
            self.report_all("fakes.twitch", setup.problems());
        }
    }

    fn check_fake_kick(&mut self) {
        let scenario = self.scenario;
        let paired = self.pair(
            "fakes.kick",
            scenario.fixture.kick.as_ref(),
            scenario.fakes.kick.as_ref(),
            "is required because the fixture seeds a Kick account; without it forge would reach the real Kick",
            "needs fixture.kick: the fake accepts only the credentials the fixture seeds",
        );
        if let Some((_, setup)) = paired
            && setup.chatroom_id == 0
        {
            self.report("fakes.kick.chatroom_id", "must be non-zero");
        }
    }

    fn check_fake_youtube(&mut self) {
        let scenario = self.scenario;
        let paired = self.pair(
            "fakes.youtube",
            scenario.fixture.youtube.as_ref(),
            scenario.fakes.youtube.as_ref(),
            "is required because the fixture seeds a YouTube account; without it forge would reach the real YouTube",
            "needs fixture.youtube: the fake accepts only the credentials the fixture seeds",
        );
        if let Some((_, setup)) = paired {
            self.not_blank("fakes.youtube.broadcast_id", &setup.broadcast_id);
            self.not_blank("fakes.youtube.live_chat_id", &setup.live_chat_id);
            self.in_range(
                "fakes.youtube.polling_interval_ms",
                setup.polling_interval_ms,
                1,
                MAX_WAIT_MS,
            );
        }
    }

    fn check_fake_obs(&mut self) {
        let scenario = self.scenario;
        let paired = self.pair(
            "fakes.obs",
            scenario.fixture.obs.as_ref(),
            scenario.fakes.obs.as_ref(),
            "is required because the fixture seeds an OBS connection; without it forge has no OBS to reach",
            "needs fixture.obs: forge connects only to an OBS the fixture seeds",
        );
        if let Some((connection, config)) = paired {
            if connection.port != 0 {
                self.report(
                    "fixture.obs.port",
                    "must be left out: the run fills it with the fake OBS port",
                );
            }
            self.report_all("fakes.obs", config.problems());
        }
    }

    fn check_fake_vtube(&mut self) {
        let scenario = self.scenario;
        let paired = self.pair(
            "fakes.vtube",
            scenario.fixture.vtube.as_ref(),
            scenario.fakes.vtube.as_ref(),
            "is required because the fixture seeds a VTube Studio connection; without it forge has no VTube Studio to reach",
            "needs fixture.vtube: forge connects only to a VTube Studio the fixture seeds",
        );
        if let Some((connection, config)) = paired {
            if connection.port != 0 {
                self.report(
                    "fixture.vtube.port",
                    "must be left out: the run fills it with the fake VTube Studio port",
                );
            }
            self.not_blank("fixture.vtube.token", &connection.token);
            self.report_all("fakes.vtube", config.problems());
        }
    }

    fn check_fake_donations(&mut self) {
        let scenario = self.scenario;
        let donatello = self.pair(
            "fakes.donatello",
            scenario.fixture.donatello.as_ref(),
            scenario.fakes.donatello.as_ref(),
            "is required because the fixture seeds a Donatello token; without it forge would poll the real Donatello",
            "needs fixture.donatello: forge polls Donatello only with a seeded token",
        );
        if let Some((_, setup)) = donatello {
            for (index, gift) in setup.history.iter().enumerate() {
                self.check_donatello_gift(&format!("fakes.donatello.history[{index}]"), gift);
            }
        }
        let monobank = self.pair(
            "fakes.monobank",
            scenario.fixture.monobank.as_ref(),
            scenario.fakes.monobank.as_ref(),
            "is required because the fixture seeds a monobank token; without it forge would poll the real monobank",
            "needs fixture.monobank: the fake jar is the one the fixture seeds",
        );
        if let Some((_, setup)) = monobank {
            for (index, gift) in setup.history.iter().enumerate() {
                self.check_monobank_gift(&format!("fakes.monobank.history[{index}]"), gift);
            }
        }
    }
}
