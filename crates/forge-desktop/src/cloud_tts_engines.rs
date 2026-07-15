use std::sync::{Arc, RwLock};

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, ForgePalette,
    InputEvent, Radius, Spacing, TextInput, ToastKind, card, radius, spacing,
};
use forge_speak_queue::{SpeakCommand, SpeakQueueHandle};
use forge_storage::{CredentialId, CredentialsRepo};
use forge_tts_cloud::azure::AzureEngineFactory;
use forge_tts_cloud::credentials::{
    AZURE_CREDENTIAL_ID, AzureCredentials, ELEVENLABS_CREDENTIAL_ID, ElevenLabsCredentials,
    OPENAI_CREDENTIAL_ID, OpenAiCredentials, POLLY_CREDENTIAL_ID, PollyCredentials,
};
use forge_tts_cloud::elevenlabs::ElevenLabsEngineFactory;
use forge_tts_cloud::openai::OpenAiEngineFactory;
use forge_tts_cloud::polly::PollyEngineFactory;
use forge_tts_core::{EngineId, TtsEngineFactory, TtsRegistry};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, Rgba, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use crate::cloud_tts_boot;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const STATUS_DOT: Pixels = px(7.0);
const RESULT_DOT: Pixels = px(6.0);
const LABEL_W: Pixels = px(120.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloudEngineKind {
    Azure,
    ElevenLabs,
    OpenAI,
    Polly,
}

impl CloudEngineKind {
    fn key(self) -> &'static str {
        match self {
            CloudEngineKind::Azure => "azure",
            CloudEngineKind::ElevenLabs => "elevenlabs",
            CloudEngineKind::OpenAI => "openai",
            CloudEngineKind::Polly => "polly",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            CloudEngineKind::Azure => "Azure Speech",
            CloudEngineKind::ElevenLabs => "ElevenLabs",
            CloudEngineKind::OpenAI => "OpenAI TTS",
            CloudEngineKind::Polly => "Amazon Polly",
        }
    }

    fn credential_id(self) -> &'static str {
        match self {
            CloudEngineKind::Azure => AZURE_CREDENTIAL_ID,
            CloudEngineKind::ElevenLabs => ELEVENLABS_CREDENTIAL_ID,
            CloudEngineKind::OpenAI => OPENAI_CREDENTIAL_ID,
            CloudEngineKind::Polly => POLLY_CREDENTIAL_ID,
        }
    }

    fn engine_id(self) -> EngineId {
        EngineId(self.key().to_owned())
    }
}

enum CloudCreds {
    Azure(AzureCredentials),
    ElevenLabs(ElevenLabsCredentials),
    OpenAi(OpenAiCredentials),
    Polly(PollyCredentials),
}

impl CloudCreds {
    fn to_json(&self) -> Result<String, serde_json::Error> {
        match self {
            CloudCreds::Azure(c) => serde_json::to_string(c),
            CloudCreds::ElevenLabs(c) => serde_json::to_string(c),
            CloudCreds::OpenAi(c) => serde_json::to_string(c),
            CloudCreds::Polly(c) => serde_json::to_string(c),
        }
    }

    async fn test(self) -> Result<(), String> {
        let engine = match self {
            CloudCreds::Azure(c) => AzureEngineFactory::new(c).create(),
            CloudCreds::ElevenLabs(c) => ElevenLabsEngineFactory::new(c).create(),
            CloudCreds::OpenAi(c) => OpenAiEngineFactory::new(c).create(),
            CloudCreds::Polly(c) => PollyEngineFactory::new(c).create(),
        }
        .map_err(|e| e.to_string())?;
        engine
            .test_connection()
            .await
            .map_err(|e| truncate_err(e.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TestStatus {
    Idle,
    Testing,
    Ok,
    Err(String),
}

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

pub struct CloudTtsEnginesView {
    /// `None` only when the speak subsystem didn't build; persistence still happens without it.
    registry: Option<Arc<RwLock<TtsRegistry>>>,
    credentials: Arc<dyn CredentialsRepo>,
    rt_handle: tokio::runtime::Handle,
    speak: Option<SpeakQueueHandle>,
    azure: AzureForm,
    elevenlabs: ElevenLabsForm,
    openai: OpenAiForm,
    polly: PollyForm,
    _subs: Vec<Subscription>,
}

impl CloudTtsEnginesView {
    pub fn new(
        registry: Option<Arc<RwLock<TtsRegistry>>>,
        credentials: Arc<dyn CredentialsRepo>,
        rt_handle: tokio::runtime::Handle,
        speak: Option<SpeakQueueHandle>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();

        let registered = registry
            .as_ref()
            .map(|r| r.read().unwrap_or_else(|e| e.into_inner()).engine_ids())
            .unwrap_or_default();
        let is_registered = |kind: CloudEngineKind| registered.contains(&kind.engine_id());

        let azure_api = field("Subscription key", true, palette, cx);
        let azure_region = field("e.g. eastus", false, palette, cx);
        let eleven_api = field("xi-api-key", true, palette, cx);
        let openai_api = field("sk-...", true, palette, cx);
        let polly_access = field("AKIA...", false, palette, cx);
        let polly_secret = field("secret access key", true, palette, cx);
        let polly_region = field("e.g. us-east-1", false, palette, cx);

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
            registry,
            credentials,
            rt_handle,
            speak,
            azure: AzureForm {
                api_key: azure_api,
                region: azure_region,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: is_registered(CloudEngineKind::Azure),
            },
            elevenlabs: ElevenLabsForm {
                api_key: eleven_api,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: is_registered(CloudEngineKind::ElevenLabs),
            },
            openai: OpenAiForm {
                api_key: openai_api,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: is_registered(CloudEngineKind::OpenAI),
            },
            polly: PollyForm {
                access_key: polly_access,
                secret_key: polly_secret,
                region: polly_region,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: is_registered(CloudEngineKind::Polly),
            },
            _subs: subs,
        }
    }

    fn mark_dirty(&mut self, kind: CloudEngineKind, cx: &mut Context<Self>) {
        match kind {
            CloudEngineKind::Azure => self.azure.dirty = true,
            CloudEngineKind::ElevenLabs => self.elevenlabs.dirty = true,
            CloudEngineKind::OpenAI => self.openai.dirty = true,
            CloudEngineKind::Polly => self.polly.dirty = true,
        }
        cx.notify();
    }

    fn build_creds(&self, kind: CloudEngineKind, cx: &App) -> Option<CloudCreds> {
        match kind {
            CloudEngineKind::Azure => {
                let api_key = self.azure.api_key.read(cx).content().to_owned();
                let region = self.azure.region.read(cx).content().to_owned();
                if api_key.trim().is_empty() || region.trim().is_empty() {
                    return None;
                }
                Some(CloudCreds::Azure(AzureCredentials {
                    api_key,
                    region,
                    base_url: None,
                }))
            }
            CloudEngineKind::ElevenLabs => {
                let api_key = self.elevenlabs.api_key.read(cx).content().to_owned();
                if api_key.trim().is_empty() {
                    return None;
                }
                Some(CloudCreds::ElevenLabs(ElevenLabsCredentials {
                    api_key,
                    base_url: None,
                }))
            }
            CloudEngineKind::OpenAI => {
                let api_key = self.openai.api_key.read(cx).content().to_owned();
                if api_key.trim().is_empty() {
                    return None;
                }
                Some(CloudCreds::OpenAi(OpenAiCredentials {
                    api_key,
                    base_url: None,
                }))
            }
            CloudEngineKind::Polly => {
                let access_key_id = self.polly.access_key.read(cx).content().to_owned();
                let secret_access_key = self.polly.secret_key.read(cx).content().to_owned();
                let region = self.polly.region.read(cx).content().to_owned();
                if access_key_id.trim().is_empty()
                    || secret_access_key.trim().is_empty()
                    || region.trim().is_empty()
                {
                    return None;
                }
                Some(CloudCreds::Polly(PollyCredentials {
                    access_key_id,
                    secret_access_key,
                    region,
                    base_url: None,
                }))
            }
        }
    }

    fn save(&mut self, kind: CloudEngineKind, cx: &mut Context<Self>) {
        let Some(creds) = self.build_creds(kind, cx) else {
            return;
        };
        let json = match creds.to_json() {
            Ok(json) => json,
            Err(e) => {
                cx.push_toast(
                    ToastKind::Error,
                    format!("Couldn't save {} credentials: {e}", kind.display_name()),
                );
                return;
            }
        };
        let repo = Arc::clone(&self.credentials);
        let credential_id = kind.credential_id();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
        self.rt_handle.spawn(async move {
            let outcome = repo
                .store(&CredentialId::new(credential_id), &json)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(())) => {
                let _ = this.update(cx, |this, cx| this.on_saved(kind, creds, cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_save_failed(kind, &message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn on_saved(&mut self, kind: CloudEngineKind, creds: CloudCreds, cx: &mut Context<Self>) {
        match kind {
            CloudEngineKind::Azure => self.azure.dirty = false,
            CloudEngineKind::ElevenLabs => self.elevenlabs.dirty = false,
            CloudEngineKind::OpenAI => self.openai.dirty = false,
            CloudEngineKind::Polly => self.polly.dirty = false,
        }
        self.hot_register(kind, creds);
        cx.push_toast(
            ToastKind::Info,
            format!("{} credentials saved", kind.display_name()),
        );
        cx.notify();
    }

    fn on_save_failed(&mut self, kind: CloudEngineKind, message: &str, cx: &mut Context<Self>) {
        cx.push_toast(
            ToastKind::Error,
            format!(
                "Couldn't save {} credentials: {message}",
                kind.display_name()
            ),
        );
        cx.notify();
    }

    fn hot_register(&mut self, kind: CloudEngineKind, creds: CloudCreds) {
        let Some(registry) = self.registry.as_ref() else {
            return;
        };
        match creds {
            CloudCreds::Azure(c) => cloud_tts_boot::register_azure(registry, c),
            CloudCreds::ElevenLabs(c) => cloud_tts_boot::register_elevenlabs(registry, c),
            CloudCreds::OpenAi(c) => cloud_tts_boot::register_openai(registry, c),
            CloudCreds::Polly(c) => cloud_tts_boot::register_polly(registry, c),
        };
        match kind {
            CloudEngineKind::Azure => self.azure.is_registered = true,
            CloudEngineKind::ElevenLabs => self.elevenlabs.is_registered = true,
            CloudEngineKind::OpenAI => self.openai.is_registered = true,
            CloudEngineKind::Polly => self.polly.is_registered = true,
        }
        if let Some(queue) = self.speak.clone() {
            self.rt_handle.spawn(async move {
                if let Err(e) = queue.send(SpeakCommand::RefreshVoiceCatalog).await {
                    eprintln!("forge-desktop: cloud engine voice-catalog refresh failed: {e}");
                }
            });
        }
    }

    fn test(&mut self, kind: CloudEngineKind, cx: &mut Context<Self>) {
        let Some(creds) = self.build_creds(kind, cx) else {
            return;
        };
        self.set_test_status(kind, TestStatus::Testing);
        cx.notify();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
        self.rt_handle.spawn(async move {
            let _ = tx.send(creds.test().await);
        });
        cx.spawn(async move |this, cx| {
            let status = match rx.await {
                Ok(Ok(())) => TestStatus::Ok,
                Ok(Err(e)) => TestStatus::Err(e),
                Err(_) => return,
            };
            let _ = this.update(cx, |this, cx| {
                this.set_test_status(kind, status);
                cx.notify();
            });
        })
        .detach();
    }

    fn set_test_status(&mut self, kind: CloudEngineKind, status: TestStatus) {
        match kind {
            CloudEngineKind::Azure => self.azure.test_status = status,
            CloudEngineKind::ElevenLabs => self.elevenlabs.test_status = status,
            CloudEngineKind::OpenAI => self.openai.test_status = status,
            CloudEngineKind::Polly => self.polly.test_status = status,
        }
    }

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
        let test_base = div()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(test_border)
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(test_fg)
            .child(test_label);
        let test_btn: AnyElement = if can_test {
            test_base
                .id(SharedString::from(format!("cloud-test-{}", kind.key())))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.test(kind, cx)))
                .into_any_element()
        } else {
            test_base.into_any_element()
        };

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

fn field(
    placeholder: &'static str,
    secure: bool,
    palette: ForgePalette,
    cx: &mut Context<CloudTtsEnginesView>,
) -> Entity<TextInput> {
    cx.new(|cx| {
        TextInput::new(placeholder, cx)
            .with_palette(palette)
            .with_font_size(FONT_SM)
            .secure(secure)
    })
}

fn nonempty(input: &Entity<TextInput>, cx: &App) -> bool {
    !input.read(cx).content().trim().is_empty()
}

fn truncate_err(s: String) -> String {
    const MAX: usize = 120;
    if s.chars().count() <= MAX {
        s
    } else {
        s.chars().take(MAX).collect::<String>() + "\u{2026}"
    }
}
