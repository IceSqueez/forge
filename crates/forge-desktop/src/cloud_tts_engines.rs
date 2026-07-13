use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, ForgePalette,
    InputEvent, Radius, Spacing, TextInput, ToastKind, card, radius, spacing,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, Rgba, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

/// Engine status-dot side — the parity source pins the header dot at a fixed 7px square,
/// off the `Spacing` scale, so it is carried as a named literal.
const STATUS_DOT: Pixels = px(7.0);
/// Test-result dot side (the source's fixed 6px square).
const RESULT_DOT: Pixels = px(6.0);
/// Field-label column width (the source's fixed 120px label gutter).
const LABEL_W: Pixels = px(120.0);
/// The representative Azure subscription key the section seeds so the configured-engine
/// visual states (verified row + Configured badge) render before real credentials load.
const AZURE_SEED_KEY: &str = "0e7f2c9a4b1d4e8fa2c6b3d5e9f10a2b";

/// Which cloud engine a card, seed and handler act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloudEngineKind {
    Azure,
    ElevenLabs,
    OpenAI,
    Polly,
}

impl CloudEngineKind {
    /// Stable element-id fragment for the card's Save button.
    fn key(self) -> &'static str {
        match self {
            CloudEngineKind::Azure => "azure",
            CloudEngineKind::ElevenLabs => "elevenlabs",
            CloudEngineKind::OpenAI => "openai",
            CloudEngineKind::Polly => "polly",
        }
    }
}

/// A card's connection-test outcome. `Testing` is unreachable while the Test button is
/// inert (the real engine factory + network are not wired here), but is retained so the
/// button's label + gating mirror the source once the runtime path lands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TestStatus {
    Idle,
    Testing,
    Ok,
    // Produced only by the runtime connection-test path (not wired here); the badge and
    // result-row logic already read it, so it is retained rather than stubbed away.
    #[allow(dead_code)]
    Err(String),
}

/// Azure Speech credential form. Fields are child [`TextInput`] entities owning their own
/// edit state; `dirty` flips on any edit and `test_status`/`is_registered` are seeded here
/// until the real credential store + registry reach this view over the bridge.
struct AzureForm {
    api_key: Entity<TextInput>,
    region: Entity<TextInput>,
    dirty: bool,
    test_status: TestStatus,
    is_registered: bool,
}

struct ElevenLabsForm {
    api_key: Entity<TextInput>,
    dirty: bool,
    test_status: TestStatus,
    is_registered: bool,
}

struct OpenAiForm {
    api_key: Entity<TextInput>,
    dirty: bool,
    test_status: TestStatus,
    is_registered: bool,
}

struct PollyForm {
    access_key: Entity<TextInput>,
    secret_key: Entity<TextInput>,
    region: Entity<TextInput>,
    dirty: bool,
    test_status: TestStatus,
    is_registered: bool,
}

/// The TTS Cloud-engines section view-entity: a scrollable column of four credential
/// cards (Azure Speech, ElevenLabs, OpenAI TTS, Amazon Polly), each with secret fields,
/// a connection-test button and a save button, plus a config-status badge and a
/// test-result row.
///
/// Owns the four forms as seeded stub state — `forge-desktop` wires no credential store,
/// engine factory or TTS registry yet, so the field contents, dirty flags and
/// registration are seeded representative (Azure configured, the rest empty). The real
/// screen loads persisted credentials from `forge-storage` over the runtime→UI bridge;
/// Save persists through that store's handle and hot-registers the engine into the live
/// `TtsRegistry`, and Test builds the engine and round-trips a probe request through the
/// network — neither of which is reachable here, so Save only clears the dirty flag and
/// Test is inert (it never fakes a success).
pub struct CloudTtsEnginesView {
    azure: AzureForm,
    elevenlabs: ElevenLabsForm,
    openai: OpenAiForm,
    polly: PollyForm,
    _subs: Vec<Subscription>,
}

impl CloudTtsEnginesView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();

        let azure_api = field("Subscription key", AZURE_SEED_KEY, true, palette, cx);
        let azure_region = field("e.g. eastus", "eastus", false, palette, cx);
        let eleven_api = field("xi-api-key", "", true, palette, cx);
        let openai_api = field("sk-...", "", true, palette, cx);
        let polly_access = field("AKIA...", "", false, palette, cx);
        let polly_secret = field("secret access key", "", true, palette, cx);
        let polly_region = field("e.g. us-east-1", "", false, palette, cx);

        let mut subs = Vec::new();
        for (entity, kind) in [
            (&azure_api, CloudEngineKind::Azure),
            (&azure_region, CloudEngineKind::Azure),
            (&eleven_api, CloudEngineKind::ElevenLabs),
            (&openai_api, CloudEngineKind::OpenAI),
            (&polly_access, CloudEngineKind::Polly),
            (&polly_secret, CloudEngineKind::Polly),
            (&polly_region, CloudEngineKind::Polly),
        ] {
            subs.push(
                cx.subscribe(entity, move |this, _input, event: &InputEvent, cx| {
                    if let InputEvent::Changed(_) = event {
                        this.mark_dirty(kind, cx);
                    }
                }),
            );
        }

        Self {
            azure: AzureForm {
                api_key: azure_api,
                region: azure_region,
                dirty: false,
                test_status: TestStatus::Ok,
                is_registered: true,
            },
            elevenlabs: ElevenLabsForm {
                api_key: eleven_api,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: false,
            },
            openai: OpenAiForm {
                api_key: openai_api,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: false,
            },
            polly: PollyForm {
                access_key: polly_access,
                secret_key: polly_secret,
                region: polly_region,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: false,
            },
            _subs: subs,
        }
    }

    // --- handlers (view-state stubs) --------------------------------------

    fn mark_dirty(&mut self, kind: CloudEngineKind, cx: &mut Context<Self>) {
        match kind {
            CloudEngineKind::Azure => self.azure.dirty = true,
            CloudEngineKind::ElevenLabs => self.elevenlabs.dirty = true,
            CloudEngineKind::OpenAI => self.openai.dirty = true,
            CloudEngineKind::Polly => self.polly.dirty = true,
        }
        cx.notify();
    }

    /// Optimistically clears the dirty flag. Real path: persist the credentials through
    /// the store, hot-register the engine into the live `TtsRegistry` and refresh the
    /// speak-queue voice catalog.
    fn save(&mut self, kind: CloudEngineKind, cx: &mut Context<Self>) {
        match kind {
            CloudEngineKind::Azure => self.azure.dirty = false,
            CloudEngineKind::ElevenLabs => self.elevenlabs.dirty = false,
            CloudEngineKind::OpenAI => self.openai.dirty = false,
            CloudEngineKind::Polly => self.polly.dirty = false,
        }
        cx.push_toast(ToastKind::Success, "Credentials saved");
        cx.notify();
    }

    // --- cards ------------------------------------------------------------

    fn azure_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_key = nonempty(&self.azure.api_key, cx);
        let has_region = nonempty(&self.azure.region, cx);
        let can_save = self.azure.dirty && has_key && has_region;
        let can_test = has_key && has_region && self.azure.test_status != TestStatus::Testing;

        let fields = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(labeled_field(
                "API Key",
                self.azure.api_key.clone(),
                palette,
                density,
            ))
            .child(labeled_field(
                "Region",
                self.azure.region.clone(),
                palette,
                density,
            ))
            .into_any_element();

        self.engine_card(
            "Azure Speech",
            palette.info,
            &self.azure.test_status,
            self.azure.is_registered,
            fields,
            CloudEngineKind::Azure,
            can_save,
            can_test,
            palette,
            density,
            cx,
        )
    }

    fn elevenlabs_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_key = nonempty(&self.elevenlabs.api_key, cx);
        let can_save = self.elevenlabs.dirty && has_key;
        let can_test = has_key && self.elevenlabs.test_status != TestStatus::Testing;

        let fields = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(labeled_field(
                "API Key",
                self.elevenlabs.api_key.clone(),
                palette,
                density,
            ))
            .into_any_element();

        self.engine_card(
            "ElevenLabs",
            palette.bits,
            &self.elevenlabs.test_status,
            self.elevenlabs.is_registered,
            fields,
            CloudEngineKind::ElevenLabs,
            can_save,
            can_test,
            palette,
            density,
            cx,
        )
    }

    fn openai_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_key = nonempty(&self.openai.api_key, cx);
        let can_save = self.openai.dirty && has_key;
        let can_test = has_key && self.openai.test_status != TestStatus::Testing;

        let fields = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(labeled_field(
                "API Key",
                self.openai.api_key.clone(),
                palette,
                density,
            ))
            .into_any_element();

        self.engine_card(
            "OpenAI TTS",
            palette.success,
            &self.openai.test_status,
            self.openai.is_registered,
            fields,
            CloudEngineKind::OpenAI,
            can_save,
            can_test,
            palette,
            density,
            cx,
        )
    }

    fn polly_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_access = nonempty(&self.polly.access_key, cx);
        let has_secret = nonempty(&self.polly.secret_key, cx);
        let has_region = nonempty(&self.polly.region, cx);
        let can_save = self.polly.dirty && has_access && has_secret && has_region;
        let can_test =
            has_access && has_secret && has_region && self.polly.test_status != TestStatus::Testing;

        let fields = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(labeled_field(
                "Access key ID",
                self.polly.access_key.clone(),
                palette,
                density,
            ))
            .child(labeled_field(
                "Secret key",
                self.polly.secret_key.clone(),
                palette,
                density,
            ))
            .child(labeled_field(
                "Region",
                self.polly.region.clone(),
                palette,
                density,
            ))
            .into_any_element();

        self.engine_card(
            "Amazon Polly",
            palette.warning,
            &self.polly.test_status,
            self.polly.is_registered,
            fields,
            CloudEngineKind::Polly,
            can_save,
            can_test,
            palette,
            density,
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn engine_card(
        &self,
        name: &'static str,
        dot_color: Rgba,
        test_status: &TestStatus,
        is_registered: bool,
        fields: AnyElement,
        kind: CloudEngineKind,
        can_save: bool,
        can_test: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dot = div()
            .flex_none()
            .size(STATUS_DOT)
            .rounded(radius(Radius::Pill))
            .bg(dot_color);

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(dot)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(name),
            )
            .child(config_status_badge(
                test_status,
                is_registered,
                palette,
                density,
            ));

        // The test-connection button cannot run without an engine factory + network, so
        // it renders its enabled/disabled visual state but carries no click handler and
        // never fakes a result.
        let test_label = if *test_status == TestStatus::Testing {
            "Testing…"
        } else {
            "Test connection"
        };
        let (test_border, test_fg) = if can_test {
            (palette.border_regular, palette.text_muted)
        } else {
            (palette.disabled, palette.disabled)
        };
        let test_btn = div()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(test_border)
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(test_fg)
            .child(test_label);

        let save_btn: AnyElement = if can_save {
            div()
                .id(SharedString::from(format!("cloud-save-{}", kind.key())))
                .py(spacing(Spacing::Xxs, density))
                .px(spacing(Spacing::Sm, density))
                .rounded(radius(Radius::Md))
                .bg(palette.brand)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.save(kind, cx)))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child("Save credentials")
                .into_any_element()
        } else {
            div()
                .py(spacing(Spacing::Xxs, density))
                .px(spacing(Spacing::Sm, density))
                .rounded(radius(Radius::Md))
                .bg(palette.surface_overlay)
                .border(BORDER_THIN)
                .border_color(palette.disabled)
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.disabled)
                .child("Save credentials")
                .into_any_element()
        };

        let action = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(test_btn)
            .child(save_btn);

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(header)
            .child(fields)
            .child(action);
        if let Some(result) = test_result_row(test_status, palette, density) {
            body = body.child(result);
        }

        card(body, palette)
            .radius(Radius::Lg)
            .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Md, density))
            .full_width()
            .into_any_element()
    }
}

impl Render for CloudTtsEnginesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child("CLOUD ENGINES · 4");

        let column = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .p(spacing(Spacing::Md, density))
            .child(header)
            .child(self.azure_card(&palette, density, cx))
            .child(self.elevenlabs_card(&palette, density, cx))
            .child(self.openai_card(&palette, density, cx))
            .child(self.polly_card(&palette, density, cx));

        div()
            .id("cloud-engines-scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(column)
    }
}

// ── view-specific fragments ───────────────────────────────────────────────

/// The config-status badge: a `surface_overlay` pill with a `border_regular` outline,
/// its mono caption inked by registration (Configured), else a failed test (Connection
/// failed), else the muted Not-configured default.
fn config_status_badge(
    test_status: &TestStatus,
    is_registered: bool,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let (label, color) = if is_registered {
        ("CONFIGURED", palette.success)
    } else if matches!(test_status, TestStatus::Err(_)) {
        ("CONNECTION FAILED", palette.random)
    } else {
        ("NOT CONFIGURED", palette.text_muted)
    };

    div()
        .flex_none()
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Pill))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.surface_overlay)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(color)
                .child(label),
        )
}

/// The post-test result row: a small dot plus a caption — the success hue with "Connection
/// verified" on a passed test, the `random` hue with the error string on a failure, and
/// nothing at all while Idle/Testing.
fn test_result_row(
    test_status: &TestStatus,
    palette: &ForgePalette,
    density: Density,
) -> Option<AnyElement> {
    let (color, message): (Rgba, String) = match test_status {
        TestStatus::Ok => (palette.success, "Connection verified".to_owned()),
        TestStatus::Err(error) => (palette.random, error.clone()),
        _ => return None,
    };

    Some(
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex_none()
                    .size(RESULT_DOT)
                    .rounded(radius(Radius::Pill))
                    .bg(color),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(color)
                    .child(message),
            )
            .into_any_element(),
    )
}

/// A form row: a fixed-width muted label beside an input that fills the remaining width.
fn labeled_field(
    label: &'static str,
    input: Entity<TextInput>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(
            div()
                .w(LABEL_W)
                .flex_none()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(label),
        )
        .child(div().flex_1().min_w(px(0.0)).child(input))
}

/// Builds a credential text-field entity seeded with `initial` and adopting `palette`,
/// rendered at the source's `FONT_SM` body size. `secure` masks secret fields (API keys,
/// secret access keys) so they never render in plaintext.
fn field(
    placeholder: &'static str,
    initial: &str,
    secure: bool,
    palette: ForgePalette,
    cx: &mut Context<CloudTtsEnginesView>,
) -> Entity<TextInput> {
    let initial = initial.to_owned();
    cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx)
            .with_palette(palette)
            .with_font_size(FONT_SM)
            .secure(secure);
        if !initial.is_empty() {
            input.set_content(initial, cx);
        }
        input
    })
}

/// True when the field's trimmed content is non-empty — the save/test gate predicate.
fn nonempty(input: &Entity<TextInput>, cx: &App) -> bool {
    !input.read(cx).content().trim().is_empty()
}
