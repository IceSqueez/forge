use std::sync::Arc;

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_LG, FONT_SM, FONT_XS, ForgePalette, Icon,
    InputEvent, Radius, Spacing, TextInput, icon, primary_button, radius, spacing, toggle, tr,
    with_alpha,
};
use forge_script::{ScriptHttpConfig, load_script_http_config};
use forge_storage::{DataProvider, SettingsRepo, reserved_keys};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, SharedString, Subscription, Window, div,
    prelude::*, px, relative,
};

use crate::presentation::ActivePresentation;

const DEFAULT_OP_LIMIT: u32 = 100_000;
const DEFAULT_ENGINE_TIMEOUT_MS: u32 = 500;

struct ScriptingSnapshot {
    allowed_domains: Vec<String>,
    max_calls_per_script: u32,
    http_timeout_ms: u32,
    allow_local: bool,
    max_response_bytes: u32,
    op_limit: u32,
    engine_timeout_ms: u32,
}

struct SavePayload {
    domains_csv: String,
    max_calls: u32,
    http_timeout_ms: u32,
    allow_local: bool,
    max_response_bytes: u32,
    op_limit: u32,
    engine_timeout_ms: u32,
}

pub struct SettingsScriptingView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    allowed_domains: Vec<String>,
    allow_local: bool,
    loading: bool,
    saving: bool,
    save_error: Option<String>,
    all_changes_saved: bool,
    op_limit: Entity<TextInput>,
    engine_timeout: Entity<TextInput>,
    max_calls: Entity<TextInput>,
    http_timeout: Entity<TextInput>,
    max_response_kib: Entity<TextInput>,
    domain_draft: Entity<TextInput>,
    _subs: Vec<Subscription>,
}

impl SettingsScriptingView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let http = ScriptHttpConfig::default();

        let op_limit = numeric_input("100000", &DEFAULT_OP_LIMIT.to_string(), palette, cx);
        let engine_timeout =
            numeric_input("500", &DEFAULT_ENGINE_TIMEOUT_MS.to_string(), palette, cx);
        let max_calls = numeric_input("10", &http.max_calls_per_script.to_string(), palette, cx);
        let http_timeout = numeric_input("5000", &http.timeout_ms.to_string(), palette, cx);
        let max_response_kib = numeric_input(
            "1024",
            &(http.max_response_bytes / 1024).to_string(),
            palette,
            cx,
        );
        let domain_draft = cx.new(|cx| {
            TextInput::new(tr!("settings_scripting_domains_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
        });

        let mut subs = Vec::new();
        for input in [
            &op_limit,
            &engine_timeout,
            &max_calls,
            &http_timeout,
            &max_response_kib,
        ] {
            subs.push(cx.subscribe(input, |this, _input, event: &InputEvent, cx| {
                if let InputEvent::Changed(_) = event {
                    this.mark_unsaved(cx);
                }
            }));
        }
        subs.push(
            cx.subscribe(&domain_draft, |this, _input, event: &InputEvent, cx| {
                if let InputEvent::Submitted(_) = event {
                    this.add_domain(cx);
                }
            }),
        );

        let mut view = Self {
            backend,
            rt_handle,
            allowed_domains: http.allowed_domains,
            allow_local: http.allow_local,
            loading: false,
            saving: false,
            save_error: None,
            all_changes_saved: true,
            op_limit,
            engine_timeout,
            max_calls,
            http_timeout,
            max_response_kib,
            domain_draft,
            _subs: subs,
        };
        view.load(cx);
        view
    }

    fn mark_unsaved(&mut self, cx: &mut Context<Self>) {
        self.all_changes_saved = false;
        cx.notify();
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.save_error = None;
        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(load_scripting_settings(repo).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_loaded(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_loaded(&mut self, result: Result<ScriptingSnapshot, String>, cx: &mut Context<Self>) {
        self.loading = false;
        match result {
            Ok(snap) => {
                self.allowed_domains = snap.allowed_domains;
                self.allow_local = snap.allow_local;
                self.op_limit
                    .update(cx, |i, cx| i.set_content(snap.op_limit.to_string(), cx));
                self.engine_timeout.update(cx, |i, cx| {
                    i.set_content(snap.engine_timeout_ms.to_string(), cx)
                });
                self.max_calls.update(cx, |i, cx| {
                    i.set_content(snap.max_calls_per_script.to_string(), cx)
                });
                self.http_timeout.update(cx, |i, cx| {
                    i.set_content(snap.http_timeout_ms.to_string(), cx)
                });
                self.max_response_kib.update(cx, |i, cx| {
                    i.set_content((snap.max_response_bytes / 1024).to_string(), cx)
                });
                self.all_changes_saved = true;
                self.save_error = None;
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to load scripting settings");
                self.save_error = Some(message);
            }
        }
        cx.notify();
    }

    fn add_domain(&mut self, cx: &mut Context<Self>) {
        let draft = self.domain_draft.read(cx).content().trim().to_owned();
        if !draft.is_empty() && !self.allowed_domains.contains(&draft) {
            self.allowed_domains.push(draft);
            self.domain_draft.update(cx, |i, cx| i.clear(cx));
            self.all_changes_saved = false;
        }
        cx.notify();
    }

    fn remove_domain(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.allowed_domains.len() {
            self.allowed_domains.remove(index);
            self.all_changes_saved = false;
        }
        cx.notify();
    }

    fn toggle_allow_local(&mut self, cx: &mut Context<Self>) {
        self.allow_local = !self.allow_local;
        self.all_changes_saved = false;
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let max_calls = self
            .max_calls
            .read(cx)
            .content()
            .parse::<u32>()
            .ok()
            .filter(|v| (1..=100).contains(v))
            .unwrap_or(10);
        let http_timeout_ms = self
            .http_timeout
            .read(cx)
            .content()
            .parse::<u32>()
            .ok()
            .filter(|v| (100..=30_000).contains(v))
            .unwrap_or(5_000);
        let max_response_bytes = self
            .max_response_kib
            .read(cx)
            .content()
            .parse::<u32>()
            .ok()
            .map(|kib| kib.saturating_mul(1024))
            .filter(|v| (1024..=10_485_760).contains(v))
            .unwrap_or(1_048_576);
        let op_limit = self
            .op_limit
            .read(cx)
            .content()
            .parse::<u32>()
            .ok()
            .filter(|v| (1_000..=10_000_000).contains(v))
            .unwrap_or(100_000);
        let engine_timeout_ms = self
            .engine_timeout
            .read(cx)
            .content()
            .parse::<u32>()
            .ok()
            .filter(|v| (50..=10_000).contains(v))
            .unwrap_or(500);

        let payload = SavePayload {
            domains_csv: self.allowed_domains.join(","),
            max_calls,
            http_timeout_ms,
            allow_local: self.allow_local,
            max_response_bytes,
            op_limit,
            engine_timeout_ms,
        };

        self.saving = true;
        self.all_changes_saved = false;
        self.save_error = None;

        let repo = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(do_save(repo, payload).await);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_save_result(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_save_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.saving = false;
        match result {
            Ok(()) => {
                self.all_changes_saved = true;
                self.save_error = None;
            }
            Err(message) => {
                tracing::warn!(error = %message, "failed to save scripting settings");
                self.save_error = Some(message);
            }
        }
        cx.notify();
    }

    fn save_indicator(&self, palette: &ForgePalette) -> AnyElement {
        if let Some(err) = &self.save_error {
            return div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.random)
                .child(tr!("settings_scripting_save_failed", error = err.as_str()))
                .into_any_element();
        }
        if self.saving {
            return div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_faint)
                .child(tr!("settings_scripting_saving"))
                .into_any_element();
        }
        if self.all_changes_saved {
            return div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .child(icon(Icon::CircleCheck, px(13.0), palette.success))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.success)
                        .child(tr!("settings_scripting_all_saved")),
                )
                .into_any_element();
        }
        div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.warning)
            .child(tr!("settings_scripting_unsaved"))
            .into_any_element()
    }

    fn header_row(&self, palette: &ForgePalette) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(icon(Icon::FileCode, px(20.0), palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(tr!("settings_scripting_title")),
            )
            .child(div().flex_1())
            .child(self.save_indicator(palette))
    }

    fn domains_block(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("settings_scripting_allowed_domains_label")),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_scripting_allowed_domains_hint")),
            )
            .child(self.domain_list(palette, density, cx))
    }

    fn domain_list(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut chips = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(spacing(Spacing::Xxs, density));
        for (idx, domain) in self.allowed_domains.iter().enumerate() {
            chips = chips.child(self.domain_chip(idx, domain, palette, cx));
        }

        let add_btn = div()
            .id("settings-scripting-domain-add")
            .flex()
            .items_center()
            .justify_center()
            .px(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(|style| style.bg(with_alpha(palette.brand, 0.08)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.add_domain(cx)))
            .child(icon(Icon::Plus, FONT_SM, palette.text_secondary));

        let input_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(div().flex_1().child(self.domain_draft.clone()))
            .child(add_btn);

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(chips)
            .child(input_row)
    }

    fn domain_chip(
        &self,
        idx: usize,
        domain: &str,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .rounded(radius(Radius::Pill))
            .bg(palette.surface_overlay)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(domain.to_owned()),
            )
            .child(
                div()
                    .id(SharedString::from(format!(
                        "settings-scripting-domain-{idx}"
                    )))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_domain(idx, cx)),
                    )
                    .child(icon(Icon::X, px(11.0), palette.text_muted)),
            )
    }

    fn allow_local_row(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> impl IntoElement {
        let labels = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(tr!("settings_scripting_allow_local_label")),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_scripting_allow_local_description")),
            );

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .child(labels)
            .child(toggle(self.allow_local, palette).on_click(
                "settings-scripting-allow-local",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_allow_local(cx)),
            ))
    }
}

impl Render for SettingsScriptingView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let engine_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(section_header(
                tr!("settings_scripting_engine_section"),
                &palette,
            ))
            .child(section_rule(&palette))
            .child(labeled_row(
                tr!("settings_scripting_op_limit_label"),
                tr!("settings_scripting_op_limit_hint"),
                self.op_limit.clone().into_any_element(),
                &palette,
                density,
            ))
            .child(labeled_row(
                tr!("settings_scripting_engine_timeout_label"),
                tr!("settings_scripting_engine_timeout_hint"),
                self.engine_timeout.clone().into_any_element(),
                &palette,
                density,
            ));

        let mut http_section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(section_header(
                tr!("settings_scripting_http_section"),
                &palette,
            ))
            .child(section_rule(&palette))
            .child(self.domains_block(&palette, density, cx))
            .child(labeled_row(
                tr!("settings_scripting_max_calls_label"),
                tr!("settings_scripting_max_calls_hint"),
                self.max_calls.clone().into_any_element(),
                &palette,
                density,
            ))
            .child(labeled_row(
                tr!("settings_scripting_http_timeout_label"),
                tr!("settings_scripting_http_timeout_hint"),
                self.http_timeout.clone().into_any_element(),
                &palette,
                density,
            ))
            .child(labeled_row(
                tr!("settings_scripting_max_response_label"),
                tr!("settings_scripting_max_response_hint"),
                self.max_response_kib.clone().into_any_element(),
                &palette,
                density,
            ))
            .child(self.allow_local_row(&palette, cx));

        if self.allow_local {
            http_section = http_section.child(
                div()
                    .py(px(4.0))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.random)
                    .child(tr!("settings_scripting_ssrf_warning")),
            );
        }

        let save_btn = primary_button(tr!("common_save"), &palette).on_click(
            "settings-scripting-save",
            cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
        );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Lg, density))
            .child(self.header_row(&palette))
            .child(engine_section)
            .child(http_section)
            .child(div().child(save_btn))
    }
}

fn numeric_input(
    placeholder: &'static str,
    seed: &str,
    palette: ForgePalette,
    cx: &mut Context<SettingsScriptingView>,
) -> Entity<TextInput> {
    cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx)
            .with_palette(palette)
            .with_font_size(FONT_SM);
        if !seed.is_empty() {
            input.set_content(seed.to_owned(), cx);
        }
        input
    })
}

fn section_header(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_BODY_FAMILY)
        .font_weight(FontWeight::MEDIUM)
        .text_size(FONT_SM)
        .text_color(palette.text_primary)
        .child(label.into())
}

fn section_rule(palette: &ForgePalette) -> impl IntoElement {
    div().h(px(1.0)).w_full().bg(palette.border_regular)
}

fn labeled_row(
    label: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    control: AnyElement,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(spacing(Spacing::Md, density))
        .child(
            div()
                .flex_grow()
                .flex_shrink()
                .flex_basis(relative(4.0))
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, density))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child(label.into()),
                )
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(hint.into()),
                ),
        )
        .child(
            div()
                .flex_grow()
                .flex_shrink()
                .flex_basis(relative(3.0))
                .child(control),
        )
}

async fn load_scripting_settings(repo: Arc<dyn SettingsRepo>) -> Result<ScriptingSnapshot, String> {
    let http = load_script_http_config(repo.as_ref()).await;
    let op_limit = repo
        .get_string(reserved_keys::SCRIPT_OP_LIMIT_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_OP_LIMIT);
    let engine_timeout_ms = repo
        .get_string(reserved_keys::SCRIPT_TIMEOUT_MS_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_ENGINE_TIMEOUT_MS);
    Ok(ScriptingSnapshot {
        allowed_domains: http.allowed_domains,
        max_calls_per_script: http.max_calls_per_script,
        http_timeout_ms: http.timeout_ms,
        allow_local: http.allow_local,
        max_response_bytes: http.max_response_bytes,
        op_limit,
        engine_timeout_ms,
    })
}

async fn do_save(repo: Arc<dyn SettingsRepo>, p: SavePayload) -> Result<(), String> {
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS_KEY,
        &p.domains_csv,
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_MAX_CALLS_KEY,
        &p.max_calls.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_TIMEOUT_MS_KEY,
        &p.http_timeout_ms.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_ALLOW_LOCAL_KEY,
        if p.allow_local { "true" } else { "false" },
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_HTTP_MAX_RESPONSE_BYTES_KEY,
        &p.max_response_bytes.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    repo.set_string(reserved_keys::SCRIPT_OP_LIMIT_KEY, &p.op_limit.to_string())
        .await
        .map_err(|e| e.to_string())?;
    repo.set_string(
        reserved_keys::SCRIPT_TIMEOUT_MS_KEY,
        &p.engine_timeout_ms.to_string(),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}
