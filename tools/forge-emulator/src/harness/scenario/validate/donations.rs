use super::{MAX_READY_MS, Validator};
use crate::harness::scenario::donation::{DonatelloGift, MonobankGift, OfflineGift};
use crate::harness::scenario::step::StepAction;

impl<'a> Validator<'a> {
    pub(super) fn check_donatello_gift(&mut self, location: &str, gift: &DonatelloGift) {
        if self.scenario.fakes.donatello.is_none() {
            self.report(
                location,
                "needs a fake Donatello: add fixture.donatello and fakes.donatello",
            );
        }
        self.not_blank(format!("{location}.id"), &gift.id);
        self.not_blank(format!("{location}.amount"), &gift.amount);
    }

    pub(super) fn check_monobank_gift(&mut self, location: &str, gift: &MonobankGift) {
        if self.scenario.fakes.monobank.is_none() {
            self.report(
                location,
                "needs a fake monobank: add fixture.monobank and fakes.monobank",
            );
        }
        self.not_blank(format!("{location}.id"), &gift.id);
        if gift.amount_minor <= 0 {
            self.report(
                format!("{location}.amount_minor"),
                "must be positive: a jar top-up is money coming in",
            );
        }
    }

    pub(super) fn check_donation_action(&mut self, location: &str, action: &StepAction) {
        let scenario = self.scenario;
        match action {
            StepAction::DonatelloDonation(gift) => self.check_donatello_gift(location, gift),
            StepAction::MonobankTopUp(gift) => self.check_monobank_gift(location, gift),
            StepAction::DonationsPolled { within_ms } => {
                if scenario.fakes.donatello.is_none() && scenario.fakes.monobank.is_none() {
                    self.report(
                        location,
                        "needs a fake donation service: add fakes.donatello or fakes.monobank",
                    );
                }
                self.check_within(location, *within_ms);
            }
            StepAction::ForgeRestart { within_ms, offline } => {
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_READY_MS);
                for (index, gift) in offline.iter().enumerate() {
                    let at = format!("{location}.offline[{index}]");
                    match gift {
                        OfflineGift::Donatello(gift) => self.check_donatello_gift(&at, gift),
                        OfflineGift::Monobank(gift) => self.check_monobank_gift(&at, gift),
                    }
                }
            }
            _ => {}
        }
    }
}
