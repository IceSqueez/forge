use std::ops::RangeInclusive;
use std::sync::Arc;

use forge_components::{
    DISABLED_OPACITY, Density, FONT_SM, FONT_XS, ForgePalette, Icon, InputEvent, PushToast, Radius,
    SettingRow, Spacing, TextInput, ToastKind, body_family, error_row_with_retry, icon,
    icon_button, radius, setting_block, setting_groups, setting_row, setting_rows, spacing, toggle,
    tr,
};
use forge_script::{EngineConfig, ScriptHttpConfig, stored_script_http_allowed_domains};
use forge_storage::{
    Count, DataProvider, Flag, Setting, SettingsRepo, StorageError, read_setting_or_default,
    write_setting,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};

use crate::async_bridge::{
    self, Debounced, ErrorSink, Loadable, SLIDER_PERSIST_DEBOUNCE, SnapshotWriter, WriteOutcome,
};
use crate::navigation::SettingsSection;
use crate::settings_pane_header::SettingsPaneHeader;
use forge_components::ActivePresentation;

const REMOVE_GLYPH: Pixels = px(11.0);
const NUMERIC_INPUT_WIDTH: Pixels = px(120.0);
const BYTES_PER_KIB: u64 = 1024;

const OP_LIMIT_RANGE: RangeInclusive<u64> = 1_000..=10_000_000;
const ENGINE_TIMEOUT_MS_RANGE: RangeInclusive<u64> = 50..=10_000;
const MAX_CALLS_RANGE: RangeInclusive<u64> = 1..=100;
const HTTP_TIMEOUT_MS_RANGE: RangeInclusive<u64> = 100..=30_000;
const MAX_RESPONSE_KIB_RANGE: RangeInclusive<u64> = 1..=10_240;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NumericSetting {
    OpLimit,
    EngineTimeoutMs,
    MaxCalls,
    HttpTimeoutMs,
    MaxResponseKib,
}

impl NumericSetting {
    const ALL: [NumericSetting; 5] = [
        NumericSetting::OpLimit,
        NumericSetting::EngineTimeoutMs,
        NumericSetting::MaxCalls,
        NumericSetting::HttpTimeoutMs,
        NumericSetting::MaxResponseKib,
    ];

    pub fn range(self) -> RangeInclusive<u64> {
        match self {
            NumericSetting::OpLimit => OP_LIMIT_RANGE,
            NumericSetting::EngineTimeoutMs => ENGINE_TIMEOUT_MS_RANGE,
            NumericSetting::MaxCalls => MAX_CALLS_RANGE,
            NumericSetting::HttpTimeoutMs => HTTP_TIMEOUT_MS_RANGE,
            NumericSetting::MaxResponseKib => MAX_RESPONSE_KIB_RANGE,
        }
    }

    pub fn declaration(self) -> &'static Setting<Count> {
        match self {
            NumericSetting::OpLimit => &forge_script::settings::OP_LIMIT,
            NumericSetting::EngineTimeoutMs => &forge_script::settings::TIMEOUT_MS,
            NumericSetting::MaxCalls => &forge_script::settings::HTTP_MAX_CALLS,
            NumericSetting::HttpTimeoutMs => &forge_script::settings::HTTP_TIMEOUT_MS,
            NumericSetting::MaxResponseKib => &forge_script::settings::HTTP_MAX_RESPONSE_BYTES,
        }
    }

    pub fn stored_value_from_text(self, text: &str) -> Option<u64> {
        let value = text
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|v| self.range().contains(v))?;
        Some(match self {
            NumericSetting::MaxResponseKib => value * BYTES_PER_KIB,
            _ => value,
        })
    }

    pub fn display_from_stored(self, stored: u64) -> u64 {
        match self {
            NumericSetting::MaxResponseKib => stored / BYTES_PER_KIB,
            _ => stored,
        }
    }

    fn label(self) -> String {
        match self {
            NumericSetting::OpLimit => tr!("settings_scripting_op_limit_label"),
            NumericSetting::EngineTimeoutMs => tr!("settings_scripting_engine_timeout_label"),
            NumericSetting::MaxCalls => tr!("settings_scripting_max_calls_label"),
            NumericSetting::HttpTimeoutMs => tr!("settings_scripting_http_timeout_label"),
            NumericSetting::MaxResponseKib => tr!("settings_scripting_max_response_label"),
        }
    }

    fn hint(self) -> String {
        match self {
            NumericSetting::OpLimit => tr!("settings_scripting_op_limit_hint"),
            NumericSetting::EngineTimeoutMs => tr!("settings_scripting_engine_timeout_hint"),
            NumericSetting::MaxCalls => tr!("settings_scripting_max_calls_hint"),
            NumericSetting::HttpTimeoutMs => tr!("settings_scripting_http_timeout_hint"),
            NumericSetting::MaxResponseKib => tr!("settings_scripting_max_response_hint"),
        }
    }

    fn range_error(self) -> String {
        let range = self.range();
        tr!(
            "settings_scripting_range_error",
            min = range.start().to_string(),
            max = range.end().to_string()
        )
    }
}

struct NumericField {
    setting: NumericSetting,
    input: Entity<TextInput>,
    debounce: Debounced,
    invalid: bool,
}

struct ScriptingSnapshot {
    allow_local: bool,
    core_allow_local: bool,
    stored_numbers: [(NumericSetting, u64); 5],
}

pub struct SettingsScriptingView {
    backend: Arc<dyn DataProvider>,
    rt_handle: tokio::runtime::Handle,
    allowed_domains: Loadable<Vec<String>>,
    domains_writer: SnapshotWriter,
    allow_local: bool,
    core_allow_local: bool,
    numeric: Vec<NumericField>,
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
        let defaults = snapshot_defaults();

        let mut subs = Vec::new();
        let mut numeric = Vec::with_capacity(NumericSetting::ALL.len());
        for (setting, stored) in defaults.stored_numbers {
            let input = cx.new(|cx| {
                let mut input = TextInput::new("", cx)
                    .with_palette(palette)
                    .with_font_size(FONT_SM);
                input.set_content(setting.display_from_stored(stored).to_string(), cx);
                input
            });
            subs.push(
                cx.subscribe(&input, move |this, _input, event: &InputEvent, cx| {
                    if let InputEvent::Changed(text) | InputEvent::Submitted(text) = event {
                        this.commit_numeric(setting, text.as_ref(), cx);
                    }
                }),
            );
            numeric.push(NumericField {
                setting,
                input,
                debounce: Debounced::new(SLIDER_PERSIST_DEBOUNCE),
                invalid: false,
            });
        }

        let domain_draft = cx.new(|cx| {
            TextInput::new(tr!("settings_scripting_domains_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
                .read_only(true)
        });
        subs.push(
            cx.subscribe(&domain_draft, |this, _input, event: &InputEvent, cx| {
                if let InputEvent::Submitted(_) = event {
                    this.add_domain(cx);
                }
            }),
        );

        let mut view = Self {
            backend,
            domains_writer: SnapshotWriter::new(&rt_handle),
            rt_handle,
            allowed_domains: Loadable::Loading,
            allow_local: defaults.allow_local,
            core_allow_local: defaults.core_allow_local,
            numeric,
            domain_draft,
            _subs: subs,
        };
        view.load(cx);
        view.load_domains(cx);
        view
    }

    fn settings_repo(&self) -> Arc<dyn SettingsRepo> {
        Arc::clone(&self.backend) as Arc<dyn SettingsRepo>
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        async_bridge::run_async(
            &self.rt_handle,
            load_scripting_settings(self.settings_repo()),
            |this, snapshot, cx| this.apply_loaded(snapshot, cx),
            cx,
        );
    }

    fn load_domains(&mut self, cx: &mut Context<Self>) {
        self.allowed_domains = Loadable::Loading;
        self.domain_draft
            .update(cx, |input, cx| input.set_read_only(true, cx));
        async_bridge::run_async(
            &self.rt_handle,
            load_allowed_domains(self.settings_repo()),
            |this, result, cx| this.apply_domains(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_domains(&mut self, result: Result<Vec<String>, StorageError>, cx: &mut Context<Self>) {
        if let Err(error) = &result {
            tracing::warn!(%error, "failed to load the script HTTP allowlist");
        }
        self.allowed_domains = Loadable::settled(result);
        let locked = !self.allowed_domains.is_loaded();
        self.domain_draft
            .update(cx, |input, cx| input.set_read_only(locked, cx));
        cx.notify();
    }

    fn apply_loaded(&mut self, snapshot: ScriptingSnapshot, cx: &mut Context<Self>) {
        self.allow_local = snapshot.allow_local;
        self.core_allow_local = snapshot.core_allow_local;
        for (setting, stored) in snapshot.stored_numbers {
            if let Some(field) = self.numeric.iter_mut().find(|f| f.setting == setting) {
                field.invalid = false;
                let text = setting.display_from_stored(stored).to_string();
                field.input.update(cx, |input, cx| {
                    input.set_content(text, cx);
                    input.set_invalid(false, cx);
                });
            }
        }
        cx.notify();
    }

    fn commit_numeric(&mut self, setting: NumericSetting, text: &str, cx: &mut Context<Self>) {
        let parsed = setting
            .stored_value_from_text(text)
            .and_then(|stored| u32::try_from(stored).ok());
        let repo = self.settings_repo();
        let handle = self.rt_handle.clone();
        let Some(field) = self.numeric.iter_mut().find(|f| f.setting == setting) else {
            return;
        };
        field.invalid = parsed.is_none();
        let invalid = field.invalid;
        field
            .input
            .update(cx, |input, cx| input.set_invalid(invalid, cx));
        if let Some(stored) = parsed {
            field.debounce.schedule_reporting(
                &handle,
                tr!("settings_scripting_persist_failed"),
                async move { write_setting(repo.as_ref(), setting.declaration(), &stored).await },
                ErrorSink::Toast,
                cx,
            );
        }
        cx.notify();
    }

    fn persist_domains(&self, domains: Vec<String>, cx: &mut Context<Self>) {
        let repo = self.settings_repo();
        self.domains_writer.write(
            async move {
                write_setting(
                    repo.as_ref(),
                    &forge_script::settings::HTTP_ALLOWED_DOMAINS,
                    &domains,
                )
                .await
            },
            Self::domains_write_settled,
            cx,
        );
    }

    fn domains_write_settled(&mut self, outcome: WriteOutcome, cx: &mut Context<Self>) {
        if let WriteOutcome::Failed(error) = outcome {
            cx.push_toast(
                ToastKind::Error,
                tr!(
                    "settings_scripting_domains_save_failed",
                    reason = error.as_str()
                ),
            );
            self.load_domains(cx);
        }
    }

    fn persist_flag(&self, setting: &'static Setting<Flag>, value: bool, cx: &mut Context<Self>) {
        let repo = self.settings_repo();
        async_bridge::report_failure(
            &self.rt_handle,
            async move { write_setting(repo.as_ref(), setting, &value).await },
            ErrorSink::Toast,
            tr!("settings_scripting_persist_failed"),
            cx,
        );
    }

    fn add_domain(&mut self, cx: &mut Context<Self>) {
        let draft = self.domain_draft.read(cx).content().trim().to_owned();
        let Some(domains) = self.allowed_domains.loaded_mut() else {
            return;
        };
        if !draft.is_empty() && !domains.contains(&draft) {
            domains.push(draft);
            let snapshot = domains.clone();
            self.domain_draft.update(cx, |i, cx| i.clear(cx));
            self.persist_domains(snapshot, cx);
        }
        cx.notify();
    }

    fn remove_domain(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(domains) = self.allowed_domains.loaded_mut() else {
            return;
        };
        if index < domains.len() {
            domains.remove(index);
            let snapshot = domains.clone();
            self.persist_domains(snapshot, cx);
        }
        cx.notify();
    }

    fn toggle_allow_local(&mut self, cx: &mut Context<Self>) {
        self.allow_local = !self.allow_local;
        self.persist_flag(
            &forge_script::settings::HTTP_ALLOW_LOCAL,
            self.allow_local,
            cx,
        );
        cx.notify();
    }

    fn toggle_core_allow_local(&mut self, cx: &mut Context<Self>) {
        self.core_allow_local = !self.core_allow_local;
        self.persist_flag(
            &forge_runtime::settings::CORE_HTTP_ALLOW_LOCAL,
            self.core_allow_local,
            cx,
        );
        cx.notify();
    }

    fn numeric_row(
        &self,
        setting: NumericSetting,
        palette: &ForgePalette,
        density: Density,
    ) -> Option<SettingRow> {
        let field = self.numeric.iter().find(|f| f.setting == setting)?;
        let hint = if field.invalid {
            setting.range_error()
        } else {
            setting.hint()
        };
        let row = setting_row(
            setting.label(),
            Some(SharedString::from(hint)),
            div().w(NUMERIC_INPUT_WIDTH).child(field.input.clone()),
            palette,
            density,
        );
        Some(if field.invalid {
            row.hint_color(palette.danger)
        } else {
            row
        })
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
        for (idx, domain) in self
            .allowed_domains
            .loaded()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .enumerate()
        {
            chips = chips.child(self.domain_chip(idx, domain, palette, cx));
        }
        let failure = self.allowed_domains.failure().map(|reason| {
            error_row_with_retry(
                "settings-scripting-domains-retry",
                tr!("settings_scripting_domains_load_failed", reason = reason),
                palette,
                cx.listener(|this, _: &ClickEvent, _, cx| this.load_domains(cx)),
                density,
            )
        });

        let add_btn = icon_button(Icon::Plus, palette).on_click(
            "settings-scripting-domain-add",
            cx.listener(|this, _: &ClickEvent, _, cx| this.add_domain(cx)),
        );

        let input_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.domain_draft.clone()),
            )
            .child(add_btn)
            .when(!self.allowed_domains.is_loaded(), |row| {
                row.opacity(DISABLED_OPACITY)
            });

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .children(failure)
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
        let density = cx.density();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xxs, density))
            .rounded(radius(Radius::Pill))
            .bg(palette.surface_overlay)
            .child(
                div()
                    .font_family(body_family())
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
                    .child(icon(Icon::X, REMOVE_GLYPH, palette.text_muted)),
            )
    }

    fn engine_block(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let rows = setting_rows(palette)
            .rows(
                [NumericSetting::OpLimit, NumericSetting::EngineTimeoutMs]
                    .into_iter()
                    .filter_map(|s| self.numeric_row(s, palette, density))
                    .map(IntoElement::into_any_element),
            )
            .in_card(palette);
        setting_block(
            tr!("settings_scripting_engine_section"),
            None,
            rows,
            palette,
            density,
        )
        .into_any_element()
    }

    fn script_http_block(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = setting_rows(palette)
            .row(
                setting_row(
                    tr!("settings_scripting_allowed_domains_label"),
                    Some(tr!("settings_scripting_allowed_domains_hint").into()),
                    self.domain_list(palette, density, cx),
                    palette,
                    density,
                )
                .stacked(),
            )
            .rows(
                [
                    NumericSetting::MaxCalls,
                    NumericSetting::HttpTimeoutMs,
                    NumericSetting::MaxResponseKib,
                ]
                .into_iter()
                .filter_map(|s| self.numeric_row(s, palette, density))
                .map(IntoElement::into_any_element),
            )
            .row(setting_row(
                tr!("settings_scripting_allow_local_label"),
                Some(tr!("settings_scripting_allow_local_description").into()),
                toggle(self.allow_local, palette).on_click(
                    "settings-scripting-allow-local",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_allow_local(cx)),
                ),
                palette,
                density,
            ))
            .in_card(palette);
        setting_block(
            tr!("settings_scripting_http_section"),
            None,
            with_ssrf_warning(rows, self.allow_local, palette, density),
            palette,
            density,
        )
        .into_any_element()
    }

    fn http_action_block(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = setting_rows(palette)
            .row(setting_row(
                tr!("settings_scripting_core_allow_local_label"),
                Some(tr!("settings_scripting_core_allow_local_description").into()),
                toggle(self.core_allow_local, palette).on_click(
                    "settings-scripting-core-allow-local",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_core_allow_local(cx)),
                ),
                palette,
                density,
            ))
            .in_card(palette);
        setting_block(
            tr!("settings_scripting_http_action_section"),
            None,
            with_ssrf_warning(rows, self.core_allow_local, palette, density),
            palette,
            density,
        )
        .into_any_element()
    }
}

impl Render for SettingsScriptingView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        div()
            .flex()
            .flex_col()
            .child(SettingsPaneHeader::new(SettingsSection::Scripting))
            .child(setting_groups(
                [
                    self.engine_block(&palette, density),
                    self.script_http_block(&palette, density, cx),
                    self.http_action_block(&palette, density, cx),
                ],
                density,
            ))
    }
}

fn with_ssrf_warning(
    rows: impl IntoElement,
    allow_local: bool,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let mut content = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(rows);
    if allow_local {
        content = content.child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.danger)
                .child(tr!("settings_scripting_ssrf_warning")),
        );
    }
    content
}

fn snapshot_from(
    http: ScriptHttpConfig,
    engine: EngineConfig,
    core_allow_local: bool,
) -> ScriptingSnapshot {
    ScriptingSnapshot {
        stored_numbers: [
            (NumericSetting::OpLimit, engine.op_limit),
            (NumericSetting::EngineTimeoutMs, engine.wall_time_ms),
            (
                NumericSetting::MaxCalls,
                u64::from(http.max_calls_per_script),
            ),
            (NumericSetting::HttpTimeoutMs, u64::from(http.timeout_ms)),
            (
                NumericSetting::MaxResponseKib,
                u64::from(http.max_response_bytes),
            ),
        ],
        allow_local: http.allow_local,
        core_allow_local,
    }
}

fn snapshot_defaults() -> ScriptingSnapshot {
    snapshot_from(ScriptHttpConfig::default(), EngineConfig::default(), false)
}

async fn load_scripting_settings(repo: Arc<dyn SettingsRepo>) -> ScriptingSnapshot {
    let http = ScriptHttpConfig::read(repo.as_ref()).await;
    let engine = EngineConfig::read(repo.as_ref()).await;
    let core_allow_local = read_setting_or_default(
        repo.as_ref(),
        &forge_runtime::settings::CORE_HTTP_ALLOW_LOCAL,
    )
    .await;
    snapshot_from(http, engine, core_allow_local)
}

async fn load_allowed_domains(repo: Arc<dyn SettingsRepo>) -> Result<Vec<String>, StorageError> {
    stored_script_http_allowed_domains(repo.as_ref()).await
}

#[cfg(test)]
mod tests {
    use forge_storage::reserved_keys;
    use gpui::TestAppContext;

    use super::*;
    use tokio::sync::mpsc::UnboundedReceiver;

    use crate::test_support::{
        ReadFault, SETTINGS_READ_FAILED, SettingWrite, TestBackend, install_presentation, pump,
        test_backend, writes_to,
    };

    #[test]
    fn stored_value_from_text_accepts_inclusive_range_bounds() {
        for (setting, text, stored) in [
            (NumericSetting::OpLimit, "1000", 1_000),
            (NumericSetting::OpLimit, "10000000", 10_000_000),
            (NumericSetting::EngineTimeoutMs, "50", 50),
            (NumericSetting::EngineTimeoutMs, "10000", 10_000),
            (NumericSetting::MaxCalls, "1", 1),
            (NumericSetting::MaxCalls, "100", 100),
            (NumericSetting::HttpTimeoutMs, "100", 100),
            (NumericSetting::HttpTimeoutMs, "30000", 30_000),
            (NumericSetting::MaxResponseKib, "1", 1_024),
            (NumericSetting::MaxResponseKib, "10240", 10_485_760),
        ] {
            assert_eq!(
                setting.stored_value_from_text(text),
                Some(stored),
                "{setting:?} {text:?}"
            );
        }
    }

    #[test]
    fn stored_value_from_text_rejects_one_past_either_bound() {
        for setting in NumericSetting::ALL {
            let range = setting.range();
            for outside in [range.start() - 1, range.end() + 1] {
                assert_eq!(
                    setting.stored_value_from_text(&outside.to_string()),
                    None,
                    "{setting:?} {outside}"
                );
            }
        }
    }

    #[test]
    fn stored_value_from_text_rejects_text_that_is_not_an_unsigned_integer() {
        for setting in NumericSetting::ALL {
            for text in [
                "",
                "   ",
                "abc",
                "-5",
                "12.5",
                "1e3",
                "1 000",
                "18446744073709551616",
            ] {
                assert_eq!(
                    setting.stored_value_from_text(text),
                    None,
                    "{setting:?} {text:?}"
                );
            }
        }
    }

    #[test]
    fn stored_value_from_text_ignores_surrounding_whitespace() {
        for (setting, text, stored) in [
            (NumericSetting::MaxCalls, " 42 \t", 42),
            (NumericSetting::MaxResponseKib, "\n2048 ", 2_097_152),
        ] {
            assert_eq!(setting.stored_value_from_text(text), Some(stored));
        }
    }

    #[test]
    fn display_from_stored_converts_bytes_to_kib_only_for_max_response() {
        for (setting, stored, shown) in [
            (NumericSetting::MaxResponseKib, 2_097_152, 2_048),
            (NumericSetting::OpLimit, 250_000, 250_000),
            (NumericSetting::EngineTimeoutMs, 2_048, 2_048),
            (NumericSetting::MaxCalls, 2_048, 2_048),
            (NumericSetting::HttpTimeoutMs, 2_048, 2_048),
        ] {
            assert_eq!(setting.display_from_stored(stored), shown, "{setting:?}");
        }
    }

    #[test]
    fn typed_value_reads_back_unchanged_after_storage_conversion() {
        for setting in NumericSetting::ALL {
            let range = setting.range();
            let middle = (range.start() + range.end()) / 2;
            for typed in [*range.start(), middle, *range.end()] {
                let stored = setting
                    .stored_value_from_text(&typed.to_string())
                    .expect("in-range value");
                assert_eq!(
                    setting.display_from_stored(stored),
                    typed,
                    "{setting:?} {typed}"
                );
            }
        }
    }

    #[test]
    fn default_values_display_as_valid_entries() {
        for (setting, stored) in snapshot_defaults().stored_numbers {
            let shown = setting.display_from_stored(stored).to_string();
            assert_eq!(
                setting.stored_value_from_text(&shown),
                Some(stored),
                "{setting:?} default {stored} shows {shown:?}"
            );
        }
    }

    #[tokio::test]
    async fn every_persisted_bound_loads_back_into_its_own_field() {
        for at_upper_bound in [false, true] {
            let (backend, _writes) = test_backend();
            let mut typed_by_setting = Vec::new();
            for (offset, setting) in (0u64..).zip(NumericSetting::ALL) {
                let range = setting.range();
                let typed = if at_upper_bound {
                    range.end() - offset
                } else {
                    range.start() + offset
                };
                let stored = setting
                    .stored_value_from_text(&typed.to_string())
                    .expect("in-range value");
                backend
                    .set_string(setting.declaration().name(), &stored.to_string())
                    .await
                    .unwrap();
                typed_by_setting.push((setting, typed));
            }

            let snapshot = load_scripting_settings(backend as Arc<dyn SettingsRepo>).await;

            for (setting, typed) in typed_by_setting {
                let (_, stored) = snapshot
                    .stored_numbers
                    .iter()
                    .find(|(s, _)| *s == setting)
                    .copied()
                    .expect("setting present in snapshot");
                assert_eq!(setting.display_from_stored(stored), typed, "{setting:?}");
            }
        }
    }

    #[tokio::test]
    async fn loaded_snapshot_carries_the_core_allow_local_switch() {
        let (backend, _writes) = test_backend();
        backend
            .set_string(reserved_keys::CORE_HTTP_ALLOW_LOCAL, "true")
            .await
            .unwrap();

        let snapshot = load_scripting_settings(backend as Arc<dyn SettingsRepo>).await;

        assert!(snapshot.core_allow_local);
    }

    fn paused_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .expect("paused current-thread runtime")
    }

    fn mounted_view(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        backend: Arc<TestBackend>,
    ) -> Entity<SettingsScriptingView> {
        install_presentation(cx);
        let handle = rt.handle().clone();
        let view =
            cx.new(|cx| SettingsScriptingView::new(backend as Arc<dyn DataProvider>, handle, cx));
        pump(rt);
        cx.run_until_parked();
        view
    }

    fn commit(
        cx: &mut TestAppContext,
        view: &Entity<SettingsScriptingView>,
        setting: NumericSetting,
        text: &str,
    ) {
        view.update(cx, |this, cx| this.commit_numeric(setting, text, cx));
    }

    fn outlast_debounce(rt: &tokio::runtime::Runtime) {
        rt.block_on(async { tokio::time::sleep(SLIDER_PERSIST_DEBOUNCE * 2).await });
    }

    fn is_invalid(
        cx: &mut TestAppContext,
        view: &Entity<SettingsScriptingView>,
        setting: NumericSetting,
    ) -> bool {
        view.read_with(cx, |this, _| {
            this.numeric
                .iter()
                .find(|f| f.setting == setting)
                .expect("field for setting")
                .invalid
        })
    }

    #[gpui::test]
    fn out_of_range_entry_flags_the_field_and_persists_nothing(cx: &mut TestAppContext) {
        let rt = paused_runtime();
        let (backend, mut writes) = test_backend();
        let view = mounted_view(cx, &rt, backend);

        commit(cx, &view, NumericSetting::MaxCalls, "101");
        outlast_debounce(&rt);

        assert!(is_invalid(cx, &view, NumericSetting::MaxCalls));
        assert!(writes.try_recv().is_err());
    }

    #[gpui::test]
    fn valid_entry_persists_the_stored_value_under_its_key(cx: &mut TestAppContext) {
        let rt = paused_runtime();
        let (backend, mut writes) = test_backend();
        let view = mounted_view(cx, &rt, backend);

        commit(cx, &view, NumericSetting::MaxResponseKib, "2048");
        outlast_debounce(&rt);

        assert_eq!(
            writes.try_recv().expect("debounced write"),
            (
                reserved_keys::SCRIPT_HTTP_MAX_RESPONSE_BYTES.to_owned(),
                "2097152".to_owned()
            )
        );
    }

    #[gpui::test]
    fn valid_entry_after_an_invalid_one_clears_the_flag(cx: &mut TestAppContext) {
        let rt = paused_runtime();
        let (backend, _writes) = test_backend();
        let view = mounted_view(cx, &rt, backend);

        commit(cx, &view, NumericSetting::HttpTimeoutMs, "99");
        commit(cx, &view, NumericSetting::HttpTimeoutMs, "100");

        assert!(!is_invalid(cx, &view, NumericSetting::HttpTimeoutMs));
    }

    const STORED_DOMAIN: &str = "keep.example.com";
    const TYPED_DOMAIN: &str = "new.example.com";

    type DomainEdit = fn(&mut SettingsScriptingView, &mut Context<SettingsScriptingView>);

    fn domain_edits() -> [(&'static str, DomainEdit); 2] {
        [
            ("add a domain", |v, cx| {
                v.domain_draft.update(cx, |input, cx| {
                    input.set_content(TYPED_DOMAIN.to_owned(), cx)
                });
                v.add_domain(cx);
            }),
            ("remove a domain", |v, cx| v.remove_domain(0, cx)),
        ]
    }

    fn domains_rig(
        cx: &mut TestAppContext,
    ) -> (
        tokio::runtime::Runtime,
        Arc<TestBackend>,
        UnboundedReceiver<SettingWrite>,
    ) {
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let rt = paused_runtime();
        let (backend, writes) = test_backend();
        backend.seed(
            reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS,
            &serde_json::to_string(&[STORED_DOMAIN]).unwrap(),
        );
        (rt, backend, writes)
    }

    fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        for _ in 0..3 {
            pump(rt);
            cx.run_until_parked();
        }
    }

    fn domain_writes(writes: &mut UnboundedReceiver<SettingWrite>) -> Vec<Vec<String>> {
        writes_to(writes, reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS)
            .iter()
            .map(|raw| serde_json::from_str(raw).unwrap())
            .collect()
    }

    fn shown_domains(
        cx: &mut TestAppContext,
        view: &Entity<SettingsScriptingView>,
    ) -> Option<Vec<String>> {
        view.read_with(cx, |this, _| this.allowed_domains.loaded().cloned())
    }

    #[gpui::test]
    fn no_domain_edit_writes_the_allowlist_until_it_loads(cx: &mut TestAppContext) {
        let unreachable = StorageError::Connection {
            reason: SETTINGS_READ_FAILED.to_owned(),
        }
        .to_string();
        for (state, fault, reason) in [
            ("failed", ReadFault::Fails, Some(unreachable)),
            (
                "pending",
                ReadFault::Held(Arc::new(tokio::sync::Notify::new())),
                None,
            ),
        ] {
            for (case, edit) in domain_edits() {
                let (rt, backend, mut writes) = domains_rig(cx);
                backend.fault_reads(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS, fault.clone());
                let view = mounted_view(cx, &rt, backend);
                settle(cx, &rt);

                view.update(cx, edit);
                settle(cx, &rt);

                let failure = view.read_with(cx, |this, _| {
                    this.allowed_domains.failure().map(str::to_owned)
                });
                assert_eq!(
                    (domain_writes(&mut writes), failure),
                    (Vec::<Vec<String>>::new(), reason.clone()),
                    "{state}: {case}"
                );
            }
        }
    }

    #[gpui::test]
    fn retrying_a_failed_allowlist_load_lets_domain_edits_persist(cx: &mut TestAppContext) {
        for (case, edit, expected) in [
            (
                domain_edits()[0].0,
                domain_edits()[0].1,
                vec![STORED_DOMAIN.to_owned(), TYPED_DOMAIN.to_owned()],
            ),
            (domain_edits()[1].0, domain_edits()[1].1, Vec::new()),
        ] {
            let (rt, backend, mut writes) = domains_rig(cx);
            backend.fault_reads(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS, ReadFault::Fails);
            let view = mounted_view(cx, &rt, Arc::clone(&backend));
            settle(cx, &rt);
            backend.heal_reads(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS);

            view.update(cx, |this, cx| this.load_domains(cx));
            settle(cx, &rt);
            view.update(cx, edit);
            settle(cx, &rt);

            assert_eq!(domain_writes(&mut writes), vec![expected], "{case}");
        }
    }

    #[gpui::test]
    fn a_failed_allowlist_write_toasts_and_reloads_the_stored_domains(cx: &mut TestAppContext) {
        for (case, edit) in domain_edits() {
            let (rt, backend, _writes) = domains_rig(cx);
            backend.fail_writes(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS);
            let view = mounted_view(cx, &rt, backend);
            settle(cx, &rt);

            view.update(cx, edit);
            settle(cx, &rt);

            assert_eq!(
                (
                    cx.update(|cx| crate::test_support::error_toasts(cx)),
                    shown_domains(cx, &view)
                ),
                (1, Some(vec![STORED_DOMAIN.to_owned()])),
                "{case}"
            );
        }
    }
}
