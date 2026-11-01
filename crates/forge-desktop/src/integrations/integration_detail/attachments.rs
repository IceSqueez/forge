use super::{DonationSettingsLink, IntegrationDetail};
use crate::integrations::donation_settings::{DonationSettingsLaunch, DonationSettingsView};
use crate::integrations::integration_extensions::{DetailExtension, has_extension};
use crate::integrations::twitch_bot_presence::TwitchBotPresence;
use crate::integrations::twitch_bot_section::{StreamerSignInRequested, TwitchBotSection};
use forge_host::donation_services::DonationService;
use forge_platform_twitch::TwitchBotAccount;
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
            credentials: Arc::clone(&self.services.credentials),
            settings: Arc::clone(&self.services.settings),
            rt_handle: self.services.rt_handle.clone(),
        };
        let view = cx.new(|cx| DonationSettingsView::new(launch, cx));
        let repaint = cx.observe(&view, |_, _, cx| cx.notify());
        self.sign_in.donation_settings = Some(DonationSettingsLink {
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
        if !has_extension(self.ports.status.id(), DetailExtension::TwitchBotSection) {
            return;
        }
        let rt_handle = self.services.rt_handle.clone();
        let streamer_login = self.ports.status.hero_name();
        let section =
            cx.new(|cx| TwitchBotSection::new(bot, presence, rt_handle, streamer_login, cx));
        self.sign_in._twitch_bot_subs = vec![
            cx.subscribe(
                &section,
                |this, _section, _: &StreamerSignInRequested, cx| this.sign_out(cx),
            ),
            cx.observe(&section, |_, _, cx| cx.notify()),
        ];
        self.sign_in.twitch_bot = Some(section);
        cx.notify();
    }
}
