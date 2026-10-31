use super::*;
use crate::pack_export::PackExportFlow;
use crate::transfer_flows::TransferFlows;

pub(super) struct PackExportHost {
    pub(super) flow: Entity<PackExportFlow>,
    transfers: Entity<TransferFlows>,
    _observe: Subscription,
}

impl ScreenActionsView {
    #[must_use]
    pub(crate) fn with_pack_export(mut self, cx: &mut Context<Self>) -> Self {
        if let Some(transfers) = TransferFlows::route(cx) {
            let flow = transfers.read(cx).pack_export.clone();
            let observe = cx.observe(&flow, |this, _, cx| {
                this.sync_step_drag_lock(cx);
                cx.notify();
            });
            self.modals.pack_export = Some(PackExportHost {
                flow,
                transfers,
                _observe: observe,
            });
        }
        self
    }

    pub(super) fn export_pack(&mut self, id: ActionId, cx: &mut Context<Self>) {
        if let Some(host) = self.modals.pack_export.as_ref() {
            host.transfers
                .update(cx, |transfers, cx| transfers.export_pack_of(vec![id], cx));
        }
        cx.notify();
    }
}
