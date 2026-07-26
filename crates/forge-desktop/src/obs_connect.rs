use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, FONT_MD, FONT_XS, ForgePalette, Icon, Radius, body_family, icon,
    mono_family, page_frame, radius, status_dot, tr,
};
use forge_events::EventPublisher;
use forge_storage::{CredentialsRepo, SettingsRepo};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::builtin_sections::grow_cell;
use crate::integrations::ObsInstallSeed;
use crate::obs_credentials_form::{DEFAULT_PORT, ObsConnected, ObsCredentialsForm, ObsSubmit};
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

const BODY_PAD_V: Pixels = px(18.0);
const BODY_PAD_H: Pixels = px(22.0);
const SECTION_GAP: Pixels = px(14.0);
const COLUMN_GAP: Pixels = px(12.0);

const HERO_PAD_V: Pixels = px(16.0);
const HERO_PAD_H: Pixels = px(18.0);
const HERO_GAP: Pixels = px(16.0);
const HERO_TILE: Pixels = px(48.0);
const HERO_TILE_RADIUS: Pixels = px(11.0);
const HERO_GLYPH: Pixels = px(24.0);

const CARD_HEAD_PAD_V: Pixels = px(12.0);
const CARD_HEAD_PAD_H: Pixels = px(14.0);
const CARD_BODY_PAD: Pixels = px(14.0);
const CARD_TITLE_SIZE: Pixels = px(13.0);
const CARD_TITLE_GAP: Pixels = px(7.0);
const CARD_GLYPH: Pixels = px(14.0);

const STEP_PAD_V: Pixels = px(7.0);
const STEP_GAP: Pixels = px(10.0);
const STEP_NUMBER_SIZE: Pixels = px(11.0);
const STEP_TEXT_SIZE: Pixels = px(11.5);
const STEP_LINE_HEIGHT: Pixels = px(17.25);
/// Fluent trims edge whitespace, so inter-fragment word spacing is a layout gap, not part of the string.
const WORD_GAP: Pixels = px(4.0);
const INTRO_LINE_HEIGHT: Pixels = px(17.8);

const HEADER_STATUS_GAP: Pixels = px(5.0);
const HEADER_DOT_SIZE: Pixels = px(7.0);
const HEADER_STATUS_SIZE: Pixels = px(11.5);

const LEFT_COLUMN_GROW: f32 = 10.0;
const RIGHT_COLUMN_GROW: f32 = 12.0;

pub struct ObsConnectView {
    form: Entity<ObsCredentialsForm>,
    _form_sub: Subscription,
}

impl EventEmitter<NavRequested> for ObsConnectView {}
impl EventEmitter<ObsConnected> for ObsConnectView {}

impl ObsConnectView {
    pub fn new(
        rt_handle: tokio::runtime::Handle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        bus: Arc<dyn EventPublisher>,
        seed: ObsInstallSeed,
        cx: &mut Context<Self>,
    ) -> Self {
        let form = cx.new(|cx| {
            ObsCredentialsForm::new(
                rt_handle,
                credentials,
                settings,
                bus,
                seed,
                ObsSubmit::Connect,
                cx,
            )
        });
        let form_sub = cx.subscribe(&form, |_, _, _: &ObsConnected, cx| cx.emit(ObsConnected));

        Self {
            form,
            _form_sub: form_sub,
        }
    }

    fn go_stream_apps(&mut self, cx: &mut Context<Self>) {
        cx.emit(NavRequested(Screen::StreamApps));
    }

    fn hero(&self, palette: &ForgePalette) -> AnyElement {
        let tile = div()
            .flex_none()
            .size(HERO_TILE)
            .flex()
            .items_center()
            .justify_center()
            .rounded(HERO_TILE_RADIUS)
            .bg(palette.surface_overlay)
            .child(icon(Icon::Broadcast, HERO_GLYPH, palette.success));

        let text = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child(tr!("obs_connect_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("obs_connect_subtitle")),
            );

        card_shell(palette)
            .flex()
            .items_center()
            .gap(HERO_GAP)
            .py(HERO_PAD_V)
            .px(HERO_PAD_H)
            .child(tile)
            .child(text)
            .into_any_element()
    }

    fn guide_card(&self, palette: &ForgePalette) -> AnyElement {
        let steps = [
            step_row(
                "1.",
                vec![
                    fragment(tr!("obs_connect_step_menu_prefix"), palette.text_primary),
                    fragment(tr!("obs_connect_step_menu_path"), palette.text_muted),
                ],
                false,
                palette,
            ),
            step_row(
                "2.",
                vec![
                    fragment(tr!("obs_connect_step_enable_prefix"), palette.text_primary),
                    fragment(tr!("obs_connect_step_enable_option"), palette.success),
                ],
                false,
                palette,
            ),
            step_row(
                "3.",
                vec![
                    fragment(tr!("obs_connect_step_port_prefix"), palette.text_primary),
                    mono_fragment(tr!("obs_connect_step_port_field"), palette.text_muted),
                    fragment(
                        tr!("obs_connect_step_port_default_prefix"),
                        palette.text_primary,
                    ),
                    mono_fragment(
                        SharedString::from(format!("{DEFAULT_PORT})")),
                        palette.text_primary,
                    ),
                ],
                false,
                palette,
            ),
            step_row(
                "4.",
                vec![
                    fragment(tr!("obs_connect_step_reveal_prefix"), palette.text_primary),
                    fragment(tr!("obs_connect_step_reveal_button"), palette.success),
                    fragment(tr!("obs_connect_step_reveal_suffix"), palette.text_primary),
                ],
                true,
                palette,
            ),
        ];

        let intro = div()
            .font_family(body_family())
            .text_size(STEP_TEXT_SIZE)
            .text_color(palette.text_muted)
            .line_height(INTRO_LINE_HEIGHT)
            .mb(SECTION_GAP)
            .child(tr!("obs_connect_guide_intro"));

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .p(CARD_BODY_PAD)
            .child(intro)
            .children(steps);

        card_shell(palette)
            .flex()
            .flex_col()
            .child(card_header(
                Icon::InfoCircle,
                palette.info,
                tr!("obs_connect_guide_title"),
                palette,
            ))
            .child(body)
            .into_any_element()
    }

    fn settings_card(&self, palette: &ForgePalette) -> AnyElement {
        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .p(CARD_BODY_PAD)
            .child(self.form.clone());

        card_shell(palette)
            .flex()
            .flex_col()
            .child(card_header(
                Icon::Plug,
                palette.success,
                tr!("obs_connect_settings_title"),
                palette,
            ))
            .child(body)
            .into_any_element()
    }

    fn header_status(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(HEADER_STATUS_GAP)
            .child(status_dot(palette.text_faint, HEADER_DOT_SIZE))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(HEADER_STATUS_SIZE)
                    .text_color(palette.text_muted)
                    .child(tr!("common_status_not_connected")),
            )
            .into_any_element()
    }
}

impl Render for ObsConnectView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let columns = div()
            .w_full()
            .flex()
            .items_stretch()
            .gap(COLUMN_GAP)
            .mb(SECTION_GAP)
            .child(grow_cell(self.guide_card(&palette), LEFT_COLUMN_GROW))
            .child(grow_cell(self.settings_card(&palette), RIGHT_COLUMN_GROW));

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .child(div().w_full().mb(SECTION_GAP).child(self.hero(&palette)))
            .child(columns);

        let scroll = div()
            .id("obs-connect-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(div().w_full().py(BODY_PAD_V).px(BODY_PAD_H).child(body));

        page_frame(
            vec![
                BreadcrumbCrumb::link(
                    tr!("stream_apps_breadcrumb"),
                    "obs-connect-crumb-apps",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.go_stream_apps(cx)),
                ),
                BreadcrumbCrumb::leaf(tr!("obs_connect_title")),
            ],
            &palette,
        )
        .density(density)
        .header_right(self.header_status(&palette))
        .body(scroll)
    }
}

fn card_shell(palette: &ForgePalette) -> gpui::Div {
    div()
        .w_full()
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
}

fn card_header(glyph: Icon, tint: Rgba, title: String, palette: &ForgePalette) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(CARD_TITLE_GAP)
        .py(CARD_HEAD_PAD_V)
        .px(CARD_HEAD_PAD_H)
        .border_b(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(icon(glyph, CARD_GLYPH, tint))
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(CARD_TITLE_SIZE)
                .text_color(palette.text_primary)
                .child(title),
        )
}

fn fragment(text: impl Into<SharedString>, tint: Rgba) -> AnyElement {
    div()
        .font_family(body_family())
        .text_size(STEP_TEXT_SIZE)
        .text_color(tint)
        .line_height(STEP_LINE_HEIGHT)
        .child(text.into())
        .into_any_element()
}

fn mono_fragment(text: impl Into<SharedString>, tint: Rgba) -> AnyElement {
    div()
        .font_family(mono_family())
        .text_size(STEP_TEXT_SIZE)
        .text_color(tint)
        .line_height(STEP_LINE_HEIGHT)
        .child(text.into())
        .into_any_element()
}

fn step_row(
    number: &'static str,
    parts: Vec<AnyElement>,
    last: bool,
    palette: &ForgePalette,
) -> AnyElement {
    let text = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_x(WORD_GAP)
        .children(parts);

    let mut row = div()
        .w_full()
        .flex()
        .items_start()
        .gap(STEP_GAP)
        .py(STEP_PAD_V)
        .child(
            div()
                .flex_none()
                .font_family(mono_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(STEP_NUMBER_SIZE)
                .text_color(palette.brand)
                .line_height(STEP_LINE_HEIGHT)
                .child(number),
        )
        .child(text);
    if !last {
        row = row
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);
    }
    row.into_any_element()
}
