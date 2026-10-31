use super::*;
use crate::pack_export::PackExportFlow;
use crate::transfer_flows::TransferFlows;

pub(in crate::actions_screen) struct PackExportHost {
    pub(in crate::actions_screen) flow: Entity<PackExportFlow>,
    transfers: Entity<TransferFlows>,
    _observe: Subscription,
}

impl ScreenActionsView {
    #[must_use]
    pub(crate) fn with_pack_export(mut self, cx: &mut Context<Self>) -> Self {
        if let Some(transfers) = TransferFlows::route(cx) {
            let flow = transfers.read(cx).pack_export.clone();
            let observe = cx.observe(&flow, |_, _, cx| cx.notify());
            self.pack_export = Some(PackExportHost {
                flow,
                transfers,
                _observe: observe,
            });
        }
        self
    }

    pub(super) fn export_pack(&mut self, cx: &mut Context<Self>) {
        self.header_menu_open = None;
        let action = self.detail.as_ref().map(|detail| detail.action.id);
        if let (Some(id), Some(host)) = (action, self.pack_export.as_ref()) {
            host.transfers
                .update(cx, |transfers, cx| transfers.export_pack_of(vec![id], cx));
        }
        cx.notify();
    }
}
