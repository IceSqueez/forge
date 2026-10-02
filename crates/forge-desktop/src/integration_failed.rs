use forge_components::{
    BreadcrumbCrumb, FONT_XS, ForgePalette, Icon, ToastKind, body_family, ghost_button_with_icon,
    icon, mono_family, page_frame, primary_button_with_icon, tr,
};
use forge_platform_core::{ConnectionAffordance, IntegrationDeclaration};
use forge_types::IntegrationId;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px, relative,
};

use crate::async_bridge;
use crate::hub_crumb::hub_crumb;
use crate::integration_catalog::declaration_of;
use crate::integration_disabled::{disclaimer_block, hero_card};
use crate::integration_lifecycle::{CardStatus, IntegrationLifecycle};
use crate::integration_supervisor::{IntegrationSlot, LifecycleState};
use crate::integrations_hub::card_status_badge;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;

const BODY_PAD_V: Pixels = px(18.0);
const BODY_PAD_H: Pixels = px(22.0);
const REASON_MT: Pixels = px(6.0);
const REASON_GAP: Pixels = px(6.0);
const REASON_ICON: Pixels = px(12.0);
const REASON_ICON_MT: Pixels = px(2.0);
const REASON_LINE_HEIGHT: f32 = 1.45;

pub struct IntegrationFailedView {
    id: IntegrationId,
    declaration: Option<IntegrationDeclaration>,
    lifecycle: Entity<IntegrationLifecycle>,
    slot: Option<IntegrationSlot>,
    rt_handle: tokio::runtime::Handle,
    _lifecycle_observer: Subscription,
}

pub struct SignInRequested;

impl EventEmitter<NavRequested> for IntegrationFailedView {}

impl EventEmitter<SignInRequested> for IntegrationFailedView {}

impl IntegrationFailedView {
    pub fn new(
        id: IntegrationId,
        lifecycle: Entity<IntegrationLifecycle>,
        slot: Option<IntegrationSlot>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let observer = cx.observe(&lifecycle, |_, _, cx| cx.notify());
        Self {
            declaration: declaration_of(&id),
            id,
            lifecycle,
            slot,
            rt_handle,
            _lifecycle_observer: observer,
        }
    }

    pub fn retry(&mut self, cx: &mut Context<Self>) {
        let Some(slot) = self.slot.clone() else {
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move { slot.set_enabled(true).await },
            |this, outcome, cx| this.apply_retry_outcome(outcome, cx),
            cx,
        );
        cx.notify();
    }

    pub fn apply_retry_outcome(
        &mut self,
        outcome: Result<LifecycleState, String>,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = outcome {
            tracing::warn!(integration = %self.id, error = %e, "could not restart the integration");
            cx.push_toast(
                ToastKind::Error,
                tr!("integration_retry_failed", name = self.name().to_string()),
            );
        }
        cx.notify();
    }

    pub fn sign_in(&mut self, cx: &mut Context<Self>) {
        cx.emit(SignInRequested);
    }

    pub fn offers_sign_in(&self) -> bool {
        self.declaration
            .as_ref()
            .is_some_and(|d| d.connection == ConnectionAffordance::Connectable)
    }

    fn go(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

    fn name(&self) -> SharedString {
        match &self.declaration {
            Some(declaration) => SharedString::new_static(declaration.brand_name),
            None => SharedString::from(self.id.as_str().to_owned()),
        }
    }

    fn state(&self, cx: &Context<Self>) -> LifecycleState {
        self.lifecycle.read(cx).state_of(&self.id)
    }

    fn reason(&self, cx: &Context<Self>) -> Option<String> {
        match self.state(cx) {
            LifecycleState::Failed(reason) => Some(reason),
            _ => None,
        }
    }

    fn crumbs(&self, cx: &mut Context<Self>) -> Vec<BreadcrumbCrumb> {
        let category = self.declaration.as_ref().map(|d| d.category);
        vec![hub_crumb(category, cx), BreadcrumbCrumb::leaf(self.name())]
    }

    fn lead(&self, palette: &ForgePalette, cx: &Context<Self>) -> AnyElement {
        let reason = self
            .reason(cx)
            .unwrap_or_else(|| tr!("integration_failed_retrying"));
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("integration_failed_lead")),
            )
            .child(
                div()
                    .mt(REASON_MT)
                    .flex()
                    .items_start()
                    .gap(REASON_GAP)
                    .child(div().mt(REASON_ICON_MT).child(icon(
                        Icon::AlertCircle,
                        REASON_ICON,
                        palette.random,
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(mono_family())
                            .text_size(FONT_XS)
                            .line_height(relative(REASON_LINE_HEIGHT))
                            .text_color(palette.random)
                            .child(reason),
                    ),
            )
            .into_any_element()
    }

    fn hero(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let busy = matches!(
            self.state(cx),
            LifecycleState::Starting | LifecycleState::Stopping
        );
        let all =
            ghost_button_with_icon(Icon::LayoutGrid, tr!("integration_disabled_all"), palette)
                .on_click(
                    "integration-failed-all",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.go(Screen::Integrations(None), cx)
                    }),
                );
        let retry = primary_button_with_icon(Icon::Refresh, tr!("integration_retry"), palette)
            .busy(busy)
            .on_click(
                "integration-failed-retry",
                cx.listener(|this, _: &ClickEvent, _, cx| this.retry(cx)),
            );
        let mut actions = vec![all.into_any_element()];
        if self.offers_sign_in() {
            actions.push(
                ghost_button_with_icon(Icon::Plug, tr!("integration_failed_sign_in"), palette)
                    .on_click(
                        "integration-failed-sign-in",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.sign_in(cx)),
                    )
                    .into_any_element(),
            );
        }
        actions.push(retry.into_any_element());
        hero_card(
            &self.id,
            tr!("integration_failed_title", name = self.name().to_string()),
            self.lead(palette, cx),
            actions,
            palette,
        )
    }
}

impl Render for IntegrationFailedView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let status =
            CardStatus::resolve(&self.state(cx), ConnectionAffordance::Connectionless, false);
        let badge = card_status_badge(&self.id, &status, &palette);

        let body = div()
            .id("integration-failed-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(BODY_PAD_V)
                    .px(BODY_PAD_H)
                    .font_family(body_family())
                    .child(self.hero(&palette, cx))
                    .children(disclaimer_block(&self.id, &palette)),
            );

        let crumbs = self.crumbs(cx);
        page_frame(crumbs, &palette).header_right(badge).body(body)
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;
    use crate::integration_supervisor::LifecycleStates;
    use crate::test_support::{error_toasts, hub_crumb_targets, runtime};
    use crate::toasts::Toasts;

    fn failed(id: &IntegrationId) -> LifecycleStates {
        LifecycleStates::from([(id.clone(), LifecycleState::Failed("boom".to_owned()))])
    }

    fn open(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        id: IntegrationId,
    ) -> Entity<IntegrationFailedView> {
        let handle = rt.handle().clone();
        cx.update(|cx| {
            cx.set_global(Toasts::new());
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(failed(&id)));
            cx.new(|cx| IntegrationFailedView::new(id, lifecycle, None, handle, cx))
        })
    }

    #[gpui::test]
    fn sign_in_is_offered_only_for_integrations_that_hold_a_connection(cx: &mut TestAppContext) {
        let rt = runtime();
        let cases = [
            (forge_platform_twitch::TWITCH_INTEGRATION.id, true),
            (forge_platform_youtube::YOUTUBE_INTEGRATION.id, true),
            (forge_platform_kick::KICK_INTEGRATION.id, true),
            (forge_obs::OBS_INTEGRATION.id, true),
            (forge_vtube::VTUBE_INTEGRATION.id, true),
            (forge_discord::DISCORD_INTEGRATION.id, false),
            (forge_midi::MIDI_INTEGRATION.id, false),
            (forge_hotkey::HOTKEY_INTEGRATION.id, false),
            (IntegrationId::new("unknown"), false),
        ];

        for (id, expected) in cases {
            let view = open(cx, &rt, id.clone());
            let offered = view.read_with(cx, |view, _| view.offers_sign_in());
            assert_eq!(offered, expected, "{id}");
        }
    }

    #[gpui::test]
    fn a_retry_that_could_not_restart_raises_an_error_toast(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = open(cx, &rt, forge_obs::OBS_INTEGRATION.id);

        view.update(cx, |view, cx| {
            view.apply_retry_outcome(Err("port in use".to_owned()), cx)
        });

        assert_eq!(cx.update(|cx| error_toasts(cx)), 1);
    }

    #[gpui::test]
    fn a_retry_that_restarted_raises_no_toast(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = open(cx, &rt, forge_obs::OBS_INTEGRATION.id);

        view.update(cx, |view, cx| {
            view.apply_retry_outcome(Ok(LifecycleState::Running), cx)
        });

        assert_eq!(cx.update(|cx| error_toasts(cx)), 0);
    }

    #[gpui::test]
    fn the_crumb_leads_back_to_the_integration_category_on_the_hub(cx: &mut TestAppContext) {
        let rt = runtime();
        let handle = rt.handle().clone();
        let kick = forge_platform_kick::KICK_INTEGRATION;

        let targets = hub_crumb_targets(cx, |_, cx| {
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(failed(&kick.id)));
            IntegrationFailedView::new(kick.id.clone(), lifecycle, None, handle, cx)
        });

        assert!(!targets.is_empty(), "no click reached the crumb");
        assert!(
            targets
                .iter()
                .all(|screen| *screen == Screen::Integrations(Some(kick.category))),
            "{targets:?}"
        );
    }
}
