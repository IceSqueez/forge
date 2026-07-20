use std::sync::{Arc, RwLock};

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, FONT_XS, ForgePalette, breadcrumb,
    status_dot, tr, with_alpha,
};
use forge_speak_queue::{PipelineConfigHandle, SpeakQueueHandle};
use forge_storage::{CredentialsRepo, DataProvider, SettingsRepo};
use forge_tts_core::TtsRegistry;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, Window, div, prelude::*, px,
};

use crate::presentation::ActivePresentation;
use crate::speak_state::SpeakState;
use crate::tts_dashboard::TtsDashboardView;
use crate::tts_engines::TtsEnginesView;
use crate::tts_filters::TtsFiltersView;
use crate::voice_aliases::VoiceAliasesView;

const TAB_PAD_V: Pixels = px(7.0);
const TAB_PAD_H: Pixels = px(14.0);
const TAB_INDICATOR_H: Pixels = px(2.0);
const TAB_GAP: Pixels = px(2.0);
const TAB_BAR_PAD_T: Pixels = px(8.0);
const TAB_BAR_PAD_H: Pixels = px(14.0);
const ENGINES_READY_DOT: Pixels = px(7.0);

pub(crate) fn name_accent(name: &str, palette: &ForgePalette) -> Rgba {
    let accents = [
        palette.brand,
        palette.info,
        palette.success,
        palette.warning,
        palette.bits,
        palette.accent_teal,
        palette.accent_pink_light,
    ];
    let hash = name
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    accents[hash % accents.len()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsSection {
    Dashboard,
    Engines,
    Aliases,
    Filters,
}

impl TtsSection {
    const ALL: [TtsSection; 4] = [
        TtsSection::Dashboard,
        TtsSection::Engines,
        TtsSection::Aliases,
        TtsSection::Filters,
    ];

    fn label(self) -> String {
        match self {
            TtsSection::Dashboard => tr!("tts_tab_dashboard"),
            TtsSection::Engines => tr!("tts_tab_engines"),
            TtsSection::Aliases => tr!("tts_tab_aliases"),
            TtsSection::Filters => tr!("tts_tab_filters"),
        }
    }

    fn key(self) -> &'static str {
        match self {
            TtsSection::Dashboard => "dashboard",
            TtsSection::Engines => "engines",
            TtsSection::Aliases => "aliases",
            TtsSection::Filters => "filters",
        }
    }
}

pub struct TtsView {
    section: TtsSection,
    dashboard: Entity<TtsDashboardView>,
    engines: Entity<TtsEnginesView>,
    aliases: Entity<VoiceAliasesView>,
    filters: Entity<TtsFiltersView>,
    tts_registry: Option<Arc<RwLock<TtsRegistry>>>,
    speak: Option<SpeakQueueHandle>,
}

impl TtsView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        speak_state: Entity<SpeakState>,
        speak: Option<SpeakQueueHandle>,
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        pipeline_config: Option<PipelineConfigHandle>,
        tts_registry: Option<Arc<RwLock<TtsRegistry>>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let credentials: Arc<dyn CredentialsRepo> =
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>;
        let settings: Arc<dyn SettingsRepo> = Arc::clone(&backend) as Arc<dyn SettingsRepo>;
        let dashboard = cx.new(|cx| {
            TtsDashboardView::new(
                speak_state,
                speak.clone(),
                Arc::clone(&settings),
                tts_registry.clone(),
                rt_handle.clone(),
                cx,
            )
        });
        let engines = cx.new(|cx| {
            TtsEnginesView::new(
                tts_registry.clone(),
                Arc::clone(&credentials),
                settings,
                speak.clone(),
                rt_handle.clone(),
                cx,
            )
        });
        let filters = cx.new(|cx| {
            TtsFiltersView::new(
                backend.tts_filters_repo(),
                pipeline_config,
                speak.clone(),
                rt_handle.clone(),
                cx,
            )
        });
        let aliases = cx.new(|cx| {
            VoiceAliasesView::new(
                backend.voice_alias_repo(),
                backend.viewer_repo(),
                speak.clone(),
                rt_handle.clone(),
                cx,
            )
        });
        Self {
            section: TtsSection::Dashboard,
            dashboard,
            engines,
            aliases,
            filters,
            tts_registry,
            speak,
        }
    }

    fn select_section(&mut self, section: TtsSection, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn render_header(&self, palette: &ForgePalette) -> impl IntoElement + use<> {
        let registered = self
            .tts_registry
            .as_ref()
            .map(|r| r.read().unwrap_or_else(|e| e.into_inner()).engine_ids())
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
        let chip = div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .text_size(FONT_XS)
            .text_color(palette.success)
            .child(status_dot(palette.success, ENGINES_READY_DOT))
            .child(tr!("tts_header_engines_ready", count = engine_count as i64));
        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf(tr!("tts_breadcrumb_builtin")),
                BreadcrumbCrumb::leaf(tr!("tts_breadcrumb_tts")),
                BreadcrumbCrumb::leaf(self.section.label()),
            ],
            palette,
        )
        .right(chip)
    }

    fn render_tab_bar(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut bar = div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .gap(TAB_GAP)
            .pt(TAB_BAR_PAD_T)
            .px(TAB_BAR_PAD_H)
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
        let active = self.section == section;
        let fg = if active {
            palette.text_primary
        } else {
            palette.text_muted
        };
        // Always laid out to hold the row height; transparent on inactive tabs.
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
            .py(TAB_PAD_V)
            .px(TAB_PAD_H)
            .border_b(TAB_INDICATOR_H)
            .border_color(indicator)
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_section(section, cx)),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
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

impl Render for TtsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let header = self.render_header(&palette);
        let tab_bar = self.render_tab_bar(&palette, cx);
        let content = self.render_content();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(tab_bar)
            .child(div().w_full().flex_1().min_h(px(0.0)).child(content))
    }
}
