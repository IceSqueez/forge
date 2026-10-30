use std::sync::Arc;

use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    Radius, Spacing, body_family, card_shell, divider, error_row, ghost_button,
    ghost_button_with_icon, icon, mono_family, primary_button, primary_button_with_icon, radius,
    scaled, secondary_button, spacing, spinner, status_dot, tr, with_alpha,
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
            .px(scaled(ROW_PAD_X, density));
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
                .child(error_row(message.clone(), palette, density))
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
                error_row(message.clone(), palette, density).into_any_element()
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
        note_box(palette.danger, palette, density)
            .child(icon(Icon::AlertTriangle, INLINE_GLYPH, palette.danger))
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
                    .ink(palette.danger)
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
            .px(scaled(ROW_PAD_X, density))
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::SystemTime;

    use forge_events::EventPublisher;
    use forge_platform_twitch::{
        BotLoginError, BotModerator, BotModeratorError, BotStatus, DeviceCodeInfo,
    };
    use forge_runtime::{EventBus, NullEventLogRepo};
    use forge_storage::CredentialsRepo;
    use gpui::{AppContext as _, Entity, TestAppContext};

    use super::{
        BotLogin, BotProblem, BotTask, SentAs, TwitchBotSection, default_sender, login_failure,
        moderator_failure,
    };
    use crate::i18n::install_language;
    use crate::test_support::{pump, runtime, test_backend};
    use crate::twitch_bot_presence::TwitchBotPresence;
    use forge_storage::Language;

    struct Mounted {
        section: Entity<TwitchBotSection>,
        presence: Entity<TwitchBotPresence>,
        rt: tokio::runtime::Runtime,
    }

    fn mount(cx: &mut TestAppContext) -> Mounted {
        install_language(Language::En);
        let rt = runtime();
        let (backend, _writes) = test_backend();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bot = crate::test_support::default_twitch_bot(
            backend as Arc<dyn CredentialsRepo>,
            bus as Arc<dyn EventPublisher>,
        );
        let handle = rt.handle().clone();
        let presence = cx.new(|_| TwitchBotPresence::new());
        let for_section = presence.clone();
        let section = cx.new(|cx| {
            TwitchBotSection::new(bot, for_section, handle, Some("streamer".to_owned()), cx)
        });
        Mounted {
            section,
            presence,
            rt,
        }
    }

    fn code() -> DeviceCodeInfo {
        DeviceCodeInfo {
            user_code: "ABCD-EFGH".to_owned(),
            verification_uri: "https://www.twitch.tv/activate".to_owned(),
            expires_at: SystemTime::UNIX_EPOCH,
        }
    }

    fn waiting() -> BotLogin {
        BotLogin::Waiting {
            user_code: "ABCD-EFGH".to_owned(),
            verification_uri: "https://www.twitch.tv/activate".to_owned(),
        }
    }

    fn login_state(m: &Mounted, cx: &mut TestAppContext) -> BotLogin {
        m.section.read_with(cx, |section, _| section.login.clone())
    }

    fn started(m: &Mounted, cx: &mut TestAppContext) -> u64 {
        m.section.update(cx, |section, cx| {
            section.start_login(cx);
            section.login_ticket
        })
    }

    fn connected(login: &str) -> BotStatus {
        BotStatus::Connected {
            login: login.to_owned(),
        }
    }

    #[gpui::test]
    fn a_code_for_the_current_login_shows_the_link_and_code(cx: &mut TestAppContext) {
        let m = mount(cx);
        let ticket = started(&m, cx);

        m.section.update(cx, |section, cx| {
            section.apply_login_code(ticket, Ok(code()), cx)
        });

        assert_eq!(login_state(&m, cx), waiting());
    }

    #[gpui::test]
    fn a_code_arriving_after_cancel_is_dropped(cx: &mut TestAppContext) {
        let m = mount(cx);
        let ticket = started(&m, cx);

        m.section.update(cx, |section, cx| {
            section.cancel_login(cx);
            section.apply_login_code(ticket, Ok(code()), cx);
        });

        assert_eq!(login_state(&m, cx), BotLogin::Idle);
    }

    #[gpui::test]
    fn an_outcome_of_a_replaced_login_is_dropped(cx: &mut TestAppContext) {
        let m = mount(cx);
        let first = started(&m, cx);
        m.section.update(cx, |section, cx| {
            section.apply_login_code(first, Ok(code()), cx)
        });
        let second = started(&m, cx);

        m.section.update(cx, |section, cx| {
            section.apply_login_outcome(first, Err(BotLoginError::Denied), cx)
        });

        assert_ne!(first, second);
        assert_eq!(login_state(&m, cx), BotLogin::Requesting);
    }

    #[gpui::test]
    fn the_current_login_outcome_ends_the_flow(cx: &mut TestAppContext) {
        for (outcome, failed) in [
            (Ok(connected("forgebot")), false),
            (Err(BotLoginError::Expired), true),
            (Err(BotLoginError::Cancelled), false),
        ] {
            let m = mount(cx);
            let ticket = started(&m, cx);
            m.section.update(cx, |section, cx| {
                section.apply_login_code(ticket, Ok(code()), cx)
            });

            m.section.update(cx, |section, cx| {
                section.apply_login_outcome(ticket, outcome, cx)
            });

            let state = login_state(&m, cx);
            assert_eq!(matches!(state, BotLogin::Failed(_)), failed, "{state:?}");
            assert_ne!(state, waiting());
        }
    }

    #[gpui::test]
    fn the_bot_connecting_ends_a_pending_login(cx: &mut TestAppContext) {
        let m = mount(cx);
        let ticket = started(&m, cx);
        m.section.update(cx, |section, cx| {
            section.apply_login_code(ticket, Ok(code()), cx)
        });

        m.presence.update(cx, |presence, cx| {
            presence.apply(connected("forgebot"), BotModerator::Unknown);
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(login_state(&m, cx), BotLogin::Idle);
    }

    #[gpui::test]
    fn a_presence_change_without_a_connected_bot_keeps_the_login_going(cx: &mut TestAppContext) {
        let m = mount(cx);
        let ticket = started(&m, cx);
        m.section.update(cx, |section, cx| {
            section.apply_login_code(ticket, Ok(code()), cx)
        });

        m.presence.update(cx, |presence, cx| {
            presence.apply(BotStatus::NotConnected, BotModerator::NotModerator);
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(login_state(&m, cx), waiting());
    }

    #[gpui::test]
    fn a_running_task_refuses_a_second_task(cx: &mut TestAppContext) {
        let m = mount(cx);

        let busy = m.section.update(cx, |section, cx| {
            section.disconnect(cx);
            section.make_moderator(cx);
            section.busy
        });

        assert_eq!(busy, Some(BotTask::Disconnecting));
    }

    #[gpui::test]
    fn failed_tasks_free_the_card_and_surface_their_problem(cx: &mut TestAppContext) {
        let m = mount(cx);

        let after_disconnect = m.section.update(cx, |section, cx| {
            section.disconnect(cx);
            section.apply_disconnect(Err(BotLoginError::Storage("disk full".to_owned())), cx);
            (section.busy, section.problem.clone())
        });
        let after_promote = m.section.update(cx, |section, cx| {
            section.make_moderator(cx);
            section.apply_make_moderator(Err(BotModeratorError::BotBanned), cx);
            (section.busy, section.problem.clone())
        });

        assert!(
            matches!(after_disconnect, (None, Some(BotProblem::Disconnect(ref m))) if m.contains("disk full")),
            "{after_disconnect:?}"
        );
        assert!(
            matches!(after_promote, (None, Some(BotProblem::Moderator(_)))),
            "{after_promote:?}"
        );
    }

    #[gpui::test]
    fn starting_a_new_attempt_clears_the_previous_problem(cx: &mut TestAppContext) {
        let m = mount(cx);

        let problem = m.section.update(cx, |section, cx| {
            section.make_moderator(cx);
            section.apply_make_moderator(Err(BotModeratorError::BotIsVip), cx);
            section.start_login(cx);
            section.problem.clone()
        });

        assert_eq!(problem, None);
    }

    #[gpui::test]
    fn sent_as_follows_the_streamer_login_handed_in_on_reload(cx: &mut TestAppContext) {
        let m = mount(cx);

        let sent_as = m.section.update(cx, |section, cx| {
            section.set_streamer_login(Some("newname".to_owned()), cx);
            section.sent_as(cx)
        });

        assert_eq!(
            sent_as,
            Some(SentAs {
                login: "newname".to_owned(),
                as_bot: false,
            })
        );
    }

    #[test]
    fn default_sender_is_the_bot_only_while_it_is_connected() {
        let bot = |login: &str| SentAs {
            login: login.to_owned(),
            as_bot: true,
        };
        let streamer = SentAs {
            login: "streamer".to_owned(),
            as_bot: false,
        };
        for (status, streamer_login, expected) in [
            (
                connected("forgebot"),
                Some("streamer"),
                Some(bot("forgebot")),
            ),
            (connected("forgebot"), None, Some(bot("forgebot"))),
            (
                BotStatus::ReauthRequired {
                    login: "forgebot".to_owned(),
                },
                Some("streamer"),
                Some(streamer.clone()),
            ),
            (BotStatus::NotConnected, Some("streamer"), Some(streamer)),
            (BotStatus::NotConnected, None, None),
        ] {
            assert_eq!(
                default_sender(&status, streamer_login),
                expected,
                "{status:?} {streamer_login:?}"
            );
        }
    }

    #[test]
    fn a_cancelled_login_returns_to_idle_without_an_error() {
        assert_eq!(login_failure(&BotLoginError::Cancelled), BotLogin::Idle);
    }

    #[test]
    fn each_login_failure_reads_differently_and_generic_ones_carry_the_cause() {
        install_language(Language::En);
        let messages: Vec<String> = [
            BotLoginError::SameAsBroadcaster,
            BotLoginError::Denied,
            BotLoginError::Expired,
            BotLoginError::Storage("disk full".to_owned()),
        ]
        .iter()
        .filter_map(|error| match login_failure(error) {
            BotLogin::Failed(message) => Some(message),
            BotLogin::Idle | BotLogin::Requesting | BotLogin::Waiting { .. } => None,
        })
        .collect();

        assert_eq!(messages.len(), 4, "every one of them is a failure");
        for message in &messages {
            assert_eq!(
                messages.iter().filter(|m| *m == message).count(),
                1,
                "{message}"
            );
        }
        assert!(messages[3].contains("disk full"), "{}", messages[3]);
    }

    #[test]
    fn each_moderator_failure_reads_differently_and_twitch_reasons_are_kept() {
        install_language(Language::En);
        let messages: Vec<String> = [
            BotModeratorError::BotNotConnected,
            BotModeratorError::BroadcasterReauthRequired,
            BotModeratorError::BotBanned,
            BotModeratorError::BotIsVip,
            BotModeratorError::Refused("user is already a mod".to_owned()),
            BotModeratorError::Unavailable("timed out".to_owned()),
        ]
        .iter()
        .map(moderator_failure)
        .collect();

        for message in &messages {
            assert_eq!(
                messages.iter().filter(|m| *m == message).count(),
                1,
                "{message}"
            );
        }
        assert!(
            messages[4].contains("user is already a mod"),
            "{}",
            messages[4]
        );
        assert!(messages[5].contains("timed out"), "{}", messages[5]);
    }

    fn set_presence(
        m: &Mounted,
        cx: &mut TestAppContext,
        status: BotStatus,
        moderator: BotModerator,
    ) {
        m.presence.update(cx, |presence, cx| {
            presence.apply(status, moderator);
            cx.notify();
        });
        cx.run_until_parked();
    }

    fn presence_state(m: &Mounted, cx: &mut TestAppContext) -> (BotStatus, BotModerator) {
        m.presence.read_with(cx, |presence, _| {
            (presence.status().clone(), presence.moderator())
        })
    }

    #[gpui::test]
    fn a_bot_account_exists_while_the_bot_is_known_or_a_bot_task_runs(cx: &mut TestAppContext) {
        let reauth = BotStatus::ReauthRequired {
            login: "forgebot".to_owned(),
        };
        for (status, login, busy, expected) in [
            (BotStatus::NotConnected, BotLogin::Idle, None, false),
            (connected("forgebot"), BotLogin::Idle, None, true),
            (reauth, BotLogin::Idle, None, true),
            (BotStatus::NotConnected, BotLogin::Requesting, None, true),
            (BotStatus::NotConnected, waiting(), None, true),
            (
                BotStatus::NotConnected,
                BotLogin::Failed("expired".to_owned()),
                None,
                true,
            ),
            (
                BotStatus::NotConnected,
                BotLogin::Idle,
                Some(BotTask::Disconnecting),
                true,
            ),
        ] {
            let m = mount(cx);
            set_presence(&m, cx, status.clone(), BotModerator::Unknown);

            let exists = m.section.update(cx, |section, cx| {
                section.login = login.clone();
                section.busy = busy;
                section.has_bot_account(cx)
            });

            assert_eq!(exists, expected, "{status:?} {login:?} {busy:?}");
        }
    }

    #[gpui::test]
    fn a_successful_login_shows_the_bot_without_waiting_for_the_status_event(
        cx: &mut TestAppContext,
    ) {
        let m = mount(cx);
        let ticket = started(&m, cx);
        m.section.update(cx, |section, cx| {
            section.apply_login_code(ticket, Ok(code()), cx)
        });

        m.section.update(cx, |section, cx| {
            section.apply_login_outcome(ticket, Ok(connected("forgebot")), cx)
        });

        assert_eq!(presence_state(&m, cx).0, connected("forgebot"));
    }

    #[gpui::test]
    fn a_successful_disconnect_shows_no_bot_and_resets_the_composer_to_me(cx: &mut TestAppContext) {
        let m = mount(cx);
        set_presence(&m, cx, connected("forgebot"), BotModerator::Moderator);
        m.presence.update(cx, |presence, _| {
            presence.set_composer_sends_as_bot(true);
        });

        m.section.update(cx, |section, cx| {
            section.busy = Some(BotTask::Disconnecting);
            section.apply_disconnect(Ok(()), cx);
        });

        let (status, as_bot) = m.presence.read_with(cx, |presence, _| {
            (presence.status().clone(), presence.composer_sends_as_bot())
        });
        assert_eq!((status, as_bot), (BotStatus::NotConnected, false));
    }

    #[gpui::test]
    fn a_successful_promotion_reads_the_account_status_again(cx: &mut TestAppContext) {
        let m = mount(cx);
        set_presence(&m, cx, connected("forgebot"), BotModerator::NotModerator);

        m.section.update(cx, |section, cx| {
            section.busy = Some(BotTask::Promoting);
            section.apply_make_moderator(Ok(()), cx);
        });
        pump(&m.rt);
        cx.run_until_parked();

        assert_eq!(
            presence_state(&m, cx),
            (BotStatus::NotConnected, BotModerator::Unknown)
        );
    }

    #[gpui::test]
    fn a_promotion_refused_for_streamer_permissions_asks_for_a_streamer_sign_in(
        cx: &mut TestAppContext,
    ) {
        for (error, sign_in) in [
            (BotModeratorError::BroadcasterReauthRequired, true),
            (BotModeratorError::BotBanned, false),
            (BotModeratorError::Refused("nope".to_owned()), false),
        ] {
            let m = mount(cx);

            let problem = m.section.update(cx, |section, cx| {
                section.busy = Some(BotTask::Promoting);
                section.apply_make_moderator(Err(error), cx);
                section.problem.clone()
            });

            assert_eq!(
                matches!(problem, Some(BotProblem::StreamerSignIn(_))),
                sign_in,
                "{problem:?}"
            );
            assert!(
                matches!(
                    problem,
                    Some(BotProblem::StreamerSignIn(_) | BotProblem::Moderator(_))
                ),
                "{problem:?}"
            );
        }
    }

    #[gpui::test]
    fn becoming_a_moderator_clears_moderator_problems_but_not_a_disconnect_problem(
        cx: &mut TestAppContext,
    ) {
        for (problem, cleared) in [
            (BotProblem::Moderator("banned".to_owned()), true),
            (BotProblem::StreamerSignIn("sign in".to_owned()), true),
            (BotProblem::Disconnect("disk full".to_owned()), false),
        ] {
            let m = mount(cx);
            set_presence(&m, cx, connected("forgebot"), BotModerator::NotModerator);
            m.section
                .update(cx, |section, _| section.problem = Some(problem.clone()));

            set_presence(&m, cx, connected("forgebot"), BotModerator::Moderator);

            let left = m
                .section
                .read_with(cx, |section, _| section.problem.clone());
            assert_eq!(left.is_none(), cleared, "{problem:?} -> {left:?}");
        }
    }

    #[gpui::test]
    fn a_moderator_problem_stays_while_the_bot_is_still_not_a_moderator(cx: &mut TestAppContext) {
        let m = mount(cx);
        m.section.update(cx, |section, _| {
            section.problem = Some(BotProblem::Moderator("banned".to_owned()))
        });

        set_presence(&m, cx, connected("forgebot"), BotModerator::NotModerator);

        let left = m
            .section
            .read_with(cx, |section, _| section.problem.clone());
        assert!(matches!(left, Some(BotProblem::Moderator(_))), "{left:?}");
    }

    #[test]
    fn the_reauth_fallback_line_names_the_streamer_in_each_language() {
        for language in [Language::En, Language::Uk] {
            install_language(language);

            let line =
                forge_components::tr!("twitch_bot_fallback_active", login = "streamer".to_owned())
                    .replace(['\u{2068}', '\u{2069}'], "");

            assert!(line.contains("@streamer"), "{language:?}: {line}");
        }
    }
}
