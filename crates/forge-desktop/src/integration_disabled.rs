use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, body_family,
    ghost_button_with_icon, health_tile, hub_tile, icon, integration_inactive_badge, mono_family,
    page_frame, primary_button_with_icon, radius, tr,
};
use forge_platform_core::{ConnectionAffordance, IntegrationDeclaration};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_storage::{CredentialsRepo, DataProvider, has_credentials_for};
use forge_types::IntegrationId;
use gpui::{
    AnyElement, ClickEvent, Context, ElementId, Entity, EventEmitter, FontWeight, Pixels,
    SharedString, Subscription, Window, div, prelude::*, px, relative,
};

use crate::builtin_sections::grow_cell;
use crate::integration_catalog::{declaration_of, disclaimer_of, look_of};
use crate::integration_lifecycle::{CardStatus, IntegrationLifecycle};
use crate::integration_references::IntegrationReferences;
use crate::integration_supervisor::{IntegrationSlot, LifecycleState};
use crate::integrations_hub::{card_status_badge, load_references};
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

const BODY_PAD_V: Pixels = px(18.0);
const BODY_PAD_H: Pixels = px(22.0);
const HERO_PAD_V: Pixels = px(16.0);
const HERO_PAD_H: Pixels = px(18.0);
const HERO_MB: Pixels = px(14.0);
const HERO_GAP: Pixels = px(16.0);
const HERO_TILE: Pixels = px(48.0);
const HERO_TITLE: Pixels = px(16.0);
const HERO_TITLE_MB: Pixels = px(2.0);
const DISCLAIMER_GAP: Pixels = px(6.0);
const DISCLAIMER_ICON: Pixels = px(11.0);
const DISCLAIMER_ICON_MT: Pixels = px(2.0);
const DISCLAIMER_LINE_HEIGHT: f32 = 1.45;
const DISCLAIMER_MB: Pixels = px(14.0);
const PANEL_GAP: Pixels = px(12.0);
const AFFECTED_WEIGHT: f32 = 1.4;
const KEPT_WEIGHT: f32 = 1.0;
const PANEL_HEAD_PAD_V: Pixels = px(10.0);
const PANEL_HEAD_PAD_H: Pixels = px(14.0);
const PANEL_TITLE: Pixels = px(12.5);
const PANEL_TITLE_GAP: Pixels = px(7.0);
const PANEL_TITLE_ICON: Pixels = px(14.0);
const EMPTY_PAD_V: Pixels = px(18.0);
const ROWS_PAD_V: Pixels = px(6.0);
const ROW_PAD_V: Pixels = px(7.0);
const ROW_GAP: Pixels = px(10.0);
const ROW_HINT: Pixels = px(11.0);
const ROW_CHEVRON: Pixels = px(13.0);
const KEPT_PAD_TOP: Pixels = px(8.0);
const KEPT_PAD_BOTTOM: Pixels = px(12.0);
const KEPT_GAP: Pixels = px(6.0);
const KEPT_ROW_GAP: Pixels = px(8.0);
const KEPT_ICON: Pixels = px(12.0);

pub struct DisabledLaunch {
    pub slot: Option<IntegrationSlot>,
    pub backend: Arc<dyn DataProvider>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub triggers: Arc<TriggerRegistry>,
    pub rt_handle: tokio::runtime::Handle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    Credentials,
}

pub struct IntegrationDisabledView {
    id: IntegrationId,
    declaration: Option<IntegrationDeclaration>,
    lifecycle: Entity<IntegrationLifecycle>,
    launch: DisabledLaunch,
    references: IntegrationReferences,
    kept: Vec<Kept>,
    _lifecycle_observer: Subscription,
}

impl EventEmitter<NavRequested> for IntegrationDisabledView {}

impl IntegrationDisabledView {
    pub fn new(
        id: IntegrationId,
        lifecycle: Entity<IntegrationLifecycle>,
        launch: DisabledLaunch,
        cx: &mut Context<Self>,
    ) -> Self {
        let observer = cx.observe(&lifecycle, |_, _, cx| cx.notify());
        let mut view = Self {
            declaration: declaration_of(&id),
            id,
            lifecycle,
            launch,
            references: IntegrationReferences::default(),
            kept: Vec::new(),
            _lifecycle_observer: observer,
        };
        view.load(cx);
        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let backend = Arc::clone(&self.launch.backend);
        let sub_actions = Arc::clone(&self.launch.sub_actions);
        let triggers = Arc::clone(&self.launch.triggers);
        let id = self.id.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.launch.rt_handle.spawn(async move {
            let references =
                match load_references(backend.as_ref(), &sub_actions, &triggers).await {
                    Ok(mut all) => all.remove(&id).unwrap_or_default(),
                    Err(e) => {
                        tracing::warn!(integration = %id, error = %e, "could not count integration references");
                        IntegrationReferences::default()
                    }
                };
            let credentials = Arc::clone(&backend) as Arc<dyn CredentialsRepo>;
            let has_credentials = match has_credentials_for(credentials.as_ref(), &id).await {
                Ok(present) => present,
                Err(e) => {
                    tracing::warn!(integration = %id, error = %e, "could not check stored credentials");
                    false
                }
            };
            let _ = tx.send((references, has_credentials));
        });
        cx.spawn(async move |this, cx| {
            let Ok((references, has_credentials)) = rx.await else {
                return;
            };
            let _ = this.update(cx, |view, cx| {
                view.apply_loaded(references, has_credentials);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn apply_loaded(&mut self, references: IntegrationReferences, has_credentials: bool) {
        self.references = references;
        self.kept = if has_credentials {
            vec![Kept::Credentials]
        } else {
            Vec::new()
        };
    }

    pub fn enable(&mut self, cx: &mut Context<Self>) {
        let Some(slot) = self.launch.slot.clone() else {
            return;
        };
        self.launch.rt_handle.spawn(async move {
            if let Err(e) = slot.set_enabled(true).await {
                tracing::warn!(integration = %slot.id(), error = %e, "could not enable the integration");
            }
        });
        cx.notify();
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

    fn crumbs(&self, cx: &mut Context<Self>) -> Vec<BreadcrumbCrumb> {
        vec![
            BreadcrumbCrumb::link(
                tr!("integrations_breadcrumb"),
                "integration-disabled-crumb-hub",
                cx.listener(|this, _: &ClickEvent, _, cx| this.go(Screen::Integrations(None), cx)),
            ),
            BreadcrumbCrumb::leaf(self.name()),
        ]
    }

    fn hero(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let name = self.name();
        let starting = matches!(
            self.state(cx),
            LifecycleState::Starting | LifecycleState::Stopping
        );
        let all =
            ghost_button_with_icon(Icon::LayoutGrid, tr!("integration_disabled_all"), palette)
                .on_click(
                    "integration-disabled-all",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.go(Screen::Integrations(None), cx)
                    }),
                );
        let enable = primary_button_with_icon(
            Icon::Power,
            tr!("integration_disabled_enable", name = name.to_string()),
            palette,
        )
        .busy(starting)
        .on_click(
            "integration-disabled-enable",
            cx.listener(|this, _: &ClickEvent, _, cx| this.enable(cx)),
        );
        hero_card(
            &self.id,
            tr!("integration_disabled_title", name = name.to_string()),
            div()
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("integration_disabled_lead"))
                .into_any_element(),
            vec![all.into_any_element(), enable.into_any_element()],
            palette,
        )
    }

    fn disclaimer(&self, palette: &ForgePalette) -> Option<AnyElement> {
        disclaimer_block(&self.id, palette)
    }
}

pub fn hero_card(
    id: &IntegrationId,
    title: String,
    lead: AnyElement,
    actions: Vec<AnyElement>,
    palette: &ForgePalette,
) -> AnyElement {
    let look = look_of(id, palette);
    div()
        .mb(HERO_MB)
        .flex()
        .items_center()
        .gap(HERO_GAP)
        .py(HERO_PAD_V)
        .px(HERO_PAD_H)
        .bg(palette.shell)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .rounded(radius(Radius::Md))
        .child(hub_tile(
            look.tile_glyph(),
            look.tint,
            true,
            HERO_TILE,
            palette,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(body_family())
                .child(
                    div()
                        .mb(HERO_TITLE_MB)
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(HERO_TITLE)
                        .text_color(palette.text_primary)
                        .child(title),
                )
                .child(lead),
        )
        .children(actions)
        .into_any_element()
}

pub fn disclaimer_block(id: &IntegrationId, palette: &ForgePalette) -> Option<AnyElement> {
    let disclaimer = disclaimer_of(id)?;
    Some(
        div()
            .mb(DISCLAIMER_MB)
            .flex()
            .items_start()
            .gap(DISCLAIMER_GAP)
            .font_family(body_family())
            .text_size(FONT_XXS)
            .line_height(relative(DISCLAIMER_LINE_HEIGHT))
            .text_color(palette.text_muted)
            .child(div().mt(DISCLAIMER_ICON_MT).child(icon(
                Icon::AlertTriangle,
                DISCLAIMER_ICON,
                palette.warning,
            )))
            .child(div().flex_1().min_w_0().child(tr!(disclaimer.full_key)))
            .into_any_element(),
    )
}

impl IntegrationDisabledView {
    fn affected(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let name = self.name();
        let total = self.references.total();
        let head = panel_head(palette)
            .justify_between()
            .child(panel_title(
                Icon::AlertTriangle,
                palette.warning,
                tr!("integration_disabled_affected"),
                palette,
            ))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(SharedString::from(total.to_string())),
            );
        let body = if total == 0 {
            empty_line(
                tr!("integration_disabled_unused", name = name.to_string()),
                palette,
            )
            .py(EMPTY_PAD_V)
            .into_any_element()
        } else {
            let mut rows = div().py(ROWS_PAD_V).flex().flex_col();
            for (index, action) in self.references.actions.iter().enumerate() {
                rows = rows.child(
                    affected_row(
                        ElementId::NamedInteger("integration-disabled-action".into(), index as u64),
                        palette,
                    )
                    .child(health_tile(Icon::PlugOff, palette.warning))
                    .child(
                        div()
                            .flex_none()
                            .font_family(mono_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(SharedString::from(action.name.clone())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(body_family())
                            .text_size(ROW_HINT)
                            .text_color(palette.text_faint)
                            .child(tr!(
                                "integration_disabled_steps_fail",
                                name = name.to_string()
                            )),
                    )
                    .child(icon(Icon::ChevronRight, ROW_CHEVRON, palette.text_faint))
                    .on_click({
                        let action_id = action.id;
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.go(Screen::Actions(Some(action_id)), cx)
                        })
                    }),
                );
            }
            if self.references.triggers > 0 {
                rows = rows.child(
                    affected_row("integration-disabled-triggers".into(), palette)
                        .child(integration_inactive_badge(
                            tr!("integration_inactive_short"),
                            palette,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(body_family())
                                .text_size(FONT_XS)
                                .text_color(palette.text_secondary)
                                .child(tr!(
                                    "integration_disabled_triggers_idle",
                                    count = self.references.triggers as i64
                                )),
                        )
                        .child(icon(Icon::ChevronRight, ROW_CHEVRON, palette.text_faint))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.go(Screen::Triggers(None), cx)
                        })),
                );
            }
            rows.into_any_element()
        };
        panel(palette).child(head).child(body).into_any_element()
    }

    fn kept_panel(&self, palette: &ForgePalette) -> AnyElement {
        let head = panel_head(palette).child(panel_title(
            Icon::Lock,
            palette.success,
            tr!("integration_disabled_kept"),
            palette,
        ));
        let mut body = div()
            .pt(KEPT_PAD_TOP)
            .pb(KEPT_PAD_BOTTOM)
            .px(PANEL_HEAD_PAD_H)
            .flex()
            .flex_col()
            .gap(KEPT_GAP);
        if self.kept.is_empty() {
            body = body.child(empty_line(
                tr!("integration_disabled_nothing_kept"),
                palette,
            ));
        }
        for kept in &self.kept {
            let label = match kept {
                Kept::Credentials => match self.declaration.as_ref().map(|d| d.connection) {
                    Some(ConnectionAffordance::Connectable) => tr!("integration_kept_sign_in"),
                    _ => tr!("integration_kept_credentials"),
                },
            };
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(KEPT_ROW_GAP)
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(icon(Icon::Check, KEPT_ICON, palette.success))
                    .child(label),
            );
        }
        panel(palette).child(head).child(body).into_any_element()
    }
}

fn panel(palette: &ForgePalette) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .h_full()
        .flex_col()
        .bg(palette.elevated)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .rounded(radius(Radius::Md))
        .overflow_hidden()
}

fn panel_head(palette: &ForgePalette) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .py(PANEL_HEAD_PAD_V)
        .px(PANEL_HEAD_PAD_H)
        .border_b(BORDER_THIN)
        .border_color(palette.border_regular)
}

fn panel_title(
    glyph: Icon,
    tint: gpui::Rgba,
    label: String,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(PANEL_TITLE_GAP)
        .font_family(body_family())
        .font_weight(FontWeight::MEDIUM)
        .text_size(PANEL_TITLE)
        .text_color(palette.text_primary)
        .child(icon(glyph, PANEL_TITLE_ICON, tint))
        .child(label)
}

fn empty_line(text: String, palette: &ForgePalette) -> gpui::Div {
    div()
        .px(PANEL_HEAD_PAD_H)
        .font_family(body_family())
        .text_size(FONT_XS)
        .italic()
        .text_color(palette.text_faint)
        .child(text)
}

fn affected_row(id: ElementId, palette: &ForgePalette) -> gpui::Stateful<gpui::Div> {
    let hover = palette.surface_overlay;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(ROW_GAP)
        .py(ROW_PAD_V)
        .px(PANEL_HEAD_PAD_H)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
}

impl Render for IntegrationDisabledView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let status =
            CardStatus::resolve(&self.state(cx), ConnectionAffordance::Connectionless, false);
        let badge = card_status_badge(&self.id, &status, &palette);

        let panels = div()
            .w_full()
            .flex()
            .gap(PANEL_GAP)
            .child(grow_cell(self.affected(&palette, cx), AFFECTED_WEIGHT))
            .child(grow_cell(self.kept_panel(&palette), KEPT_WEIGHT));

        let body = div()
            .id("integration-disabled-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(BODY_PAD_V)
                    .px(BODY_PAD_H)
                    .child(self.hero(&palette, cx))
                    .children(self.disclaimer(&palette))
                    .child(panels),
            );

        let crumbs = self.crumbs(cx);
        page_frame(crumbs, &palette).header_right(badge).body(body)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use forge_storage::{CredentialId, SettingsRepo, integration_enabled_key};
    use forge_types::ActionId;
    use gpui::{Modifiers, TestAppContext, VisualTestContext, point, size};

    use super::*;
    use crate::integration_references::ReferencingAction;
    use crate::integration_supervisor::LifecycleStates;
    use crate::presentation::Presentation;
    use crate::test_support::{
        Sandboxed, idle_supervisor, link, owned_sub_actions, owned_triggers, pump, runtime,
        sandboxed_provider, seed_action, seed_trigger, step, test_backend,
    };

    const OBS_SCENE: &str = "obs.scene.set";
    const TWITCH_CHAT: &str = "twitch.chat";
    const SETTLE_ROUNDS: usize = 500;
    const WINDOW_W: f32 = 1000.0;
    const WINDOW_H: f32 = 800.0;
    const SCAN_X: f32 = 120.0;
    const SCAN_STEP: f32 = 4.0;

    fn obs() -> IntegrationId {
        IntegrationId::new("obs")
    }

    fn kick() -> IntegrationId {
        IntegrationId::new("kick")
    }

    fn seeded_storage(
        rt: &tokio::runtime::Runtime,
    ) -> (Sandboxed<Arc<dyn DataProvider>>, ActionId) {
        rt.block_on(async {
            let storage = sandboxed_provider().await;
            let scene = seed_action(&storage, "Scene", vec![step(OBS_SCENE, true)]).await;
            let chat = seed_trigger(&storage, TWITCH_CHAT).await;
            link(&storage, scene, chat).await;
            (Arc::clone(&storage) as Arc<dyn CredentialsRepo>)
                .store(&CredentialId::new("obs:password"), "hunter2")
                .await
                .unwrap();
            (storage, scene)
        })
    }

    fn open(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        storage: &Sandboxed<Arc<dyn DataProvider>>,
        id: IntegrationId,
        slot: Option<IntegrationSlot>,
    ) -> Entity<IntegrationDisabledView> {
        let launch = DisabledLaunch {
            slot,
            backend: Arc::clone(storage),
            sub_actions: Arc::new(owned_sub_actions(&[(OBS_SCENE, "obs")])),
            triggers: Arc::new(owned_triggers(&[(TWITCH_CHAT, "twitch")])),
            rt_handle: rt.handle().clone(),
        };
        cx.update(|cx| {
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(LifecycleStates::new()));
            cx.new(|cx| IntegrationDisabledView::new(id, lifecycle, launch, cx))
        })
    }

    fn settle_until(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        view: &Entity<IntegrationDisabledView>,
        loaded: impl Fn(&IntegrationDisabledView) -> bool,
    ) -> bool {
        (0..SETTLE_ROUNDS).any(|_| {
            rt.block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            cx.run_until_parked();
            view.read_with(cx, |view, _| loaded(view))
        })
    }

    #[gpui::test]
    fn the_page_lists_what_the_integration_breaks_and_keeps_its_sign_in(cx: &mut TestAppContext) {
        let rt = runtime();
        let (storage, scene) = seeded_storage(&rt);
        let view = open(cx, &rt, &storage, obs(), None);

        let loaded = settle_until(cx, &rt, &view, |view| !view.kept.is_empty());

        assert!(loaded, "the page never loaded what is kept");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.references.actions,
                vec![ReferencingAction {
                    id: scene,
                    name: "Scene".to_owned(),
                }]
            );
            assert_eq!(view.references.triggers, 0);
            assert_eq!(view.kept, vec![Kept::Credentials]);
        });
    }

    #[gpui::test]
    fn the_page_of_an_unused_integration_with_no_sign_in_lists_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let (storage, _) = seeded_storage(&rt);
        let view = open(cx, &rt, &storage, kick(), None);
        view.update(cx, |view, _| {
            view.apply_loaded(
                IntegrationReferences {
                    actions: vec![ReferencingAction {
                        id: ActionId::new(),
                        name: "stale".to_owned(),
                    }],
                    triggers: 1,
                },
                true,
            )
        });

        let loaded = settle_until(cx, &rt, &view, |view| view.kept.is_empty());

        assert!(loaded, "the page never replaced the stale listing");
        view.read_with(cx, |view, _| assert_eq!(view.references.total(), 0));
    }

    #[gpui::test]
    fn enabling_from_the_page_switches_the_integration_on(cx: &mut TestAppContext) {
        let rt = runtime();
        let (storage, _) = seeded_storage(&rt);
        let (settings, mut writes) = test_backend();
        let supervisor = idle_supervisor(&rt, settings as Arc<dyn SettingsRepo>, &[obs()]);
        pump(&rt);
        while writes.try_recv().is_ok() {}
        let view = open(cx, &rt, &storage, obs(), supervisor.slot(&obs()));

        view.update(cx, |view, cx| view.enable(cx));
        pump(&rt);

        let written: Vec<_> = std::iter::from_fn(|| writes.try_recv().ok()).collect();
        assert_eq!(
            written,
            vec![(integration_enabled_key(&obs()), "true".to_owned())]
        );
    }

    struct Heard {
        screens: Vec<Screen>,
        _sub: gpui::Subscription,
    }

    #[gpui::test]
    fn clicking_an_affected_action_opens_that_action(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Presentation::new(
                forge_components::ThemeId::ForgeDefault,
                forge_components::Density::Cozy,
            ))
        });
        let rt = runtime();
        let (storage, scene) = seeded_storage(&rt);
        let launch = DisabledLaunch {
            slot: None,
            backend: Arc::clone(&storage),
            sub_actions: Arc::new(owned_sub_actions(&[(OBS_SCENE, "obs")])),
            triggers: Arc::new(owned_triggers(&[(TWITCH_CHAT, "twitch")])),
            rt_handle: rt.handle().clone(),
        };
        let (view, vcx) = cx.add_window_view(|_window, cx| {
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(LifecycleStates::new()));
            IntegrationDisabledView::new(obs(), lifecycle, launch, cx)
        });
        let heard = vcx.update(|_window, cx| {
            cx.new(|cx| Heard {
                screens: Vec::new(),
                _sub: cx.subscribe(&view, |heard: &mut Heard, _, event: &NavRequested, _| {
                    heard.screens.push(event.0.clone());
                }),
            })
        });
        vcx.simulate_resize(size(px(WINDOW_W), px(WINDOW_H)));
        let loaded = (0..SETTLE_ROUNDS).any(|_| {
            rt.block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            vcx.run_until_parked();
            view.read_with(vcx, |view, _| !view.references.actions.is_empty())
        });
        assert!(loaded, "the page never listed the affected action");

        scan_column(vcx);

        let opened: Vec<Screen> = heard.read_with(vcx, |heard, _| {
            heard
                .screens
                .iter()
                .filter(|screen| matches!(screen, Screen::Actions(_)))
                .cloned()
                .collect()
        });
        assert!(!opened.is_empty(), "no click on the page opened an action");
        assert!(
            opened
                .iter()
                .all(|screen| *screen == Screen::Actions(Some(scene))),
            "{opened:?}"
        );
    }

    fn scan_column(vcx: &mut VisualTestContext) {
        let mut y = 0.0;
        while y < WINDOW_H {
            vcx.simulate_click(point(px(SCAN_X), px(y)), Modifiers::none());
            y += SCAN_STEP;
        }
    }
}
