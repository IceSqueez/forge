mod hero;
mod panels;

use panels::GlanceCounts;

use forge_components::FONT_XL;
use forge_components::FONT_XXL;
use forge_components::RevealsFocusIn;
use forge_components::{
    ActivePresentation, BORDER_THIN, BreadcrumbCrumb, Density, FONT_LG, FONT_MD, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, Icon, KeyActivated, Radius, Spacing, card, dropdown, dropdown_row,
    dropdown_surface, empty_state, ghost_button_with_icon, grow_cell, icon, page_frame, pulse_dot,
    radius, scaled, spacing, sparkline, status_dot, tr,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FontWeight, Pixels, Rgba,
    ScrollHandle, SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::app::navigable_list::{ListCursor, ListStep, NavigableList, navigable_list};
use crate::app::navigation::NavRequested;
use crate::app::navigation::Screen;
use crate::audience::event_loss::EventLoss;
use crate::audience::event_loss_card::event_loss_card;
use crate::audience::home_stats::{ConnectionsTally, HomeEvent, HomeStats, ObsHealth};
use crate::data_flows::backup_flow::BackupFlow;
use crate::data_flows::pack_export::PackExportFlow;
use crate::data_flows::pack_import_flow::PackImportFlow;
use crate::data_flows::pack_services::PackServices;
use crate::data_flows::restore_flow::RestoreFlow;
use crate::data_flows::setup_export::SetupExportFlow;
use crate::data_flows::setup_import_flow::SetupImportFlow;
use crate::data_flows::transfer_flows::TransferFlows;
use crate::integrations::integration_lifecycle::IntegrationLifecycle;
use forge_components::{ClockStyle, DateShape, active_display_formats, fmt_display};
use forge_types::IntegrationId;

const RECENT_EVENT_ROWS: usize = 5;

#[derive(Clone, Debug, PartialEq)]
enum HomeRow {
    Jump(Screen),
    Connection(IntegrationId),
    Event(SharedString),
}

impl HomeRow {
    fn target(&self) -> Screen {
        match self {
            Self::Jump(target) => target.clone(),
            Self::Connection(integration) => Screen::BuiltinDetail(integration.clone()),
            Self::Event(_) => Screen::EventFeed,
        }
    }
}

fn jump_targets() -> [Screen; 3] {
    [
        Screen::Chat,
        Screen::Actions(None),
        Screen::Integrations(None),
    ]
}

fn event_time_width() -> Pixels {
    match active_display_formats().clock {
        ClockStyle::TwentyFourHour => EVENT_TIME_W,
        ClockStyle::TwelveHour => EVENT_TIME_W_TWELVE_HOUR,
    }
}

const BODY_PAD_V: Pixels = px(22.0);
const BODY_PAD_H: Pixels = px(28.0);

const HERO_PAD_V: Pixels = px(22.0);
const HERO_PAD_H: Pixels = px(24.0);
const HERO_BRAND_BOX: Pixels = px(54.0);
const HERO_BRAND_FONT: Pixels = px(26.0);
const HERO_TITLE_FONT: Pixels = FONT_XL;

const JUMP_PAD_H: Pixels = px(18.0);
const JUMP_ICON_BOX: Pixels = px(34.0);
const JUMP_ICON_RADIUS: Pixels = px(8.0);
const JUMP_ICON_GLYPH: Pixels = FONT_LG;
const JUMP_STAT_FONT: Pixels = FONT_XXL;
const JUMP_HINT_ARROW: Pixels = px(12.0);
const JUMP_WARN_ICON: Pixels = px(14.0);

const CONN_HEADER_PAD_V: Pixels = px(10.0);
const CONN_HEADER_PAD_H: Pixels = px(14.0);
const CONN_HEADER_ICON: Pixels = px(14.0);
const CONN_CELL_PAD_V: Pixels = px(12.0);
const CONN_CELL_PAD_H: Pixels = px(14.0);
const CONN_BRAND_DOT: Pixels = px(10.0);
const CONN_BRAND_DOT_RADIUS: Pixels = px(3.0);
const CONN_STATUS_DOT: Pixels = px(8.0);
const CELL_GAP: Pixels = px(1.0);

const HEALTH_ICON: Pixels = px(14.0);
const HEALTH_LIVE_DOT: Pixels = px(6.0);

const EVENT_ROW_PAD_V: Pixels = px(7.0);
const EVENT_TIME_W: Pixels = px(60.0);
const EVENT_TIME_W_TWELVE_HOUR: Pixels = px(84.0);
const EVENT_DOT: Pixels = px(6.0);
const EVENT_LIVE_DOT: Pixels = px(6.0);
const EVENT_HEADER_ICON: Pixels = px(14.0);

const GLANCE_ROW_PAD_V: Pixels = px(5.0);
const DIVIDER_H: Pixels = BORDER_THIN;
const GLANCE_HEADER_ICON: Pixels = px(14.0);
const RECENT_GROW: f32 = 1.4;
const GLANCE_GROW: f32 = 1.0;

const EXPORT_MENU_W: Pixels = px(180.0);

struct JumpCard {
    id: &'static str,
    glyph: Icon,
    glyph_color: Rgba,
    section: String,
    title: String,
    stat: String,
    stat_label: String,
    hint: String,
    warn: bool,
    target: Screen,
}

struct HomeFlows {
    restore: Entity<RestoreFlow>,
    backup: Entity<BackupFlow>,
    pack_export: Entity<PackExportFlow>,
    pack_import: Entity<PackImportFlow>,
    setup_export: Entity<SetupExportFlow>,
    setup_import: Entity<SetupImportFlow>,
}

struct HomeSubscriptions {
    _transfers: Subscription,
    _stats: Subscription,
    _loss: Subscription,
    _lifecycle: Option<Subscription>,
}

pub(crate) struct HomeView {
    stats: Entity<HomeStats>,
    event_loss: Entity<EventLoss>,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
    transfers: Entity<TransferFlows>,
    flows: HomeFlows,
    export_menu_open: bool,
    row_keys: ListCursor<HomeRow>,
    scroll: ScrollHandle,
    subs: HomeSubscriptions,
}

impl HomeView {
    pub(crate) fn new(
        stats: Entity<HomeStats>,
        event_loss: Entity<EventLoss>,
        pack: PackServices,
        cx: &mut Context<Self>,
    ) -> Self {
        let stats_obs = cx.observe(&stats, |_, _, cx| cx.notify());
        let loss_obs = cx.observe(&event_loss, |_, _, cx| cx.notify());
        let transfers = TransferFlows::route(cx).unwrap_or_else(|| {
            let stats = stats.clone();
            cx.new(|cx| {
                TransferFlows::new(
                    stats,
                    pack,
                    crate::app::app_paths::app_paths().data_dir(),
                    cx,
                )
            })
        });
        let transfer_obs = cx.observe(&transfers, |_, _, cx| cx.notify());
        let source = transfers.read(cx);
        let flows = HomeFlows {
            restore: source.restore.clone(),
            backup: source.backup.clone(),
            pack_export: source.pack_export.clone(),
            pack_import: source.pack_import.clone(),
            setup_export: source.setup_export.clone(),
            setup_import: source.setup_import.clone(),
        };
        Self {
            stats,
            event_loss,
            lifecycle: None,
            transfers,
            flows,
            export_menu_open: false,
            row_keys: ListCursor::new(cx),
            scroll: ScrollHandle::new(),
            subs: HomeSubscriptions {
                _transfers: transfer_obs,
                _stats: stats_obs,
                _loss: loss_obs,
                _lifecycle: None,
            },
        }
    }

    #[must_use]
    pub(crate) fn with_lifecycle(
        mut self,
        lifecycle: Entity<IntegrationLifecycle>,
        cx: &mut Context<Self>,
    ) -> Self {
        self.subs._lifecycle = Some(cx.observe(&lifecycle, |_, _, cx| cx.notify()));
        self.lifecycle = Some(lifecycle);
        self
    }

    fn connections(&self, cx: &App) -> ConnectionsTally {
        let stats = self.stats.read(cx);
        match &self.lifecycle {
            Some(lifecycle) => {
                let lifecycle = lifecycle.read(cx);
                stats.enabled_connections(|integ| lifecycle.is_on(integ))
            }
            None => stats.enabled_connections(|_| true),
        }
    }

    fn go(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

    fn cursor_rows(&self, cx: &App) -> Vec<HomeRow> {
        let mut rows: Vec<HomeRow> = jump_targets().into_iter().map(HomeRow::Jump).collect();
        rows.extend(
            self.connections(cx)
                .connections
                .into_iter()
                .map(|(integration, _)| HomeRow::Connection(integration)),
        );
        rows.extend(
            self.stats
                .read(cx)
                .recent(RECENT_EVENT_ROWS)
                .into_iter()
                .map(|event| HomeRow::Event(event.id)),
        );
        rows
    }

    fn with_row_anchor(
        &self,
        key: &HomeRow,
        row: gpui::Stateful<gpui::Div>,
    ) -> gpui::Stateful<gpui::Div> {
        if self.row_keys.is_at(key) {
            row.child(self.row_keys.row_anchor(Some(&self.scroll)))
        } else {
            row
        }
    }

    fn transfer_busy(&self, cx: &App) -> bool {
        self.transfers.read(cx).is_busy(cx)
    }

    fn request_import(&mut self, cx: &mut Context<Self>) {
        self.transfers
            .update(cx, |transfers, cx| transfers.request_import(cx));
    }

    fn toggle_export_menu(&mut self, cx: &mut Context<Self>) {
        self.export_menu_open = !self.export_menu_open && !self.transfer_busy(cx);
        cx.notify();
    }

    fn close_export_menu(&mut self, cx: &mut Context<Self>) {
        self.export_menu_open = false;
        cx.notify();
    }

    fn export_full_backup(&mut self, cx: &mut Context<Self>) {
        self.export_menu_open = false;
        self.transfers
            .update(cx, |transfers, cx| transfers.export_full_backup(cx));
        cx.notify();
    }

    fn export_pack(&mut self, cx: &mut Context<Self>) {
        self.export_menu_open = false;
        self.transfers
            .update(cx, |transfers, cx| transfers.export_pack(cx));
        cx.notify();
    }

    fn export_setup(&mut self, cx: &mut Context<Self>) {
        self.export_menu_open = false;
        self.transfers
            .update(cx, |transfers, cx| transfers.export_setup(cx));
        cx.notify();
    }
}

impl EventEmitter<NavRequested> for HomeView {}

impl NavigableList for HomeView {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        if self.export_menu_open {
            return;
        }
        let rows = self.cursor_rows(cx);
        if self.row_keys.step(&rows, None, step).is_some() {
            cx.notify();
        }
    }

    fn activate_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.export_menu_open {
            return;
        }
        if let Some(row) = self.row_keys.activation_target(&self.cursor_rows(cx)) {
            self.go(row.target(), cx);
        }
    }
}

impl Render for HomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.row_keys.claim_focus_once(window, cx);
        let palette = cx.palette();
        let density = cx.density();

        let stats = self.stats.read(cx);
        let viewers = stats.viewers_display();
        let glance = GlanceCounts {
            actions: stats.actions_display(),
            commands: stats.commands_display(),
            fired: stats.triggers_fired_display(),
            globals: stats.globals_display(),
        };
        let tally = self.connections(cx);
        let warn = tally.warn();
        let (connected, total) = (tally.connected, tally.total);
        let connections = tally.connections;
        let recent = stats.recent(RECENT_EVENT_ROWS);
        let obs_health = stats.obs_health_snapshot();

        let hero = self.render_hero(&palette, density, cx);
        let cards = jump_card_specs(
            viewers,
            &glance,
            format!("{connected}/{total}"),
            warn,
            &palette,
        );
        let jump_cards = self.render_jump_cards(cards, &palette, density, cx);
        let connections_strip =
            self.render_connections(connections, connected, total, &palette, density, cx);
        let recent_card = self.render_recent_events(recent, &palette, density, cx);
        let glance_card = self.render_glance(glance, &palette, density, cx);
        let loss_card = event_loss_card(self.event_loss.read(cx), &palette, density, cx);

        let bottom = div()
            .w_full()
            .flex()
            .gap(spacing(Spacing::Sm, density))
            .child(grow_cell(recent_card, RECENT_GROW))
            .child(grow_cell(glance_card, GLANCE_GROW));

        let content = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(hero)
            .child(jump_cards)
            .children(
                obs_health.map(|health| self.render_stream_health(health, &palette, density, cx)),
            )
            .child(loss_card)
            .child(connections_strip)
            .child(bottom);

        let focus = self.row_keys.focus().clone();
        let body = navigable_list(div().id("home-scroll"), &focus, cx)
            .track_scroll(&self.scroll)
            .reveals_focus_in(&self.scroll)
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .pt(scaled(BODY_PAD_V, density))
                    .pb(scaled(BODY_PAD_V, density))
                    .px(scaled(BODY_PAD_H, density))
                    .child(content),
            );

        div()
            .relative()
            .size_full()
            .child(page_frame(vec![BreadcrumbCrumb::leaf(tr!("nav_home"))], &palette).body(body))
            .child(self.flows.backup.clone())
            .child(self.flows.pack_export.clone())
            .child(self.flows.pack_import.clone())
            .child(self.flows.setup_export.clone())
            .child(self.flows.setup_import.clone())
            .child(self.flows.restore.clone())
    }
}

fn jump_card_specs(
    viewers: String,
    glance: &GlanceCounts,
    connections_stat: String,
    warn: bool,
    palette: &ForgePalette,
) -> [JumpCard; 3] {
    let [to_chat, to_actions, to_integrations] = jump_targets();
    [
        JumpCard {
            id: "home-jump-chat",
            glyph: Icon::MessageCircle,
            glyph_color: palette.brand,
            section: tr!("home_card_audience_section"),
            title: tr!("home_card_audience_title"),
            stat: viewers,
            stat_label: tr!("home_card_audience_stat_label"),
            hint: tr!("home_card_audience_hint"),
            warn: false,
            target: to_chat,
        },
        JumpCard {
            id: "home-jump-actions",
            glyph: Icon::Bolt,
            glyph_color: palette.warning,
            section: tr!("home_card_automation_section"),
            title: tr!("home_card_automation_title"),
            stat: glance.actions.clone(),
            stat_label: tr!(
                "home_card_automation_stat_label",
                fired = glance.fired.clone()
            ),
            hint: tr!("home_card_automation_hint"),
            warn: false,
            target: to_actions,
        },
        JumpCard {
            id: "home-jump-connections",
            glyph: Icon::Plug,
            glyph_color: palette.success,
            section: tr!("home_card_connections_section"),
            title: tr!("home_card_connections_title"),
            stat: connections_stat,
            stat_label: tr!("home_card_connections_stat_label"),
            hint: tr!("home_card_connections_hint"),
            warn,
            target: to_integrations,
        },
    ]
}

#[cfg(test)]
mod tests {
    use forge_events::{Event, EventSource};
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::test_support::pack_flow::pack_world;
    use crate::test_support::{
        click_outside_the_list, install_presentation, open_under_root, runtime, settle_focus,
    };

    fn home<'a>(
        cx: &'a mut TestAppContext,
        rt: &tokio::runtime::Runtime,
    ) -> (Entity<HomeView>, &'a mut VisualTestContext) {
        install_presentation(cx);
        let world = pack_world(rt);
        let pack = world.with_runtime(rt);
        let (view, vcx) = open_under_root(cx, |cx| {
            let stats = cx.new(|_| HomeStats::new());
            let loss = cx.new(|_| EventLoss::new());
            cx.new(|cx| HomeView::new(stats, loss, pack, cx))
        });
        settle_focus(vcx);
        (view, vcx)
    }

    fn navigation_log(
        vcx: &mut VisualTestContext,
        view: &Entity<HomeView>,
    ) -> Rc<RefCell<Vec<Screen>>> {
        let log = Rc::new(RefCell::new(Vec::new()));
        let heard = Rc::clone(&log);
        vcx.update(|_, cx| {
            cx.subscribe(view, move |_, event: &NavRequested, _| {
                heard.borrow_mut().push(event.0.clone());
            })
            .detach();
        });
        log
    }

    #[gpui::test]
    fn after_a_click_outside_the_arrows_walk_the_cards_and_enter_opens_one(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (view, vcx) = home(cx, &rt);
        let log = navigation_log(vcx, &view);

        click_outside_the_list(vcx);
        vcx.simulate_keystrokes("down down enter");

        assert_eq!(*log.borrow(), [Screen::Actions(None)]);
    }

    #[gpui::test]
    fn the_row_after_the_cards_is_the_first_connection(cx: &mut TestAppContext) {
        let rt = runtime();
        let (view, vcx) = home(cx, &rt);
        let log = navigation_log(vcx, &view);
        let first = view.read_with(vcx, |view, cx| view.cursor_rows(cx)[3].target());

        click_outside_the_list(vcx);
        vcx.simulate_keystrokes("down down down down enter");

        assert_eq!(*log.borrow(), [first]);
    }

    #[gpui::test]
    fn a_recent_event_row_opens_the_event_feed(cx: &mut TestAppContext) {
        let rt = runtime();
        let (view, vcx) = home(cx, &rt);
        let log = navigation_log(vcx, &view);
        let stats = view.read_with(vcx, |view, _| view.stats.clone());
        stats.update(vcx, |stats, cx| {
            let event = Event::new(
                EventSource::Twitch,
                "twitch.channel.follow",
                serde_json::Value::Null,
            );
            stats.record_event(&event, None);
            cx.notify();
        });
        settle_focus(vcx);

        click_outside_the_list(vcx);
        vcx.simulate_keystrokes("up enter");

        assert_eq!(*log.borrow(), [Screen::EventFeed]);
    }
}
