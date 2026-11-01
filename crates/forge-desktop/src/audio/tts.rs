use gpui::Rgba;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, BORDER_THIN, BreadcrumbCrumb, Density, FONT_XS, ForgePalette, KeyActivated,
    header_status, page_frame, scaled, tr, with_alpha,
};
use forge_speak_queue::{PipelineConfigHandle, SpeakQueueHandle};
use forge_storage::{CredentialsRepo, DataProvider, SettingsRepo};
use forge_tts_cloud::CloudGateways;
use forge_types::Shared;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Pixels, Subscription,
    Window, div, prelude::*, px,
};

use crate::app::navigation::{NavRequested, Screen, TtsSection};
use crate::audio::speak_state::SpeakState;
use crate::audio::tts_dashboard::TtsDashboardView;
use crate::audio::tts_engines::TtsEnginesView;
use crate::audio::tts_filters::TtsFiltersView;
use crate::audio::voice_aliases::VoiceAliasesView;
use crate::integrations::hub_crumb::{core_category, hub_crumb};

const TAB_PAD_V: Pixels = px(7.0);
const TAB_PAD_H: Pixels = px(14.0);
const TAB_INDICATOR_H: Pixels = px(2.0);
const TAB_GAP: Pixels = px(2.0);
const TAB_BAR_PAD_T: Pixels = px(8.0);
const TAB_BAR_PAD_H: Pixels = px(14.0);

impl TtsSection {
    fn label(self) -> String {
        match self {
            TtsSection::Dashboard => tr!("tts_tab_dashboard"),
            TtsSection::Engines => tr!("tts_tab_engines"),
            TtsSection::Aliases => tr!("tts_tab_aliases"),
            TtsSection::Filters => tr!("tts_tab_filters"),
        }
    }
}

pub(crate) struct TtsView {
    section: TtsSection,
    dashboard: Entity<TtsDashboardView>,
    engines: Entity<TtsEnginesView>,
    aliases: Entity<VoiceAliasesView>,
    filters: Entity<TtsFiltersView>,
    speak: Option<SpeakQueueHandle>,
    voice_catalog_revision: u64,
    _speak_state_obs: Subscription,
}

impl TtsView {
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor receives each shared handle it wires"
    )]
    pub(crate) fn new(
        speak_state: Entity<SpeakState>,
        speak: Option<SpeakQueueHandle>,
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        pipeline_config: Option<PipelineConfigHandle>,
        bot_accounts: Shared<Vec<String>>,
        cloud_gateways: CloudGateways,
        preselect: Option<TtsSection>,
        cx: &mut Context<Self>,
    ) -> Self {
        let voice_catalog_revision = speak_state.read(cx).voice_catalog_revision();
        let speak_state_obs = cx.observe(&speak_state, |this, state, cx| {
            let revision = state.read(cx).voice_catalog_revision();
            if revision != this.voice_catalog_revision {
                this.voice_catalog_revision = revision;
                this.voice_catalog_changed(cx);
            }
        });
        let credentials: Arc<dyn CredentialsRepo> = backend.credentials_repo();
        let settings: Arc<dyn SettingsRepo> = backend.settings_repo();
        let dashboard = cx.new(|cx| {
            TtsDashboardView::new(
                speak_state,
                speak.clone(),
                Arc::clone(&settings),
                rt_handle.clone(),
                cx,
            )
        });
        let engines = cx.new(|cx| {
            TtsEnginesView::new(
                cloud_gateways,
                Arc::clone(&credentials),
                Arc::clone(&settings),
                speak.clone(),
                rt_handle.clone(),
                cx,
            )
        });
        let filters = cx.new(|cx| {
            TtsFiltersView::new(
                backend.tts_filters_repo(),
                pipeline_config,
                bot_accounts,
                speak.clone(),
                rt_handle.clone(),
                cx,
            )
        });
        let aliases = cx.new(|cx| {
            VoiceAliasesView::new(
                backend.voice_alias_repo(),
                backend.viewer_repo(),
                settings,
                speak.clone(),
                rt_handle.clone(),
                cx,
            )
        });
        Self {
            section: preselect.unwrap_or(TtsSection::Dashboard),
            dashboard,
            engines,
            aliases,
            filters,
            speak,
            voice_catalog_revision,
            _speak_state_obs: speak_state_obs,
        }
    }

    fn voice_catalog_changed(&mut self, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.refresh_engine_roster(cx));
        self.engines
            .update(cx, |engines, cx| engines.refresh_engine_roster(cx));
        self.aliases.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    fn select_section(&mut self, section: TtsSection, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn render_header_right(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<> {
        let registered = self
            .speak
            .as_ref()
            .map(|h| h.registered_engine_ids())
            .unwrap_or_default();
        let voices = self
            .speak
            .as_ref()
            .map(|h| h.available_voices())
            .unwrap_or_default();
        let engine_count = registered
            .iter()
            .filter(|id| voices.iter().any(|v| v.engine_id == **id))
            .count();
        let (color, label) = engine_readiness(engine_count, palette);
        header_status(color, label, density)
    }

    fn render_tab_bar(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        let mut bar = div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .gap(scaled(TAB_GAP, density))
            .pt(scaled(TAB_BAR_PAD_T, density))
            .px(scaled(TAB_BAR_PAD_H, density))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);
        for section in TtsSection::ALL {
            bar = bar.child(self.tab_button(section, palette, cx));
        }
        bar
    }

    fn tab_button(
        &self,
        section: TtsSection,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        let active = self.section == section;
        let fg = if active {
            palette.text_primary
        } else {
            palette.text_muted
        };
        let indicator = if active {
            palette.brand
        } else {
            with_alpha(palette.brand, 0.0)
        };
        let weight = if active {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        };

        div()
            .id(section.key())
            .py(scaled(TAB_PAD_V, density))
            .px(scaled(TAB_PAD_H, density))
            .border_b(TAB_INDICATOR_H)
            .border_color(indicator)
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_section(section, cx)),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(weight)
                    .text_size(FONT_XS)
                    .text_color(fg)
                    .child(section.label()),
            )
    }

    fn render_content(&self) -> AnyElement {
        match self.section {
            TtsSection::Dashboard => self.dashboard.clone().into_any_element(),
            TtsSection::Engines => self.engines.clone().into_any_element(),
            TtsSection::Aliases => self.aliases.clone().into_any_element(),
            TtsSection::Filters => self.filters.clone().into_any_element(),
        }
    }
}

impl EventEmitter<NavRequested> for TtsView {}

impl Render for TtsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();

        let header_right = self.render_header_right(&palette, density);
        let tab_bar = self.render_tab_bar(&palette, cx);
        let content = self.render_content();
        let body = div().w_full().flex_1().min_h(px(0.0)).child(content);

        page_frame(
            vec![
                hub_crumb(core_category(&Screen::Tts(None)), cx),
                BreadcrumbCrumb::leaf(tr!("tts_breadcrumb_tts")),
                BreadcrumbCrumb::leaf(self.section.label()),
            ],
            &palette,
        )
        .header_right(header_right)
        .section_switcher(tab_bar)
        .body(body)
    }
}

fn engine_readiness(engine_count: usize, palette: &ForgePalette) -> (Rgba, String) {
    if engine_count == 0 {
        (palette.warning, tr!("tts_header_no_engine_ready"))
    } else {
        (
            palette.success,
            tr!(
                "tts_header_engines_ready",
                count = i64::try_from(engine_count).unwrap_or(i64::MAX)
            ),
        )
    }
}
