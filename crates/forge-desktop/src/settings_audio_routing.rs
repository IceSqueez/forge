use std::sync::Arc;

use forge_audio::{AudioRoute, AudioSink};
use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing,
    anchored_popover_below, body_family, drive_overlay_focus, field_hint, icon, mono_family,
    radius, segment, segmented, setting_row, spacing, tr, with_alpha,
};
use forge_overlay::kinds::audio::KIND_ID as AUDIO_OVERLAY_KIND;
use forge_runtime::OverlayServiceHandle;
use forge_storage::{OverlayId, OverlayRepo, SettingsRepo};
use gpui::{
    AnyElement, ClickEvent, Context, FocusHandle, Pixels, SharedString, Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::audio_routes::{
    AudioDomain, RouteFallback, RoutePlan, load_audio_routes, plan_route, resolve_destination,
    set_destination, set_route,
};
use crate::presentation::ActivePresentation;
use crate::settings_audio::test_tone;

const PANEL_WIDTH: Pixels = px(360.0);

const TRIGGER_HEIGHT: Pixels = px(34.0);

const EVERY_ROUTE: [(AudioRoute, &str); 3] = [
    (AudioRoute::Local, "settings_audio_route_local"),
    (AudioRoute::Overlay, "settings_audio_route_overlay"),
    (AudioRoute::Both, "settings_audio_route_both"),
];

const SINGLE_PAGE: usize = 1;

struct OverlayChoice {
    id: OverlayId,
    name: String,
}

struct Loaded {
    speech: AudioRoute,
    clips: AudioRoute,
    destination: Option<OverlayId>,
    choices: Vec<OverlayChoice>,
    speech_plan: RoutePlan,
    clips_plan: RoutePlan,
}

/// Resolved once at mount from the stored values, so it keeps reporting what this process boots with while the selectors move.
struct InEffect {
    speech: RoutePlan,
    clips: RoutePlan,
}

pub struct SettingsAudioRoutingView {
    settings: Arc<dyn SettingsRepo>,
    overlays: Arc<dyn OverlayRepo>,
    overlay_service: OverlayServiceHandle,
    rt_handle: tokio::runtime::Handle,
    speech_sink: Arc<dyn AudioSink>,
    server_available: bool,
    loading: bool,
    load_error: Option<String>,
    speech: AudioRoute,
    clips: AudioRoute,
    destination: Option<OverlayId>,
    choices: Vec<OverlayChoice>,
    connected_pages: Option<usize>,
    pages_gen: async_bridge::Generation,
    in_effect: Option<InEffect>,
    picker_open: bool,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
    persist_error: Option<String>,
    testing: bool,
    test_error: Option<String>,
}

impl SettingsAudioRoutingView {
    pub fn new(
        settings: Arc<dyn SettingsRepo>,
        overlays: Arc<dyn OverlayRepo>,
        overlay_service: OverlayServiceHandle,
        rt_handle: tokio::runtime::Handle,
        speech_sink: Arc<dyn AudioSink>,
        server_available: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            settings,
            overlays,
            overlay_service,
            rt_handle,
            speech_sink,
            server_available,
            loading: true,
            load_error: None,
            speech: AudioRoute::default(),
            clips: AudioRoute::default(),
            destination: None,
            choices: Vec::new(),
            connected_pages: None,
            pages_gen: async_bridge::Generation::default(),
            in_effect: None,
            picker_open: false,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
            persist_error: None,
            testing: false,
            test_error: None,
        };
        view.load(cx);
        view
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        async_bridge::run_async(
            &self.rt_handle,
            load_routing(
                Arc::clone(&self.settings),
                Arc::clone(&self.overlays),
                self.server_available,
            ),
            |this, result, cx| this.apply_loaded(result, cx),
            cx,
        );
    }

    fn apply_loaded(&mut self, result: Result<Loaded, String>, cx: &mut Context<Self>) {
        self.loading = false;
        match result {
            Ok(loaded) => {
                self.speech = loaded.speech;
                self.clips = loaded.clips;
                self.destination = loaded.destination;
                self.choices = loaded.choices;
                self.in_effect = Some(InEffect {
                    speech: loaded.speech_plan,
                    clips: loaded.clips_plan,
                });
                self.count_pages(cx);
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to read the audio routing settings");
                self.load_error = Some(message);
            }
        }
        cx.notify();
    }

    fn choose_route(&mut self, domain: AudioDomain, route: AudioRoute, cx: &mut Context<Self>) {
        match domain {
            AudioDomain::Speech => self.speech = route,
            AudioDomain::Clips => self.clips = route,
        }
        self.persist_error = None;
        let settings = Arc::clone(&self.settings);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                set_route(settings.as_ref(), domain, route)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| this.apply_persisted(result, cx),
            cx,
        );
        cx.notify();
    }

    fn choose_destination(&mut self, destination: Option<OverlayId>, cx: &mut Context<Self>) {
        self.destination = destination.clone();
        self.picker_open = false;
        self.persist_error = None;
        self.connected_pages = None;
        self.count_pages(cx);
        let settings = Arc::clone(&self.settings);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                set_destination(settings.as_ref(), destination.as_ref())
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result, cx| this.apply_persisted(result, cx),
            cx,
        );
        cx.notify();
    }

    /// Read at mount, when the destination changes and when a test tone settles; never on a timer.
    fn count_pages(&mut self, cx: &mut Context<Self>) {
        let ticket = self.pages_gen.next();
        let Some(id) = self.destination.clone() else {
            self.connected_pages = None;
            cx.notify();
            return;
        };
        let service = self.overlay_service.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { service.receivers(&id).await.sources },
            move |this, sources, cx| {
                if this.pages_gen.is_current(ticket) {
                    this.connected_pages = Some(sources);
                    cx.notify();
                }
            },
            cx,
        );
    }

    fn apply_persisted(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        if let Err(message) = result {
            tracing::warn!(error = %message, "failed to persist an audio routing setting");
            self.persist_error = Some(message);
            cx.notify();
        }
    }

    fn test_on_overlay(&mut self, cx: &mut Context<Self>) {
        if self.testing {
            return;
        }
        self.testing = true;
        self.test_error = None;
        let sink = Arc::clone(&self.speech_sink);
        async_bridge::run_async(
            &self.rt_handle,
            async move { sink.play(test_tone()).await.map_err(|e| e.to_string()) },
            |this, result, cx| this.apply_tested(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_tested(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.testing = false;
        if let Err(message) = result {
            tracing::warn!(error = %message, "the audio overlay test tone did not play");
            self.test_error = Some(message);
        }
        self.count_pages(cx);
        cx.notify();
    }

    fn toggle_picker(&mut self, cx: &mut Context<Self>) {
        self.picker_open = !self.picker_open;
        cx.notify();
    }

    fn close_picker(&mut self, cx: &mut Context<Self>) {
        if self.picker_open {
            self.picker_open = false;
            cx.notify();
        }
    }

    fn destination_label(&self) -> SharedString {
        let Some(id) = &self.destination else {
            return tr!("settings_audio_routing_destination_none").into();
        };
        self.choices
            .iter()
            .find(|choice| &choice.id == id)
            .map(|choice| SharedString::from(choice.name.clone()))
            .unwrap_or_else(|| SharedString::from(id.as_str().to_owned()))
    }

    fn card_header(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        let tile = div()
            .size(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Lg))
            .bg(with_alpha(palette.accent_teal, 0.12))
            .child(icon(Icon::Broadcast, px(16.0), palette.accent_teal));
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(tile)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("settings_audio_routing_title")),
            )
    }

    fn section_label(&self, key: &'static str, palette: &ForgePalette) -> impl IntoElement {
        div()
            .py(spacing(Spacing::Xs, Density::Cozy))
            .px(spacing(Spacing::Md, Density::Cozy))
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(tr!(key))
    }

    fn route_control(
        &self,
        domain: AudioDomain,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let current = match domain {
            AudioDomain::Speech => self.speech,
            AudioDomain::Clips => self.clips,
        };
        let group = match domain {
            AudioDomain::Speech => "settings-audio-route-speech",
            AudioDomain::Clips => "settings-audio-route-clips",
        };
        let segments = EVERY_ROUTE
            .into_iter()
            .enumerate()
            .map(|(idx, (route, key))| {
                segment(
                    (group, idx),
                    tr!(key),
                    current == route,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.choose_route(domain, route, cx)
                    }),
                )
            })
            .collect();
        segmented(segments, palette)
    }

    fn destination_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(text) = no_overlays_text(&self.choices) {
            return div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(text)
                .into_any_element();
        }

        let trigger = div()
            .id("settings-audio-destination-trigger")
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(px(7.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.base)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_picker(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(self.destination_label()),
            )
            .child(icon(Icon::ChevronDown, FONT_XS, palette.text_faint));

        let mut field = div().relative().flex_1().min_w(px(0.0)).child(trigger);
        if self.picker_open {
            field = field.child(self.picker_overlay(palette, cx));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(spacing(Spacing::Sm, density))
                    .child(field)
                    .child(self.test_button(palette, cx)),
            )
            .children(self.pages_section(palette, density))
            .into_any_element()
    }

    fn pages_section(&self, palette: &ForgePalette, density: Density) -> Option<impl IntoElement> {
        let connected = self.connected_pages?;
        let mut column = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(connected_pages_text(connected)),
            );
        if let Some(warning) = duplicate_pages_text(connected) {
            column = column.child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::AlertTriangle, FONT_XS, palette.warning))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.warning)
                            .child(warning),
                    ),
            );
        }
        Some(column)
    }

    fn test_button(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if self.testing {
            tr!("settings_audio_routing_testing")
        } else {
            tr!("settings_audio_routing_test")
        };
        let mut button = div()
            .id("settings-audio-routing-test")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.0))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::PlayerPlay, px(11.0), palette.text_secondary))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child(label),
            );
        if !self.testing {
            let hover = with_alpha(palette.border_regular, 0.08);
            button = button
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.test_on_overlay(cx)));
        }
        button
    }

    fn picker_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .flex()
            .flex_col()
            .w(PANEL_WIDTH)
            .py(spacing(Spacing::Xs, Density::Cozy))
            .bg(palette.elevated)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .occlude();

        panel = panel.child(self.picker_entry(
            0,
            tr!("settings_audio_routing_destination_none").into(),
            self.destination.is_none(),
            None,
            palette,
            cx,
        ));
        for (idx, choice) in self.choices.iter().enumerate() {
            let selected = self.destination.as_ref() == Some(&choice.id);
            panel = panel.child(self.picker_entry(
                idx + 1,
                choice.name.clone().into(),
                selected,
                Some(choice.id.clone()),
                palette,
                cx,
            ));
        }

        let view = cx.entity();
        anchored_popover_below(TRIGGER_HEIGHT, panel)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_picker(cx));
            })
            .into_any_element()
    }

    fn picker_entry(
        &self,
        idx: usize,
        label: SharedString,
        selected: bool,
        target: Option<OverlayId>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut item = div()
            .id(("settings-audio-destination", idx))
            .flex()
            .items_center()
            .w_full()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(spacing(Spacing::Xs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(|style| style.bg(palette.surface_overlay))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.choose_destination(target.clone(), cx)
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(label),
            );
        if selected {
            item = item.child(icon(Icon::CircleCheck, FONT_SM, palette.brand));
        }
        item
    }

    fn restart_note(&self, palette: &ForgePalette, density: Density) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::InfoCircle, FONT_XS, palette.text_faint))
            .child(field_hint(
                tr!("settings_audio_routing_restart_note"),
                palette,
            ))
    }

    fn in_effect_section(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> Option<impl IntoElement> {
        let in_effect = self.in_effect.as_ref()?;
        Some(
            div()
                .flex()
                .flex_col()
                .child(self.section_label("settings_audio_routing_in_effect", palette))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(spacing(Spacing::Xxs, density))
                        .child(self.in_effect_row(
                            tr!("settings_audio_routing_speech_label"),
                            &in_effect.speech,
                            palette,
                            density,
                        ))
                        .child(self.in_effect_row(
                            tr!("settings_audio_routing_clips_label"),
                            &in_effect.clips,
                            palette,
                            density,
                        )),
                ),
        )
    }

    fn in_effect_row(
        &self,
        label: String,
        plan: &RoutePlan,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement {
        let value = in_effect_text(plan);
        let tone = match plan.fallback {
            Some(_) => palette.warning,
            None => palette.text_secondary,
        };
        div()
            .flex()
            .flex_row()
            .items_baseline()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(tone)
                    .child(value),
            )
    }

    fn error_text(
        &self,
        key: &'static str,
        message: &str,
        palette: &ForgePalette,
    ) -> impl IntoElement {
        div()
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.random)
            .child(tr!(key, error = message))
    }
}

impl Render for SettingsAudioRoutingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.picker_open,
            &self.overlay_focus,
            &mut self.focus_restore,
            window,
            cx,
        );

        let palette = cx.palette();
        let density = cx.density();

        let mut content = div().flex().flex_col().gap(spacing(Spacing::Md, density));

        if self.loading {
            content = content.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_audio_routing_loading")),
            );
        } else if let Some(message) = &self.load_error {
            content = content.child(self.error_text(
                "settings_audio_routing_load_error",
                message,
                &palette,
            ));
        } else {
            content = content
                .child(setting_row(
                    tr!("settings_audio_routing_speech_label"),
                    Some(tr!("settings_audio_routing_speech_hint").into()),
                    self.route_control(AudioDomain::Speech, &palette, cx),
                    &palette,
                    density,
                ))
                .child(setting_row(
                    tr!("settings_audio_routing_clips_label"),
                    Some(tr!("settings_audio_routing_clips_hint").into()),
                    self.route_control(AudioDomain::Clips, &palette, cx),
                    &palette,
                    density,
                ))
                .child(self.section_label("settings_audio_routing_destination", &palette))
                .child(self.destination_section(&palette, density, cx))
                .child(self.restart_note(&palette, density))
                .children(self.in_effect_section(&palette, density));

            if let Some(message) = &self.test_error {
                content = content.child(self.error_text(
                    "settings_audio_routing_test_error",
                    message,
                    &palette,
                ));
            }
            if let Some(message) = &self.persist_error {
                content = content.child(self.error_text(
                    "settings_audio_routing_persist_error",
                    message,
                    &palette,
                ));
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .p(spacing(Spacing::Md, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.elevated)
            .child(self.card_header(&palette, density))
            .child(content)
    }
}

fn in_effect_text(plan: &RoutePlan) -> String {
    match plan.fallback {
        Some(fallback) => tr!(
            "settings_audio_routing_in_effect_fallback",
            route = route_label(plan.route),
            reason = fallback_label(fallback)
        ),
        None => route_label(plan.route),
    }
}

fn no_overlays_text(choices: &[OverlayChoice]) -> Option<String> {
    choices
        .is_empty()
        .then(|| tr!("settings_audio_routing_no_overlays"))
}

fn connected_pages_text(connected: usize) -> String {
    if connected == 0 {
        return tr!("settings_audio_routing_pages_none");
    }
    tr!(
        "settings_audio_routing_pages_connected",
        count = connected as i64
    )
}

fn duplicate_pages_text(connected: usize) -> Option<String> {
    (connected > SINGLE_PAGE).then(|| {
        tr!(
            "settings_audio_routing_pages_duplicate",
            count = connected as i64
        )
    })
}

fn fallback_label(fallback: RouteFallback) -> String {
    tr!(match fallback {
        RouteFallback::NoDestinationChosen => "settings_audio_routing_fallback_unchosen",
        RouteFallback::ServerUnavailable => "settings_audio_routing_fallback_server_off",
        RouteFallback::DestinationUnreadable => "settings_audio_routing_fallback_unreadable",
        RouteFallback::DestinationNotFound => "settings_audio_routing_fallback_missing",
        RouteFallback::DestinationNotAudioOverlay => "settings_audio_routing_fallback_wrong_kind",
    })
}

fn route_label(route: AudioRoute) -> String {
    let key = EVERY_ROUTE
        .into_iter()
        .find(|(candidate, _)| *candidate == route)
        .map(|(_, key)| key);
    match key {
        Some(key) => tr!(key),
        None => route.as_str().to_owned(),
    }
}

async fn load_routing(
    settings: Arc<dyn SettingsRepo>,
    overlays: Arc<dyn OverlayRepo>,
    server_available: bool,
) -> Result<Loaded, String> {
    let routes = load_audio_routes(settings.as_ref()).await;
    let choices = overlays
        .list()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|definition| definition.kind_id == AUDIO_OVERLAY_KIND)
        .map(|definition| OverlayChoice {
            id: definition.id,
            name: definition.display_name,
        })
        .collect();
    let destination = resolve_destination(
        overlays.as_ref(),
        server_available,
        routes.destination.as_ref(),
    )
    .await;
    Ok(Loaded {
        speech_plan: plan_route(routes.speech, &destination),
        clips_plan: plan_route(routes.clips, &destination),
        speech: routes.speech,
        clips: routes.clips,
        destination: routes.destination,
        choices,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeSet;

    use forge_overlay::kinds::alert::KIND_ID as ALERT_OVERLAY_KIND;
    use forge_storage::{Language, MockOverlayRepo, StorageError, reserved_keys};

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{overlay_named, test_backend};

    const CHOSEN: &str = "stage-audio";

    const EVERY_FALLBACK: [RouteFallback; 5] = [
        RouteFallback::NoDestinationChosen,
        RouteFallback::ServerUnavailable,
        RouteFallback::DestinationUnreadable,
        RouteFallback::DestinationNotFound,
        RouteFallback::DestinationNotAudioOverlay,
    ];

    /// A key that reached the pane untranslated renders as the key itself, which every message
    /// in this card happens to prefix the same way.
    fn reads_as_a_raw_key(text: &str) -> bool {
        text.starts_with("settings_audio_")
    }

    fn without_digits(text: &str) -> String {
        text.chars().filter(|c| !c.is_ascii_digit()).collect()
    }

    fn catalog(listed: Vec<forge_storage::OverlayDefinition>) -> Arc<dyn OverlayRepo> {
        let mut repo = MockOverlayRepo::new();
        let held = listed.clone();
        repo.expect_list().returning(move || Ok(held.clone()));
        repo.expect_get().returning(move |id| {
            Ok(listed
                .iter()
                .find(|definition| &definition.id == id)
                .cloned())
        });
        Arc::new(repo)
    }

    #[test]
    fn every_route_is_named_from_the_catalog_rather_than_by_its_stored_token() {
        install_language(Language::En);
        let mut labels = BTreeSet::new();

        for route in [AudioRoute::Local, AudioRoute::Overlay, AudioRoute::Both] {
            let label = route_label(route);

            assert_ne!(
                label,
                route.as_str(),
                "{route:?} fell through to its stored token"
            );
            assert!(
                !reads_as_a_raw_key(&label),
                "{route:?} has no catalog entry, so {label} reaches the pane"
            );
            assert!(
                labels.insert(label),
                "{route:?} shares another route's name"
            );
        }
    }

    #[test]
    fn every_route_fallback_reason_is_read_out_in_the_language_the_user_reads() {
        install_language(Language::Uk);
        let mut reasons = BTreeSet::new();

        for fallback in EVERY_FALLBACK {
            let reason = fallback_label(fallback);

            assert!(
                !reads_as_a_raw_key(&reason),
                "{fallback:?} has no catalog entry, so {reason} reaches the pane"
            );
            assert_ne!(
                reason,
                fallback.to_string(),
                "{fallback:?} puts its English log wording inside a Ukrainian sentence"
            );
            assert!(
                reasons.insert(reason),
                "{fallback:?} shares another fallback's wording, so the pane cannot tell them apart"
            );
        }
    }

    #[test]
    fn the_in_effect_line_names_a_reason_only_once_the_route_has_fallen_back() {
        install_language(Language::En);
        let standing = RoutePlan {
            route: AudioRoute::Overlay,
            destination: Some(OverlayId::new(CHOSEN)),
            fallback: None,
        };
        let fell_back = RoutePlan {
            route: AudioRoute::Local,
            destination: None,
            fallback: Some(RouteFallback::ServerUnavailable),
        };

        assert_eq!(
            in_effect_text(&standing),
            route_label(AudioRoute::Overlay),
            "a plan that stands was dressed up as a fallback"
        );

        let line = in_effect_text(&fell_back);
        assert!(
            line.contains(&route_label(AudioRoute::Local)),
            "the fallback line dropped the route that is actually playing: {line}"
        );
        assert!(
            line.contains(&fallback_label(RouteFallback::ServerUnavailable)),
            "the fallback line dropped the reason: {line}"
        );
    }

    #[test]
    fn the_empty_picker_note_shows_only_while_no_audio_overlay_exists() {
        install_language(Language::En);
        let offered = [OverlayChoice {
            id: OverlayId::new(CHOSEN),
            name: "Stage audio".to_owned(),
        }];

        let note = no_overlays_text(&[]).expect("an empty picker explains itself");
        assert!(
            !reads_as_a_raw_key(&note),
            "the note has no catalog entry, so {note} reaches the pane"
        );
        assert_eq!(
            no_overlays_text(&offered),
            None,
            "the note stayed up while an audio overlay was on offer"
        );
    }

    #[test]
    fn the_page_count_is_worded_for_the_count_in_both_languages() {
        for language in [Language::En, Language::Uk] {
            install_language(language);
            let empty = connected_pages_text(0);

            assert!(
                !empty.contains(|c: char| c.is_ascii_digit()),
                "{language:?} counted zero pages with a number instead of the empty wording: {empty}"
            );
            assert!(
                !reads_as_a_raw_key(&empty),
                "{language:?} has no catalog entry for an empty overlay: {empty}"
            );

            for connected in [1_usize, 2, 5] {
                let text = connected_pages_text(connected);

                assert!(
                    text.contains(&connected.to_string()),
                    "{language:?} lost the count of {connected}: {text}"
                );
            }
        }
    }

    /// Ukrainian needs three wordings for the same sentence; a catalog that kept only the
    /// catch-all reads wrong for every count but one.
    #[test]
    fn ukrainian_page_counts_are_declined_per_plural_category() {
        install_language(Language::Uk);

        let wordings: BTreeSet<String> = [1_usize, 2, 5]
            .into_iter()
            .map(|connected| without_digits(&connected_pages_text(connected)))
            .collect();

        assert_eq!(
            wordings.len(),
            3,
            "one wording serves several plural categories: {wordings:?}"
        );
    }

    #[test]
    fn the_duplicate_audio_warning_waits_for_a_second_connected_page() {
        install_language(Language::En);

        for quiet in [0, SINGLE_PAGE] {
            assert_eq!(
                duplicate_pages_text(quiet),
                None,
                "{quiet} connected page(s) warned about hearing the clip more than once"
            );
        }

        for connected in [2_usize, 7] {
            let warning =
                duplicate_pages_text(connected).expect("a second page is worth warning about");

            assert!(
                warning.contains(&connected.to_string()),
                "the warning never says how many times the clip is heard: {warning}"
            );
        }
    }

    #[tokio::test]
    async fn only_audio_overlays_are_offered_as_destinations() {
        let (backend, _writes) = test_backend();
        let overlays = catalog(vec![
            overlay_named("alert-box", ALERT_OVERLAY_KIND),
            overlay_named(CHOSEN, AUDIO_OVERLAY_KIND),
        ]);

        let loaded = load_routing(backend as Arc<dyn SettingsRepo>, overlays, true)
            .await
            .expect("the routing settings load");

        assert_eq!(
            loaded
                .choices
                .iter()
                .map(|choice| choice.id.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec![CHOSEN.to_owned()]
        );
    }

    #[tokio::test]
    async fn an_overlay_list_that_cannot_be_read_fails_the_load() {
        let (backend, _writes) = test_backend();
        let mut repo = MockOverlayRepo::new();
        repo.expect_list().returning(|| {
            Err(StorageError::Connection {
                reason: "the database went away".to_owned(),
            })
        });

        let refused = load_routing(
            backend as Arc<dyn SettingsRepo>,
            Arc::new(repo) as Arc<dyn OverlayRepo>,
            true,
        )
        .await;

        assert!(refused.is_err());
    }

    #[tokio::test]
    async fn the_stored_choice_stays_visible_while_the_plan_it_produced_falls_back() {
        let (backend, _writes) = test_backend();
        backend
            .set_string(
                reserved_keys::AUDIO_SPEECH_ROUTE,
                AudioRoute::Overlay.as_str(),
            )
            .await
            .expect("the stored route is written");
        backend
            .set_string(reserved_keys::AUDIO_OVERLAY_ID, CHOSEN)
            .await
            .expect("the stored destination is written");
        let overlays = catalog(vec![overlay_named(CHOSEN, AUDIO_OVERLAY_KIND)]);

        let loaded = load_routing(backend as Arc<dyn SettingsRepo>, overlays, false)
            .await
            .expect("the routing settings load");

        assert_eq!(loaded.speech, AudioRoute::Overlay);
        assert_eq!(loaded.destination, Some(OverlayId::new(CHOSEN)));
        assert_eq!(loaded.speech_plan.route, AudioRoute::Local);
        assert_eq!(
            loaded.speech_plan.fallback,
            Some(RouteFallback::ServerUnavailable)
        );
    }
}
