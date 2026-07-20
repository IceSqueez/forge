use std::sync::{Arc, RwLock};

use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_XS, FONT_XXS,
    ForgePalette, Icon, InputEvent, Radius, Spacing, TextInput, ToastKind, card, icon, radius,
    spacing, tr,
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
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::cloud_tts_boot;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const RESULT_DOT: Pixels = px(6.0);
const BOX_RADIUS: Pixels = px(7.0);
const BOX_PAD_X: Pixels = px(11.0);
const BOX_PAD_Y: Pixels = px(7.0);
const FS_12: Pixels = px(12.0);
const NOTE_FS: Pixels = px(11.0);
const EYE_GLYPH: Pixels = px(12.0);
const LOCK_GLYPH: Pixels = px(11.0);
const GRID_GAP: Pixels = px(12.0);
const CARD_PAD: Pixels = px(14.0);
const CARD_MB: Pixels = px(18.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudEngineKind {
    Azure,
    ElevenLabs,
    OpenAI,
    Polly,
}

impl CloudEngineKind {
    pub const ALL: [CloudEngineKind; 4] = [
        CloudEngineKind::Azure,
        CloudEngineKind::ElevenLabs,
        CloudEngineKind::OpenAI,
        CloudEngineKind::Polly,
    ];

    pub fn key(self) -> &'static str {
        match self {
            CloudEngineKind::Azure => "azure",
            CloudEngineKind::ElevenLabs => "elevenlabs",
            CloudEngineKind::OpenAI => "openai",
            CloudEngineKind::Polly => "polly",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            CloudEngineKind::Azure => "Azure Speech",
            CloudEngineKind::ElevenLabs => "ElevenLabs",
            CloudEngineKind::OpenAI => "OpenAI TTS",
            CloudEngineKind::Polly => "Amazon Polly",
        }
    }

    pub fn from_engine_id(id: &str) -> Option<CloudEngineKind> {
        match id {
            "azure" => Some(CloudEngineKind::Azure),
            "elevenlabs" => Some(CloudEngineKind::ElevenLabs),
            "openai" => Some(CloudEngineKind::OpenAI),
            "polly" => Some(CloudEngineKind::Polly),
            _ => None,
        }
    }

    pub fn credential_id(self) -> &'static str {
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

#[derive(Clone, Copy)]
enum SecureField {
    AzureApi,
    ElevenApi,
    OpenAiApi,
    PollyAccess,
    PollySecret,
}

/// Emitted after a cloud engine registers into the live registry so the parent
/// engines view can refresh its rail roster and select the new entry.
pub struct CloudEngineRegistered(pub EngineId);

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
    api_revealed: bool,
    dirty: bool,
    test_status: TestStatus,
    is_registered: bool,
}

struct ElevenLabsForm {
    api_key: Entity<TextInput>,
    api_revealed: bool,
    dirty: bool,
    test_status: TestStatus,
    is_registered: bool,
}

struct OpenAiForm {
    api_key: Entity<TextInput>,
    api_revealed: bool,
    dirty: bool,
    test_status: TestStatus,
    is_registered: bool,
}

struct PollyForm {
    access_key: Entity<TextInput>,
    secret_key: Entity<TextInput>,
    region: Entity<TextInput>,
    access_revealed: bool,
    secret_revealed: bool,
    dirty: bool,
    test_status: TestStatus,
    is_registered: bool,
}

pub struct CloudCredentialsView {
    /// `None` only when the speak subsystem didn't build; persistence still happens without it.
    registry: Option<Arc<RwLock<TtsRegistry>>>,
    credentials: Arc<dyn CredentialsRepo>,
    rt_handle: tokio::runtime::Handle,
    speak: Option<SpeakQueueHandle>,
    active: Option<CloudEngineKind>,
    azure: AzureForm,
    elevenlabs: ElevenLabsForm,
    openai: OpenAiForm,
    polly: PollyForm,
    _subs: Vec<Subscription>,
}

impl EventEmitter<CloudEngineRegistered> for CloudCredentialsView {}

impl CloudCredentialsView {
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

        let azure_api = field(
            tr!("tts_cloud_field_placeholder_subscription_key"),
            true,
            palette,
            cx,
        );
        let azure_region = field("e.g. eastus", false, palette, cx);
        let eleven_api = field("xi-api-key", true, palette, cx);
        let openai_api = field("sk-...", true, palette, cx);
        let polly_access = field("AKIA...", true, palette, cx);
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
            active: None,
            azure: AzureForm {
                api_key: azure_api,
                region: azure_region,
                api_revealed: false,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: is_registered(CloudEngineKind::Azure),
            },
            elevenlabs: ElevenLabsForm {
                api_key: eleven_api,
                api_revealed: false,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: is_registered(CloudEngineKind::ElevenLabs),
            },
            openai: OpenAiForm {
                api_key: openai_api,
                api_revealed: false,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: is_registered(CloudEngineKind::OpenAI),
            },
            polly: PollyForm {
                access_key: polly_access,
                secret_key: polly_secret,
                region: polly_region,
                access_revealed: false,
                secret_revealed: false,
                dirty: false,
                test_status: TestStatus::Idle,
                is_registered: is_registered(CloudEngineKind::Polly),
            },
            _subs: subs,
        }
    }

    pub fn set_active(&mut self, kind: Option<CloudEngineKind>, cx: &mut Context<Self>) {
        if self.active != kind {
            self.active = kind;
            cx.notify();
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

    fn toggle_reveal(&mut self, field: SecureField, cx: &mut Context<Self>) {
        let (input, revealed) = match field {
            SecureField::AzureApi => {
                self.azure.api_revealed = !self.azure.api_revealed;
                (self.azure.api_key.clone(), self.azure.api_revealed)
            }
            SecureField::ElevenApi => {
                self.elevenlabs.api_revealed = !self.elevenlabs.api_revealed;
                (
                    self.elevenlabs.api_key.clone(),
                    self.elevenlabs.api_revealed,
                )
            }
            SecureField::OpenAiApi => {
                self.openai.api_revealed = !self.openai.api_revealed;
                (self.openai.api_key.clone(), self.openai.api_revealed)
            }
            SecureField::PollyAccess => {
                self.polly.access_revealed = !self.polly.access_revealed;
                (self.polly.access_key.clone(), self.polly.access_revealed)
            }
            SecureField::PollySecret => {
                self.polly.secret_revealed = !self.polly.secret_revealed;
                (self.polly.secret_key.clone(), self.polly.secret_revealed)
            }
        };
        input.update(cx, |input, cx| input.set_secure(!revealed, cx));
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
                    tr!(
                        "tts_cloud_save_failed_toast",
                        name = kind.display_name(),
                        error = e.to_string()
                    ),
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
        self.hot_register(kind, creds, cx);
        cx.push_toast(
            ToastKind::Info,
            tr!("tts_cloud_saved_toast", name = kind.display_name()),
        );
        cx.notify();
    }

    fn on_save_failed(&mut self, kind: CloudEngineKind, message: &str, cx: &mut Context<Self>) {
        cx.push_toast(
            ToastKind::Error,
            tr!(
                "tts_cloud_save_failed_toast",
                name = kind.display_name(),
                error = message
            ),
        );
        cx.notify();
    }

    fn hot_register(&mut self, kind: CloudEngineKind, creds: CloudCreds, cx: &mut Context<Self>) {
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
        cx.emit(CloudEngineRegistered(kind.engine_id()));
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

    fn is_registered(&self, kind: CloudEngineKind) -> bool {
        match kind {
            CloudEngineKind::Azure => self.azure.is_registered,
            CloudEngineKind::ElevenLabs => self.elevenlabs.is_registered,
            CloudEngineKind::OpenAI => self.openai.is_registered,
            CloudEngineKind::Polly => self.polly.is_registered,
        }
    }

    fn credentials_card(
        &self,
        kind: CloudEngineKind,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (grid, extra, test_status, can_save, can_test): (
            AnyElement,
            Option<AnyElement>,
            &TestStatus,
            bool,
            bool,
        ) = match kind {
            CloudEngineKind::Azure => {
                let has_key = nonempty(&self.azure.api_key, cx);
                let has_region = nonempty(&self.azure.region, cx);
                let grid = two_col(
                    self.cred_field(
                        tr!("tts_cloud_field_api_key"),
                        self.azure.api_key.clone(),
                        Some((SecureField::AzureApi, self.azure.api_revealed)),
                        palette,
                        cx,
                    ),
                    self.cred_field(
                        tr!("tts_cloud_field_region"),
                        self.azure.region.clone(),
                        None,
                        palette,
                        cx,
                    ),
                );
                (
                    grid,
                    None,
                    &self.azure.test_status,
                    self.azure.dirty && has_key && has_region,
                    has_key && has_region && self.azure.test_status != TestStatus::Testing,
                )
            }
            CloudEngineKind::ElevenLabs => {
                let has_key = nonempty(&self.elevenlabs.api_key, cx);
                let grid = self.cred_field(
                    tr!("tts_cloud_field_api_key"),
                    self.elevenlabs.api_key.clone(),
                    Some((SecureField::ElevenApi, self.elevenlabs.api_revealed)),
                    palette,
                    cx,
                );
                (
                    grid,
                    None,
                    &self.elevenlabs.test_status,
                    self.elevenlabs.dirty && has_key,
                    has_key && self.elevenlabs.test_status != TestStatus::Testing,
                )
            }
            CloudEngineKind::OpenAI => {
                let has_key = nonempty(&self.openai.api_key, cx);
                let grid = self.cred_field(
                    tr!("tts_cloud_field_api_key"),
                    self.openai.api_key.clone(),
                    Some((SecureField::OpenAiApi, self.openai.api_revealed)),
                    palette,
                    cx,
                );
                (
                    grid,
                    None,
                    &self.openai.test_status,
                    self.openai.dirty && has_key,
                    has_key && self.openai.test_status != TestStatus::Testing,
                )
            }
            CloudEngineKind::Polly => {
                let has_access = nonempty(&self.polly.access_key, cx);
                let has_secret = nonempty(&self.polly.secret_key, cx);
                let has_region = nonempty(&self.polly.region, cx);
                let grid = two_col(
                    self.cred_field(
                        tr!("tts_cloud_field_access_key_id"),
                        self.polly.access_key.clone(),
                        Some((SecureField::PollyAccess, self.polly.access_revealed)),
                        palette,
                        cx,
                    ),
                    self.cred_field(
                        tr!("tts_cloud_field_secret_key"),
                        self.polly.secret_key.clone(),
                        Some((SecureField::PollySecret, self.polly.secret_revealed)),
                        palette,
                        cx,
                    ),
                );
                let region = self.cred_field(
                    tr!("tts_cloud_field_region"),
                    self.polly.region.clone(),
                    None,
                    palette,
                    cx,
                );
                (
                    grid,
                    Some(region),
                    &self.polly.test_status,
                    self.polly.dirty && has_access && has_secret && has_region,
                    has_access
                        && has_secret
                        && has_region
                        && self.polly.test_status != TestStatus::Testing,
                )
            }
        };

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(grid);
        if let Some(extra) = extra {
            body = body.child(extra);
        }
        body = body.child(encryption_note(palette)).child(self.action_row(
            kind,
            test_status,
            can_save,
            can_test,
            palette,
            density,
            cx,
        ));
        if let Some(result) = test_result_row(test_status, palette, density) {
            body = body.child(result);
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .mb(CARD_MB)
            .child(section_label(
                tr!("tts_engines_section_credentials"),
                palette,
            ))
            .child(card(body, palette).padding(CARD_PAD).full_width())
            .into_any_element()
    }

    fn cred_field(
        &self,
        label: impl Into<SharedString>,
        input: Entity<TextInput>,
        eye: Option<(SecureField, bool)>,
        palette: &ForgePalette,
        cx: &Context<Self>,
    ) -> AnyElement {
        let label: SharedString = label.into();
        let label = label.to_uppercase();

        let mut value_box = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .rounded(BOX_RADIUS)
            .px(BOX_PAD_X)
            .py(BOX_PAD_Y)
            .child(div().flex_1().min_w(px(0.0)).child(input));

        if let Some((field, revealed)) = eye {
            let glyph = if revealed { Icon::EyeOff } else { Icon::Eye };
            value_box =
                value_box.child(
                    div()
                        .id(SharedString::from(format!(
                            "cred-eye-{}",
                            secure_field_id(field)
                        )))
                        .flex_shrink_0()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.toggle_reveal(field, cx)
                        }))
                        .child(icon(glyph, EYE_GLYPH, palette.text_faint)),
                );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(label),
            )
            .child(value_box)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn action_row(
        &self,
        kind: CloudEngineKind,
        test_status: &TestStatus,
        can_save: bool,
        can_test: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let test_label = if *test_status == TestStatus::Testing {
            tr!("tts_cloud_testing_btn")
        } else {
            tr!("tts_cloud_test_connection_btn")
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
            .text_size(FONT_XS)
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
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(tr!("tts_cloud_save_credentials_btn"))
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
                .text_size(FONT_XS)
                .text_color(palette.disabled)
                .child(tr!("tts_cloud_save_credentials_btn"))
                .into_any_element()
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(config_status_badge(
                test_status,
                self.is_registered(kind),
                palette,
                density,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(test_btn)
                    .child(save_btn),
            )
            .into_any_element()
    }
}

impl Render for CloudCredentialsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        match self.active {
            Some(kind) => self.credentials_card(kind, &palette, density, cx),
            None => div().into_any_element(),
        }
    }
}

fn secure_field_id(field: SecureField) -> &'static str {
    match field {
        SecureField::AzureApi => "azure-api",
        SecureField::ElevenApi => "eleven-api",
        SecureField::OpenAiApi => "openai-api",
        SecureField::PollyAccess => "polly-access",
        SecureField::PollySecret => "polly-secret",
    }
}

fn two_col(left: AnyElement, right: AnyElement) -> AnyElement {
    div()
        .w_full()
        .flex()
        .gap(GRID_GAP)
        .child(div().flex_1().min_w(px(0.0)).child(left))
        .child(div().flex_1().min_w(px(0.0)).child(right))
        .into_any_element()
}

fn section_label(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label)
}

fn encryption_note(palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .child(icon(Icon::Lock, LOCK_GLYPH, palette.success))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(NOTE_FS)
                .text_color(palette.text_muted)
                .child(tr!("tts_engines_creds_encrypted_note")),
        )
}

fn config_status_badge(
    test_status: &TestStatus,
    is_registered: bool,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let (label, color) = if is_registered {
        (tr!("tts_cloud_configured"), palette.success)
    } else if matches!(test_status, TestStatus::Err(_)) {
        (tr!("tts_cloud_connection_failed"), palette.random)
    } else {
        (tr!("tts_cloud_not_configured"), palette.text_muted)
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
        TestStatus::Ok => (palette.success, tr!("tts_cloud_connection_verified")),
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

fn field(
    placeholder: impl Into<SharedString>,
    secure: bool,
    palette: ForgePalette,
    cx: &mut Context<CloudCredentialsView>,
) -> Entity<TextInput> {
    cx.new(|cx| {
        TextInput::new(placeholder, cx)
            .with_palette(palette)
            .plain()
            .mono()
            .with_font_size(FS_12)
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
