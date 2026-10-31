use forge_components::Spacing;
use forge_components::spacing;
use forge_components::{
    BreadcrumbCrumb, FONT_XS, ForgePalette, Icon, ToastKind, body_family, ghost_button_with_icon,
    icon, mono_family, page_frame, primary_button_with_icon, scaled, tr,
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
use crate::integrations::BuiltinObject;
use crate::integrations_hub::card_status_badge;
use crate::navigation::NavRequested;
use crate::navigation::Screen;
use crate::start_failure::{StartRemedy, start_failure_text};
use forge_components::ActivePresentation;
use forge_components::PushToast;
use forge_runtime::{IntegrationSlot, IntegrationStartError, LifecycleState};

const BODY_PAD_V: Pixels = px(18.0);
const BODY_PAD_H: Pixels = px(22.0);
const REASON_ICON: Pixels = px(12.0);
const REASON_ICON_MT: Pixels = px(2.0);
const REASON_LINE_HEIGHT: f32 = 1.45;

pub(crate) struct IntegrationFailedView {
    id: IntegrationId,
    declaration: Option<IntegrationDeclaration>,
    lifecycle: Entity<IntegrationLifecycle>,
    slot: Option<IntegrationSlot<BuiltinObject>>,
    rt_handle: tokio::runtime::Handle,
    _lifecycle_observer: Subscription,
}

pub(crate) struct LiveDetailRequested(pub IntegrationId);

impl EventEmitter<NavRequested> for IntegrationFailedView {}

impl EventEmitter<LiveDetailRequested> for IntegrationFailedView {}

impl IntegrationFailedView {
    pub(crate) fn new(
        id: IntegrationId,
        lifecycle: Entity<IntegrationLifecycle>,
        slot: Option<IntegrationSlot<BuiltinObject>>,
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

    pub(crate) fn retry(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn apply_retry_outcome(
        &mut self,
        outcome: Result<LifecycleState, IntegrationStartError>,
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

    pub(crate) fn open_live_detail(&mut self, cx: &mut Context<Self>) {
        cx.emit(LiveDetailRequested(self.id.clone()));
    }

    pub(crate) fn offers_sign_in(&self) -> bool {
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

    fn failure(&self, cx: &Context<Self>) -> Option<IntegrationStartError> {
        match self.state(cx) {
            LifecycleState::Failed(error) => Some(error),
            _ => None,
        }
    }

    fn crumbs(&self, cx: &mut Context<Self>) -> Vec<BreadcrumbCrumb> {
        let category = self.declaration.as_ref().map(|d| d.category);
        vec![hub_crumb(category, cx), BreadcrumbCrumb::leaf(self.name())]
    }

    fn lead(&self, palette: &ForgePalette, cx: &Context<Self>) -> AnyElement {
        let density = cx.density();
        let (reason, detail) = match self.failure(cx) {
            Some(error) => {
                let text = start_failure_text(&error);
                (text.primary, text.secondary)
            }
            None => (tr!("integration_failed_retrying"), None),
        };
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
                    .mt(spacing(Spacing::Xs, density))
                    .flex()
                    .items_start()
                    .gap(spacing(Spacing::Xs, density))
                    .child(div().mt(REASON_ICON_MT).child(icon(
                        Icon::AlertCircle,
                        REASON_ICON,
                        palette.danger,
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(mono_family())
                            .text_size(FONT_XS)
                            .line_height(relative(REASON_LINE_HEIGHT))
                            .text_color(palette.danger)
                            .child(reason),
                    ),
            )
            .children(detail.map(|detail| {
                div()
                    .mt(spacing(Spacing::Xs, density))
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .line_height(relative(REASON_LINE_HEIGHT))
                    .text_color(palette.text_muted)
                    .child(detail)
            }))
            .into_any_element()
    }

    fn hero(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
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
        let remedy = self
            .failure(cx)
            .map_or(StartRemedy::Retry, |error| StartRemedy::of(&error));
        let mut actions = vec![all.into_any_element()];
        if remedy.opens_detail() {
            actions.push(
                ghost_button_with_icon(Icon::Refresh, tr!("integration_retry"), palette)
                    .on_click(
                        "integration-failed-retry",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.retry(cx)),
                    )
                    .into_any_element(),
            );
            actions.push(
                primary_button_with_icon(remedy.icon(), remedy.label(), palette)
                    .busy(busy)
                    .on_click(
                        "integration-failed-remedy",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.open_live_detail(cx)),
                    )
                    .into_any_element(),
            );
        } else {
            if self.offers_sign_in() {
                actions.push(
                    ghost_button_with_icon(Icon::Plug, tr!("integration_failed_sign_in"), palette)
                        .on_click(
                            "integration-failed-sign-in",
                            cx.listener(|this, _: &ClickEvent, _, cx| this.open_live_detail(cx)),
                        )
                        .into_any_element(),
                );
            }
            actions.push(
                primary_button_with_icon(Icon::Refresh, tr!("integration_retry"), palette)
                    .busy(busy)
                    .on_click(
                        "integration-failed-retry",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.retry(cx)),
                    )
                    .into_any_element(),
            );
        }
        hero_card(
            &self.id,
            tr!("integration_failed_title", name = self.name().to_string()),
            self.lead(palette, cx),
            actions,
            palette,
            density,
        )
    }
}

impl Render for IntegrationFailedView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        let status =
            CardStatus::resolve(&self.state(cx), ConnectionAffordance::Connectionless, false);
        let badge = card_status_badge(&self.id, &status, &palette, density);

        let body = div()
            .id("integration-failed-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(BODY_PAD_V, density))
                    .px(scaled(BODY_PAD_H, density))
                    .font_family(body_family())
                    .child(self.hero(&palette, cx))
                    .children(disclaimer_block(&self.id, &palette, density)),
            );

        let crumbs = self.crumbs(cx);
        page_frame(crumbs, &palette).header_right(badge).body(body)
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;
    use crate::test_support::{error_toasts, hub_crumb_targets, runtime};
    use forge_components::Toasts;
    use forge_runtime::LifecycleStates;

    fn failed(id: &IntegrationId) -> LifecycleStates {
        LifecycleStates::from([(
            id.clone(),
            LifecycleState::Failed(crate::test_support::start_failure("boom")),
        )])
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
            view.apply_retry_outcome(Err(crate::test_support::start_failure("port in use")), cx)
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
