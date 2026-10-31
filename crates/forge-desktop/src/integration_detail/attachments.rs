use super::{DonationSettingsLink, IntegrationDetail};
use crate::donation_services::DonationService;
use crate::donation_settings::{DonationSettingsLaunch, DonationSettingsView};
use crate::platform_credentials::PlatformCredentialManagers;
use crate::twitch_bot_presence::TwitchBotPresence;
use crate::twitch_bot_section::{StreamerSignInRequested, TwitchBotSection};
use forge_platform_twitch::TwitchBotAccount;
use forge_types::PlatformId;
use gpui::{Context, Entity, prelude::*};
use std::sync::Arc;

impl IntegrationDetail {
    pub(crate) fn attach_donation_settings(
        &mut self,
        service: DonationService,
        cx: &mut Context<Self>,
    ) {
        let launch = DonationSettingsLaunch {
            service,
            credentials: Arc::clone(&self.credentials),
            settings: Arc::clone(&self.settings),
            rt_handle: self.rt_handle.clone(),
        };
        let view = cx.new(|cx| DonationSettingsView::new(launch, cx));
        let repaint = cx.observe(&view, |_, _, cx| cx.notify());
        self.donation_settings = Some(DonationSettingsLink {
            view,
            _repaint: repaint,
        });
        cx.notify();
    }

    pub(crate) fn attach_twitch_bot(
        &mut self,
        bot: Arc<TwitchBotAccount>,
        presence: Entity<TwitchBotPresence>,
        cx: &mut Context<Self>,
    ) {
        if !self.is_twitch {
            return;
        }
        let rt_handle = self.rt_handle.clone();
        let streamer_login = self.status.hero_name();
        let section =
            cx.new(|cx| TwitchBotSection::new(bot, presence, rt_handle, streamer_login, cx));
        self._twitch_bot_subs = vec![
            cx.subscribe(
                &section,
                |this, _section, _: &StreamerSignInRequested, cx| {
                    this.reset_to_connect(PlatformId::Twitch, cx)
                },
            ),
            cx.observe(&section, |_, _, cx| cx.notify()),
        ];
        self.twitch_bot = Some(section);
        cx.notify();
    }

    pub(crate) fn attach_twitch_sign_in(&mut self, sign_in: forge_platform_twitch::TwitchSignIn) {
        self.twitch_sign_in = Some(sign_in);
    }

    pub(crate) fn attach_youtube_sign_in(
        &mut self,
        sign_in: forge_platform_youtube::YoutubeSignIn,
    ) {
        self.youtube_sign_in = Some(sign_in);
    }

    pub(crate) fn attach_kick_sign_in(&mut self, sign_in: forge_platform_kick::KickSignIn) {
        self.kick_sign_in = Some(sign_in);
    }

    pub(crate) fn attach_platform_credentials(&mut self, managers: PlatformCredentialManagers) {
        self.platform_credentials = managers;
    }
}
