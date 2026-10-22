use std::sync::Arc;

use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    Radius, Spacing, body_family, card_shell, divider, error_row, ghost_button,
    ghost_button_with_icon, icon, mono_family, primary_button, primary_button_with_icon, radius,
    secondary_button, spacing, spinner, status_dot, tr, with_alpha,
};
use forge_platform_twitch::{
    BotLoginError, BotModerator, BotModeratorError, BotStatus, DeviceCodeInfo, TwitchBotAccount,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, EventEmitter, FontWeight, Pixels, Render,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::builtin_sections::panel_header_row;
use crate::twitch_bot_presence::TwitchBotPresence;

const SECTION_ICON: &str = "user-check";
const ROW_PAD_X: Pixels = px(14.0);
const STATUS_DOT: Pixels = px(7.0);
const INLINE_GLYPH: Pixels = px(13.0);
const VALUE_LABEL_W: Pixels = px(110.0);
const NOTE_WASH: f32 = 0.08;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BotLogin {
    Idle,
    Requesting,
    Waiting {
        user_code: String,
        verification_uri: String,
    },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BotTask {
    Disconnecting,
    Promoting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BotProblem {
    Disconnect(String),
    Moderator(String),
    StreamerSignIn(String),
}

pub struct StreamerSignInRequested;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SentAs {
    pub login: String,
    pub as_bot: bool,
}

pub struct TwitchBotSection {
    bot: Arc<TwitchBotAccount>,
    rt_handle: tokio::runtime::Handle,
    presence: Entity<TwitchBotPresence>,
    streamer_login: Option<String>,
    login: BotLogin,
    login_ticket: u64,
    busy: Option<BotTask>,
    problem: Option<BotProblem>,
    _presence_obs: Subscription,
}

impl TwitchBotSection {
    pub fn new(
        bot: Arc<TwitchBotAccount>,
        presence: Entity<TwitchBotPresence>,
        rt_handle: tokio::runtime::Handle,
        streamer_login: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let presence_obs = cx.observe(&presence, |this, presence, cx| {
            let presence = presence.read(cx);
            if presence.is_connected() {
                this.login = BotLogin::Idle;
            }
            if presence.moderator() == BotModerator::Moderator
                && matches!(
                    this.problem,
                    Some(BotProblem::Moderator(_) | BotProblem::StreamerSignIn(_))
                )
            {
                this.problem = None;
            }
            cx.notify();
        });
        Self {
            bot,
            rt_handle,
            presence,
            streamer_login,
            login: BotLogin::Idle,
            login_ticket: 0,
            busy: None,
            problem: None,
            _presence_obs: presence_obs,
        }
    }

    pub fn has_bot_account(&self, cx: &App) -> bool {
        *self.presence.read(cx).status() != BotStatus::NotConnected
            || self.login != BotLogin::Idle
            || self.busy.is_some()
    }

    fn refresh_presence(&mut self, cx: &mut Context<Self>) {
        let bot = Arc::clone(&self.bot);
        async_bridge::run_async(
            &self.rt_handle,
            async move { (bot.status(), bot.moderator_status()) },
            |this, (status, moderator), cx| {
                this.presence.update(cx, |presence, cx| {
                    if presence.apply(status, moderator) {
                        cx.notify();
                    }
                });
            },
            cx,
        );
    }

    fn adopt_status(&mut self, status: BotStatus, cx: &mut Context<Self>) {
        self.presence.update(cx, |presence, cx| {
            let moderator = presence.moderator();
            if presence.apply(status, moderator) {
                cx.notify();
            }
        });
        self.refresh_presence(cx);
    }

    pub fn set_streamer_login(&mut self, login: Option<String>, cx: &mut Context<Self>) {
        if self.streamer_login != login {
            self.streamer_login = login;
            cx.notify();
        }
    }

    pub(crate) fn sent_as(&self, cx: &Context<Self>) -> Option<SentAs> {
        default_sender(
            self.presence.read(cx).status(),
            self.streamer_login.as_deref(),
        )
    }

    fn start_login(&mut self, cx: &mut Context<Self>) {
        self.login_ticket += 1;
        let ticket = self.login_ticket;
        self.login = BotLogin::Requesting;
        self.problem = None;
        let bot = Arc::clone(&self.bot);
        async_bridge::run_async(
            &self.rt_handle,
            async move { bot.begin_login().await },
            move |this, result, cx| this.apply_login_code(ticket, result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_login_code(
        &mut self,
        ticket: u64,
        result: Result<DeviceCodeInfo, BotLoginError>,
        cx: &mut Context<Self>,
    ) {
        if ticket != self.login_ticket {
            return;
        }
        match result {
            Ok(info) => {
                self.login = BotLogin::Waiting {
                    user_code: info.user_code,
                    verification_uri: info.verification_uri,
                };
                let bot = Arc::clone(&self.bot);
                async_bridge::run_async(
                    &self.rt_handle,
                    async move { bot.await_login().await },
                    move |this, result, cx| this.apply_login_outcome(ticket, result, cx),
                    cx,
                );
            }
            Err(error) => self.login = login_failure(&error),
        }
        cx.notify();
    }

    fn apply_login_outcome(
        &mut self,
        ticket: u64,
        result: Result<BotStatus, BotLoginError>,
        cx: &mut Context<Self>,
    ) {
        if ticket != self.login_ticket {
            return;
        }
        match result {
            Ok(status) => {
                self.login = BotLogin::Idle;
                self.adopt_status(status, cx);
            }
            Err(error) => self.login = login_failure(&error),
        }
        cx.notify();
    }

    fn cancel_login(&mut self, cx: &mut Context<Self>) {
        self.login_ticket += 1;
        self.login = BotLogin::Idle;
        let bot = Arc::clone(&self.bot);
        self.rt_handle.spawn(async move { bot.cancel_login() });
        cx.notify();
    }

    fn disconnect(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(BotTask::Disconnecting);
        self.problem = None;
        let bot = Arc::clone(&self.bot);
        async_bridge::run_async(
            &self.rt_handle,
            async move { bot.logout().await },
            |this, result, cx| this.apply_disconnect(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_disconnect(&mut self, result: Result<(), BotLoginError>, cx: &mut Context<Self>) {
        self.busy = None;
        match result {
            Ok(()) => self.adopt_status(BotStatus::NotConnected, cx),
            Err(error) => {
                self.problem = Some(BotProblem::Disconnect(tr!(
                    "twitch_bot_disconnect_failed",
                    error = error.to_string()
                )));
            }
        }
        cx.notify();
    }

    fn make_moderator(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(BotTask::Promoting);
        self.problem = None;
        let bot = Arc::clone(&self.bot);
        async_bridge::run_async(
            &self.rt_handle,
            async move { bot.make_moderator().await },
            |this, result, cx| this.apply_make_moderator(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_make_moderator(
        &mut self,
        result: Result<(), BotModeratorError>,
        cx: &mut Context<Self>,
    ) {
        self.busy = None;
        match result {
            Ok(()) => self.refresh_presence(cx),
            Err(BotModeratorError::BroadcasterReauthRequired) => {
                self.problem = Some(BotProblem::StreamerSignIn(moderator_failure(
                    &BotModeratorError::BroadcasterReauthRequired,
                )));
            }
            Err(error) => {
                self.problem = Some(BotProblem::Moderator(moderator_failure(&error)));
            }
        }
        cx.notify();
    }

    fn copy(text: String, cx: &mut Context<Self>) {
        crate::toasts::copy_to_clipboard(text, cx);
    }

    fn header_status(&self, status: &BotStatus) -> String {
        match (&self.login, status) {
            (BotLogin::Requesting | BotLogin::Waiting { .. }, _) => {
                tr!("twitch_bot_header_signing_in")
            }
            (_, BotStatus::Connected { login }) => format!("@{login}"),
            (_, BotStatus::ReauthRequired { .. }) => tr!("twitch_bot_header_reauth"),
            (_, BotStatus::NotConnected) => tr!("common_status_not_connected"),
        }
    }

    fn body(
        &self,
        status: &BotStatus,
        moderator: BotModerator,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Div {
        let column = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(ROW_PAD_X);
        let column = match (&self.login, status) {
            (BotLogin::Requesting, _) => column.child(self.requesting_row(palette, density, cx)),
            (
                BotLogin::Waiting {
                    user_code,
                    verification_uri,
                },
                _,
            ) => column
                .child(muted_text(tr!("twitch_bot_private_window_hint"), palette))
                .child(copy_row(
                    "twitch-bot-copy-link",
                    tr!("twitch_bot_activation_link"),
                    verification_uri.clone(),
                    false,
                    palette,
                    density,
                    cx,
                ))
                .child(copy_row(
                    "twitch-bot-copy-code",
                    tr!("twitch_bot_activation_code"),
                    user_code.clone(),
                    true,
                    palette,
                    density,
                    cx,
                ))
                .child(self.waiting_row(palette, density, cx)),
            (BotLogin::Failed(message), _) => column
                .child(error_row(message.clone(), palette))
                .child(self.retry_row(palette, density, cx)),
            (BotLogin::Idle, BotStatus::NotConnected) => column
                .child(muted_text(tr!("twitch_bot_intro"), palette))
                .child(
                    div().flex().child(
                        primary_button_with_icon(
                            Icon::Plug,
                            tr!("twitch_bot_connect_btn"),
                            palette,
                        )
                        .density(density)
                        .on_click(
                            "twitch-bot-connect",
                            cx.listener(|this, _: &ClickEvent, _, cx| this.start_login(cx)),
                        ),
                    ),
                ),
            (BotLogin::Idle, BotStatus::Connected { login }) => {
                let mut column =
                    column.child(self.account_row(login, palette.success, palette, density, cx));
                if moderator == BotModerator::NotModerator {
                    column = column.child(self.moderator_note(palette, density, cx));
                }
                column
            }
            (BotLogin::Idle, BotStatus::ReauthRequired { login }) => column
                .child(self.account_row(login, palette.warning, palette, density, cx))
                .child(self.reauth_row(palette, density, cx)),
        };
        column.children(self.problem.as_ref().map(|problem| match problem {
            BotProblem::Disconnect(message) | BotProblem::Moderator(message) => {
                error_row(message.clone(), palette).into_any_element()
            }
            BotProblem::StreamerSignIn(message) => {
                self.streamer_sign_in_row(message.clone(), palette, density, cx)
            }
        }))
    }

    fn streamer_sign_in_row(
        &self,
        message: String,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        note_box(palette.random, palette, density)
            .child(icon(Icon::AlertTriangle, INLINE_GLYPH, palette.random))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(message),
            )
            .child(
                primary_button(tr!("twitch_reauth_btn"), palette)
                    .density(density)
                    .on_click(
                        "twitch-bot-streamer-sign-in",
                        cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(StreamerSignInRequested)),
                    ),
            )
            .into_any_element()
    }

    fn requesting_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        progress_row(
            "twitch-bot-requesting",
            tr!("twitch_bot_requesting"),
            palette,
            density,
        )
        .child(self.cancel_button(palette, density, cx))
        .into_any_element()
    }

    fn waiting_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        progress_row(
            "twitch-bot-waiting",
            tr!("twitch_bot_waiting"),
            palette,
            density,
        )
        .child(self.cancel_button(palette, density, cx))
        .into_any_element()
    }

    fn cancel_button(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        ghost_button(tr!("common_cancel"), palette)
            .density(density)
            .on_click(
                "twitch-bot-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_login(cx)),
            )
            .into_any_element()
    }

    fn retry_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                primary_button(tr!("twitch_bot_try_again_btn"), palette)
                    .density(density)
                    .on_click(
                        "twitch-bot-retry",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.start_login(cx)),
                    ),
            )
            .child(self.cancel_button(palette, density, cx))
            .into_any_element()
    }

    fn account_row(
        &self,
        login: &str,
        dot: gpui::Rgba,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disconnecting = self.busy == Some(BotTask::Disconnecting);
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(status_dot(dot, STATUS_DOT))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(format!("@{login}")),
            )
            .child(
                ghost_button_with_icon(Icon::Logout, tr!("twitch_bot_disconnect_btn"), palette)
                    .ink(palette.random)
                    .density(density)
                    .busy(disconnecting)
                    .on_click(
                        "twitch-bot-disconnect",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.disconnect(cx)),
                    ),
            )
            .into_any_element()
    }

    fn reauth_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        note_box(palette.warning, palette, density)
            .child(icon(Icon::AlertTriangle, INLINE_GLYPH, palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(tr!("twitch_bot_reauth_detail"))
                    .children(self.streamer_login.as_ref().map(|login| {
                        div()
                            .text_color(palette.warning)
                            .child(tr!("twitch_bot_fallback_active", login = login.clone()))
                    })),
            )
            .child(
                primary_button(tr!("twitch_bot_reauth_btn"), palette)
                    .density(density)
                    .on_click(
                        "twitch-bot-reauth",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.start_login(cx)),
                    ),
            )
            .into_any_element()
    }

    fn moderator_note(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let promoting = self.busy == Some(BotTask::Promoting);
        note_box(palette.info, palette, density)
            .child(icon(Icon::Shield, INLINE_GLYPH, palette.info))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("twitch_bot_not_moderator_note")),
            )
            .child(
                secondary_button(tr!("twitch_bot_make_moderator_btn"), palette)
                    .density(density)
                    .busy(promoting)
                    .on_click(
                        "twitch-bot-make-moderator",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.make_moderator(cx)),
                    ),
            )
            .into_any_element()
    }

    fn sent_as_row(&self, palette: &ForgePalette, density: Density, cx: &Context<Self>) -> Div {
        let (value, role) = match self.sent_as(cx) {
            Some(sent_as) => (
                format!("@{}", sent_as.login),
                if sent_as.as_bot {
                    tr!("twitch_bot_sent_as_role_bot")
                } else {
                    tr!("twitch_bot_sent_as_role_streamer")
                },
            ),
            None => ("\u{2014}".to_owned(), String::new()),
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .px(ROW_PAD_X)
            .bg(palette.shell)
            .child(
                div()
                    .w(VALUE_LABEL_W)
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("twitch_bot_sent_as_label")),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(value),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(role),
            )
    }
}

impl EventEmitter<StreamerSignInRequested> for TwitchBotSection {}

impl Render for TwitchBotSection {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let (status, moderator) = {
            let presence = self.presence.read(cx);
            (presence.status().clone(), presence.moderator())
        };
        let header_status = self.header_status(&status);
        let body = self.body(&status, moderator, &palette, density, cx);
        let sent_as = self.sent_as_row(&palette, density, cx);
        card_shell(&palette)
            .child(panel_header_row(
                SECTION_ICON,
                &tr!("twitch_bot_section_title"),
                None,
                Some(&header_status),
                None,
                &palette,
                density,
            ))
            .child(divider(&palette))
            .child(body)
            .child(divider(&palette))
            .child(sent_as)
    }
}

pub(crate) fn default_sender(status: &BotStatus, streamer_login: Option<&str>) -> Option<SentAs> {
    match status {
        BotStatus::Connected { login } => Some(SentAs {
            login: login.clone(),
            as_bot: true,
        }),
        BotStatus::NotConnected | BotStatus::ReauthRequired { .. } => {
            streamer_login.map(|login| SentAs {
                login: login.to_owned(),
                as_bot: false,
            })
        }
    }
}

pub(crate) fn login_failure(error: &BotLoginError) -> BotLogin {
    match error {
        BotLoginError::Cancelled => BotLogin::Idle,
        BotLoginError::SameAsBroadcaster => BotLogin::Failed(tr!("twitch_bot_error_same_account")),
        BotLoginError::Denied => BotLogin::Failed(tr!("twitch_bot_error_denied")),
        BotLoginError::Expired => BotLogin::Failed(tr!("twitch_bot_error_expired")),
        BotLoginError::NotStarted | BotLoginError::Platform(_) | BotLoginError::Storage(_) => {
            BotLogin::Failed(tr!("twitch_bot_error_failed", error = error.to_string()))
        }
    }
}

pub(crate) fn moderator_failure(error: &BotModeratorError) -> String {
    match error {
        BotModeratorError::BotNotConnected => tr!("twitch_bot_mod_error_not_connected"),
        BotModeratorError::BroadcasterReauthRequired => tr!("twitch_bot_mod_error_streamer_reauth"),
        BotModeratorError::BotBanned => tr!("twitch_bot_mod_error_banned"),
        BotModeratorError::BotIsVip => tr!("twitch_bot_mod_error_vip"),
        BotModeratorError::Refused(reason) => {
            tr!("twitch_bot_mod_error_refused", reason = reason.clone())
        }
        BotModeratorError::Unavailable(reason) => {
            tr!("twitch_bot_mod_error_unavailable", reason = reason.clone())
        }
    }
}

fn muted_text(text: String, palette: &ForgePalette) -> Div {
    div()
        .w_full()
        .font_family(body_family())
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(text)
}

fn progress_row(id: &'static str, label: String, palette: &ForgePalette, density: Density) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .child(spinner(
            id,
            Icon::Loader2,
            INLINE_GLYPH,
            palette.theme_brand,
        ))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(label),
        )
}

fn note_box(accent: gpui::Rgba, palette: &ForgePalette, density: Density) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(accent)
        .bg(with_alpha(accent, NOTE_WASH))
        .text_color(palette.text_primary)
}

fn copy_row(
    id: &'static str,
    label: String,
    value: String,
    emphasized: bool,
    palette: &ForgePalette,
    density: Density,
    cx: &mut Context<TwitchBotSection>,
) -> AnyElement {
    let (size, weight, color) = if emphasized {
        (FONT_SM, FontWeight::SEMIBOLD, palette.theme_brand)
    } else {
        (FONT_XS, FontWeight::NORMAL, palette.info)
    };
    let copied = value.clone();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(
            div()
                .w(VALUE_LABEL_W)
                .flex_none()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Sm, density))
                .rounded(radius(Radius::Sm))
                .border(BORDER_THIN)
                .border_color(palette.border_regular)
                .bg(palette.shell)
                .font_family(mono_family())
                .font_weight(weight)
                .text_size(size)
                .text_color(color)
                .child(SharedString::from(value)),
        )
        .child(
            ghost_button_with_icon(Icon::Copy, tr!("twitch_device_copy"), palette)
                .density(density)
                .on_click(
                    id,
                    cx.listener(move |_, _: &ClickEvent, _, cx| {
                        TwitchBotSection::copy(copied.clone(), cx)
                    }),
                ),
        )
        .into_any_element()
}
