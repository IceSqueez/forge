use std::sync::{Arc, RwLock};

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, Density, FONT_SM, FONT_XXS, ForgePalette,
    Spacing, StatusVariant, badge, breadcrumb, spacing, with_alpha,
};
use forge_speak_queue::{PipelineConfigHandle, SpeakQueueHandle};
use forge_storage::{CredentialsRepo, DataProvider};
use forge_tts_core::TtsRegistry;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Window, div, prelude::*, px,
};

use crate::cloud_tts_engines::CloudTtsEnginesView;
use crate::presentation::ActivePresentation;
use crate::speak_state::SpeakState;
use crate::tts_dashboard::TtsDashboardView;
use crate::tts_engines::TtsEnginesView;
use crate::tts_filters::TtsFiltersView;
use crate::tts_triggers::TtsTriggersView;
use crate::voice_aliases::VoiceAliasesView;

/// Tab-button vertical padding — the parity source pins the tab hit-target at a
/// fixed 7px inset, off the `Spacing` scale, so it is carried as a named literal.
const TAB_PAD_V: Pixels = px(7.0);
/// Tab-button horizontal padding (the source's fixed 14px inset).
const TAB_PAD_H: Pixels = px(14.0);
/// Height of the active-tab underline indicator (the source's fixed 2px rule).
const TAB_INDICATOR_H: Pixels = px(2.0);
/// Number of engines the dashboard seeds; surfaced in the header's ready chip until
/// a live TTS-engine roster reaches this screen over the runtime→UI bridge.
const SEEDED_ENGINE_COUNT: usize = 3;

/// The six horizontal tabs of the Text-to-Speech screen. The active tab lives as a
/// field on [`TtsView`] (not in the top-level router), so navigating to TTS always
/// opens at [`TtsSection::Dashboard`] and a tab click swaps the pane in place —
/// mirroring the screen mockup's internal `useState('dashboard')` tab state and the
/// Settings screen's section-nav precedent, and preserving the dashboard child
/// entity's state across tab switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsSection {
    Dashboard,
    Engines,
    Aliases,
    Filters,
    Triggers,
    CloudEngines,
}

impl TtsSection {
    /// The six tabs in bar order.
    const ALL: [TtsSection; 6] = [
        TtsSection::Dashboard,
        TtsSection::Engines,
        TtsSection::Aliases,
        TtsSection::Filters,
        TtsSection::Triggers,
        TtsSection::CloudEngines,
    ];

    /// Tab label + breadcrumb leaf.
    fn label(self) -> &'static str {
        match self {
            TtsSection::Dashboard => "Dashboard",
            TtsSection::Engines => "Engines",
            TtsSection::Aliases => "Voice aliases",
            TtsSection::Filters => "Filters",
            TtsSection::Triggers => "Triggers",
            TtsSection::CloudEngines => "Cloud engines",
        }
    }

    /// Stable element-id fragment for the tab button.
    fn key(self) -> &'static str {
        match self {
            TtsSection::Dashboard => "dashboard",
            TtsSection::Engines => "engines",
            TtsSection::Aliases => "aliases",
            TtsSection::Filters => "filters",
            TtsSection::Triggers => "triggers",
            TtsSection::CloudEngines => "cloud-engines",
        }
    }
}

/// The Text-to-Speech screen view-entity: a breadcrumb header with an
/// engines-ready chip, a horizontal tab bar over the six [`TtsSection`]s, and the
/// active section's pane. Owns the active section plus the Dashboard and Engines
/// child view-entities; the other four sections are deferred placeholders until
/// their slices land. Holds no domain state — each child caches its own seeded stub
/// state and drives the real runtime through a handle once wired.
pub struct TtsView {
    section: TtsSection,
    dashboard: Entity<TtsDashboardView>,
    engines: Entity<TtsEnginesView>,
    aliases: Entity<VoiceAliasesView>,
    filters: Entity<TtsFiltersView>,
    triggers: Entity<TtsTriggersView>,
    cloud: Entity<CloudTtsEnginesView>,
}

impl TtsView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        speak_state: Entity<SpeakState>,
        speak: Option<SpeakQueueHandle>,
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        pipeline_config: Option<PipelineConfigHandle>,
        tts_trigger_settings: forge_runtime::TtsTriggerSettingsHandle,
        tts_registry: Option<Arc<RwLock<TtsRegistry>>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let dashboard =
            cx.new(|cx| TtsDashboardView::new(speak_state, speak.clone(), rt_handle.clone(), cx));
        let engines = cx.new(TtsEnginesView::new);
        let filters = cx.new(|cx| {
            TtsFiltersView::new(
                backend.tts_filters_repo(),
                pipeline_config,
                speak.clone(),
                rt_handle.clone(),
                cx,
            )
        });
        let triggers = cx.new(|cx| {
            TtsTriggersView::new(
                backend.tts_trigger_settings_repo(),
                tts_trigger_settings,
                rt_handle.clone(),
                cx,
            )
        });
        let aliases = cx.new(|cx| {
            VoiceAliasesView::new(
                backend.voice_alias_repo(),
                speak.clone(),
                rt_handle.clone(),
                cx,
            )
        });
        let credentials: Arc<dyn CredentialsRepo> =
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>;
        let cloud =
            cx.new(|cx| CloudTtsEnginesView::new(tts_registry, credentials, rt_handle, speak, cx));
        Self {
            section: TtsSection::Dashboard,
            dashboard,
            engines,
            aliases,
            filters,
            triggers,
            cloud,
        }
    }

    fn select_section(&mut self, section: TtsSection, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn render_header(&self, palette: &ForgePalette) -> impl IntoElement + use<> {
        let (chip_bg, chip_fg) = StatusVariant::Positive.colors(palette);
        let chip = badge(
            chip_bg,
            chip_fg,
            format!("{SEEDED_ENGINE_COUNT} engines ready"),
            false,
            FONT_XXS,
        );
        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf("Builtin"),
                BreadcrumbCrumb::leaf("TTS"),
                BreadcrumbCrumb::leaf(self.section.label()),
            ],
            palette,
        )
        .right(chip)
    }

    fn render_tab_bar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut bar = div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Xxs, density))
            .bg(palette.shell)
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
        // The underline is always laid out (fixed row height); it inks the brand
        // accent on the active tab and stays fully transparent otherwise.
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
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(TAB_PAD_V)
            .px(TAB_PAD_H)
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_section(section, cx)),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(weight)
                    .text_size(FONT_SM)
                    .text_color(fg)
                    .child(section.label()),
            )
            .child(div().w_full().h(TAB_INDICATOR_H).bg(indicator))
    }

    /// The active section's pane — each section renders its own child view-entity.
    fn render_content(&self) -> AnyElement {
        match self.section {
            TtsSection::Dashboard => self.dashboard.clone().into_any_element(),
            TtsSection::Engines => self.engines.clone().into_any_element(),
            TtsSection::Aliases => self.aliases.clone().into_any_element(),
            TtsSection::Filters => self.filters.clone().into_any_element(),
            TtsSection::Triggers => self.triggers.clone().into_any_element(),
            TtsSection::CloudEngines => self.cloud.clone().into_any_element(),
        }
    }
}

impl Render for TtsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header = self.render_header(&palette);
        let tab_bar = self.render_tab_bar(&palette, density, cx);
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
