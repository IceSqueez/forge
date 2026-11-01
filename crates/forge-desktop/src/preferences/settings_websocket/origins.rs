use super::*;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, Density, FONT_XS, ForgePalette, Icon, Spacing, error_row,
    ghost_button_with_icon, icon, setting_block, spacing, tr,
};
use forge_server::ServerSettings;
use forge_storage::SettingsRepo;
use gpui::{App, ClickEvent, Context, Focusable, Pixels, Window, div, px};

use crate::app::async_bridge::{self};
use crate::shared::repo_failure::RepoFailure;
use crate::tools::server_restart::restart_ignoring_disabled;

const ORIGINS_PERSIST_CONTEXT: &str = "websocket additional origins";
const WARNING_GLYPH: Pixels = px(12.0);

impl SettingsWebSocketView {
    pub(super) fn attach_origins_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.origins_input.focus_handle(cx);
        self.origins_blur = Some(cx.on_blur(&handle, window, |this, _window, cx| {
            this.commit_origins(cx);
        }));
    }

    pub(super) fn clear_origins_error(&mut self, cx: &mut Context<Self>) {
        if self.origins_error.is_none() {
            return;
        }
        self.origins_error = None;
        self.origins_input
            .update(cx, |i, cx| i.set_invalid(false, cx));
        cx.notify();
    }

    fn origins_editable(&self) -> bool {
        self.settings_loaded() && !self.additional_origins_unreadable
    }

    pub(super) fn commit_origins(&mut self, cx: &mut Context<Self>) {
        if !self.origins_editable() {
            return;
        }
        let raw = self.origins_input.read(cx).content().to_owned();
        match parse_origins(&raw) {
            Ok(origins) => {
                self.origins_error = None;
                self.origins_input
                    .update(cx, |i, cx| i.set_invalid(false, cx));
                self.origins_input
                    .update(cx, |i, cx| i.set_content(origins.join("\n"), cx));
                if origins == self.additional_origins {
                    cx.notify();
                    return;
                }
                let prev = std::mem::replace(&mut self.additional_origins, origins.clone());
                let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
                self.persist_field(
                    prev,
                    |this, prev, cx| {
                        this.origins_input
                            .update(cx, |i, cx| i.set_content(prev.join("\n"), cx));
                        this.additional_origins = prev;
                    },
                    AfterSave::ReloadServer,
                    async move {
                        ServerSettings::save_additional_origins(repo.as_ref(), &origins)
                            .await
                            .map_err(RepoFailure::from)
                    },
                    cx,
                );
            }
            Err(RejectedOrigin(rejected)) => {
                self.origins_error = Some(rejected.into());
                self.origins_input
                    .update(cx, |i, cx| i.set_invalid(true, cx));
                cx.notify();
            }
        }
    }

    pub(super) fn persist_origins_on_release(&mut self, cx: &mut App) {
        if !self.origins_editable() {
            return;
        }
        let raw = self.origins_input.read(cx).content().to_owned();
        let Ok(origins) = parse_origins(&raw) else {
            return;
        };
        if origins == self.additional_origins {
            return;
        }
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let reload = self.running || self.restarting;
        let server = self.server.clone().filter(|_| reload);
        async_bridge::detached(&self.rt_handle, ORIGINS_PERSIST_CONTEXT, async move {
            ServerSettings::save_additional_origins(repo.as_ref(), &origins)
                .await
                .map_err(RepoFailure::from)?;
            if let Some(handle) = server {
                restart_ignoring_disabled(&handle).await?;
            }
            Ok::<(), RepoFailure>(())
        });
    }

    pub(super) fn origins_section(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if self.additional_origins_unreadable {
            return setting_block(
                tr!("settings_ws_origins_section_title"),
                Some(tr!("settings_ws_origins_subtitle").into()),
                error_row(tr!("settings_ws_origins_unreadable"), palette, density, cx),
                palette,
                density,
                cx,
            );
        }
        let apply =
            ghost_button_with_icon(Icon::Check, tr!("settings_ws_origins_apply_btn"), palette)
                .on_click(
                    "settings-ws-origins-apply",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.commit_origins(cx)),
                );

        let mut footer = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));

        if let Some(rejected) = &self.origins_error {
            footer = footer.child(
                div()
                    .flex()
                    .min_w(px(0.0))
                    .items_center()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(icon(Icon::AlertTriangle, WARNING_GLYPH, palette.danger))
                    .child(
                        div()
                            .flex_none()
                            .font_family(cx.body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.danger)
                            .child(tr!("settings_ws_origins_invalid")),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .font_family(cx.mono_family())
                            .text_size(FONT_XS)
                            .text_color(palette.danger)
                            .child(rejected.clone()),
                    ),
            );
        }

        setting_block(
            tr!("settings_ws_origins_section_title"),
            Some(tr!("settings_ws_origins_subtitle").into()),
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(self.origins_input.clone())
                .child(footer.child(div().flex_1().min_w(px(0.0))).child(apply)),
            palette,
            density,
            cx,
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RejectedOrigin(String);

fn parse_origins(raw: &str) -> Result<Vec<String>, RejectedOrigin> {
    let mut accepted: Vec<String> = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(origin) = normalize_origin(trimmed) else {
            return Err(RejectedOrigin(trimmed.to_owned()));
        };
        if !accepted.contains(&origin) {
            accepted.push(origin);
        }
    }
    Ok(accepted)
}

fn normalize_origin(raw: &str) -> Option<String> {
    let lowered = raw.trim().to_ascii_lowercase();
    let authority = lowered
        .strip_prefix("http://")
        .or_else(|| lowered.strip_prefix("https://"))?;
    let (host, port) = split_host_port(authority)?;
    if !host_is_valid(host) {
        return None;
    }
    if let Some(port) = port
        && !port.parse::<u16>().is_ok_and(|p| p > 0)
    {
        return None;
    }
    Some(lowered)
}

fn split_host_port(authority: &str) -> Option<(&str, Option<&str>)> {
    if authority.starts_with('[') {
        let close = authority.find(']')?;
        let host = &authority[..=close];
        let tail = &authority[close + 1..];
        return match tail {
            "" => Some((host, None)),
            _ => Some((host, Some(tail.strip_prefix(':')?))),
        };
    }
    match authority.split_once(':') {
        Some((_, port)) if port.contains(':') => None,
        Some((host, port)) => Some((host, Some(port))),
        None => Some((authority, None)),
    }
}

pub(crate) fn host_is_valid(host: &str) -> bool {
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return !inner.is_empty()
            && inner
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.');
    }
    !host.starts_with('.')
        && !host.ends_with('.')
        && host.split('.').all(|label| {
            !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_origin_is_kept_lowercased_for_each_host_form_and_scheme() {
        for (typed, stored) in [
            ("http://localhost", "http://localhost"),
            ("HTTPS://Overlay.Example.COM", "https://overlay.example.com"),
            ("http://a-b.test:8080", "http://a-b.test:8080"),
            ("http://127.0.0.1:65535", "http://127.0.0.1:65535"),
            ("http://[::1]", "http://[::1]"),
            ("https://[FE80::1]:443", "https://[fe80::1]:443"),
            ("  http://spaced.test  ", "http://spaced.test"),
        ] {
            assert_eq!(normalize_origin(typed).as_deref(), Some(stored), "{typed}");
        }
    }

    #[test]
    fn an_origin_the_cors_check_could_not_match_is_refused() {
        for bad in [
            "localhost",
            "ftp://a.test",
            "http://",
            "http://[::1]:",
            "http://[]",
            "http://[::1",
            "http://[::1]8080",
            "http://[zz::1]",
            "http://a..b",
            "http://.a.test",
            "http://a.test.",
            "http://a.test/",
            "http://a.test/path",
            "http://a_b.test",
            "http://зірка.test",
            "http://user@a.test",
            "http://a.test:",
            "http://a.test:0",
            "http://a.test:65536",
            "http://a.test:80:81",
            "http://a.test:http",
        ] {
            assert_eq!(normalize_origin(bad), None, "{bad}");
        }
    }

    #[test]
    fn normalizing_a_kept_origin_again_changes_nothing() {
        for typed in ["HTTP://A.Test:81", "https://[FE80::1]:443", "http://x"] {
            let once = normalize_origin(typed).expect("a valid origin");
            assert_eq!(normalize_origin(&once).as_deref(), Some(once.as_str()));
        }
    }

    #[test]
    fn origin_lines_are_trimmed_deduplicated_after_normalizing_and_kept_in_order() {
        let raw = "http://b.test\n\n   \n  HTTP://B.test \nhttp://a.test\r\n";

        assert_eq!(
            parse_origins(raw),
            Ok(vec!["http://b.test".to_owned(), "http://a.test".to_owned()])
        );
    }

    #[test]
    fn the_first_bad_origin_line_refuses_the_whole_list_and_is_named_as_typed() {
        let raw = "http://ok.test\n  Not An Origin \nalso bad";

        assert_eq!(
            parse_origins(raw),
            Err(RejectedOrigin("Not An Origin".to_owned()))
        );
    }
}
