use super::*;
use crate::pack_export::PackExportFlow;
use crate::pack_services::PackServices;

pub(in crate::actions_screen) struct PackExportHost {
    pub(in crate::actions_screen) flow: Entity<PackExportFlow>,
    _observe: Subscription,
}

impl ScreenActionsView {
    #[must_use]
    pub fn with_pack_export(mut self, services: PackServices, cx: &mut Context<Self>) -> Self {
        let flow = cx.new(|_| PackExportFlow::new(services));
        let observe = cx.observe(&flow, |_, _, cx| cx.notify());
        self.pack_export = Some(PackExportHost {
            flow,
            _observe: observe,
        });
        self
    }

    pub(super) fn export_pack(&mut self, cx: &mut Context<Self>) {
        self.header_menu_open = None;
        let action = self.detail.as_ref().map(|detail| detail.action.id);
        if let (Some(id), Some(host)) = (action, self.pack_export.as_ref()) {
            host.flow.update(cx, |flow, cx| flow.start(vec![id], cx));
        }
        cx.notify();
    }
}
