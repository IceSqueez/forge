use super::IntegrationsHubView;
use crate::app::navigation::Screen;
use crate::app::start_failure::{StartRemedy, start_failure_text, start_failure_tooltip};
use crate::integrations::hub_card::{
    CompactTone, HubNote, compact_button, hub_note, keeping_card_click,
};
use crate::integrations::integration_catalog::activity_key;
use crate::integrations::integration_lifecycle::CardStatus;
use crate::shared::user_text::resolve_message;
use forge_components::{Density, ForgePalette, Icon, toggle, tr};
use forge_platform_core::{IntegrationDeclaration, IntegrationDisclaimer};
use forge_types::IntegrationId;
use gpui::{ClickEvent, Context, ElementId, prelude::*};

impl IntegrationsHubView {
    pub(super) fn card_switch(
        id: &IntegrationId,
        on: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let toggle_id = id.clone();
        toggle(on, palette).on_click(
            ElementId::Name(format!("hub-toggle-{id}").into()),
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.request_enabled(toggle_id.clone(), !on, cx)
            }),
        )
    }

    pub(super) fn card_note(
        &self,
        id: &IntegrationId,
        status: &CardStatus,
        disclaimer: Option<&IntegrationDisclaimer>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<HubNote> {
        match (status, disclaimer) {
            (CardStatus::Failed(error), None) => Some(
                hub_note(
                    Icon::AlertCircle,
                    palette.danger,
                    start_failure_text(error).primary,
                )
                .emphasized()
                .tooltip(start_failure_tooltip(error))
                .action(self.remedy_link(id, StartRemedy::of(error), palette, cx)),
            ),
            (CardStatus::NotConnected, None) => self.connect_failure_of(id).map(|failure| {
                let text = resolve_message(&failure);
                hub_note(Icon::AlertTriangle, palette.warning, text.primary.clone())
                    .tooltip(text.inline())
            }),
            (_, Some(disclaimer)) => Some(
                hub_note(
                    Icon::AlertTriangle,
                    palette.warning,
                    tr!(disclaimer.short_key),
                )
                .tooltip(tr!(disclaimer.full_key)),
            ),
            (CardStatus::Active, None) => self.activity.get(id).map(|count| {
                hub_note(
                    Icon::CircleCheck,
                    palette.success,
                    tr!(
                        &activity_key(id),
                        count = i64::try_from(*count).unwrap_or(i64::MAX)
                    ),
                )
            }),
            _ => None,
        }
    }

    pub(super) fn footer_retry(
        id: &IntegrationId,
        status: &CardStatus,
        has_disclaimer: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        match (status, has_disclaimer) {
            (CardStatus::Failed(error), true) => {
                let remedy = StartRemedy::of(error);
                let remedy_id = id.clone();
                Some(
                    compact_button(
                        CompactTone::Primary,
                        remedy.icon(),
                        Some(remedy.label().into()),
                        palette,
                        density,
                    )
                    .tooltip(start_failure_tooltip(error))
                    .on_click(
                        ElementId::Name(format!("hub-remedy-{id}").into()),
                        keeping_card_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.apply_remedy(&remedy_id, remedy, cx)
                        })),
                    ),
                )
            }
            _ => None,
        }
    }

    pub(super) fn connection_button(
        id: &IntegrationId,
        connected: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let connect_id = id.clone();
        if connected {
            compact_button(
                CompactTone::Ghost,
                Icon::PlugOff,
                Some(tr!("integration_disconnect").into()),
                palette,
                density,
            )
            .on_click(
                ElementId::Name(format!("hub-disconnect-{id}").into()),
                keeping_card_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_connected(&connect_id, false, cx)
                })),
            )
        } else {
            compact_button(
                CompactTone::Primary,
                Icon::Plug,
                Some(tr!("integration_connect").into()),
                palette,
                density,
            )
            .on_click(
                ElementId::Name(format!("hub-connect-{id}").into()),
                keeping_card_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_connected(&connect_id, true, cx)
                })),
            )
        }
    }

    pub(super) fn settings_button(
        declaration: &IntegrationDeclaration,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = &declaration.id;
        let settings_target = Screen::BuiltinDetail(id.clone());
        compact_button(CompactTone::Ghost, Icon::Settings, None, palette, density)
            .tooltip(tr!(
                "integration_settings_tooltip",
                name = declaration.brand_name
            ))
            .on_click(
                ElementId::Name(format!("hub-settings-{id}").into()),
                keeping_card_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.go(settings_target.clone(), cx)
                })),
            )
    }
}
