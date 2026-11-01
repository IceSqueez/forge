use forge_components::FONT_META;
use std::sync::Arc;

use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent,
    KeyActivated, Radius, Spacing, TextInput, ToastKind, card, icon, radius, scaled, section_label,
    spacing, tr,
};
use forge_speak_queue::{SpeakCommand, SpeakQueueHandle};
use forge_speech_types::EngineId;
use forge_storage::{CredentialId, CredentialsRepo};
use forge_tts_cloud::CloudGateways;
use forge_tts_cloud::azure::AzureEngineFactory;
use forge_tts_cloud::credentials::{
    AZURE_CREDENTIAL_ID, AzureCredentials, ELEVENLABS_CREDENTIAL_ID, ElevenLabsCredentials,
    OPENAI_CREDENTIAL_ID, OpenAiCredentials, POLLY_CREDENTIAL_ID, PollyCredentials,
};
use forge_tts_cloud::elevenlabs::ElevenLabsEngineFactory;
use forge_tts_cloud::openai::OpenAiEngineFactory;
use forge_tts_cloud::polly::PollyEngineFactory;
use forge_tts_core::TtsEngineFactory;
use forge_types::Secret;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};
use serde::Deserialize;

use crate::app::async_bridge::{self, Generation};
use crate::shared::repo_failure::RepoFailure;
use forge_components::PushToast;
use forge_host::cloud_tts_boot;

const VALUE_BOX_GAP: Pixels = px(8.0);
const NOTE_GAP: Pixels = px(5.0);
const RESULT_DOT: Pixels = px(6.0);
const BOX_RADIUS: Pixels = px(7.0);
const BOX_PAD_X: Pixels = px(11.0);
const BOX_PAD_Y: Pixels = px(7.0);
const NOTE_FS: Pixels = FONT_META;
const EYE_GLYPH: Pixels = px(12.0);
const LOCK_GLYPH: Pixels = px(11.0);
const GRID_GAP: Pixels = px(12.0);
const CARD_PAD: Pixels = px(14.0);
const CARD_MB: Pixels = px(18.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloudEngineKind {
    Azure,
    ElevenLabs,
    OpenAI,
    Polly,
}

impl CloudEngineKind {
    pub(crate) const ALL: [CloudEngineKind; 4] = [
        CloudEngineKind::Azure,
        CloudEngineKind::ElevenLabs,
        CloudEngineKind::OpenAI,
        CloudEngineKind::Polly,
    ];

    pub(crate) fn key(self) -> &'static str {
        match self {
            CloudEngineKind::Azure => forge_tts_core::ENGINE_ID_AZURE,
            CloudEngineKind::ElevenLabs => forge_tts_core::ENGINE_ID_ELEVENLABS,
            CloudEngineKind::OpenAI => forge_tts_core::ENGINE_ID_OPENAI,
            CloudEngineKind::Polly => forge_tts_core::ENGINE_ID_POLLY,
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        forge_tts_core::engine_display_name(self.key())
    }

    pub(crate) fn from_engine_id(id: &str) -> Option<CloudEngineKind> {
        match id {
            forge_tts_core::ENGINE_ID_AZURE => Some(CloudEngineKind::Azure),
            forge_tts_core::ENGINE_ID_ELEVENLABS => Some(CloudEngineKind::ElevenLabs),
            forge_tts_core::ENGINE_ID_OPENAI => Some(CloudEngineKind::OpenAI),
            forge_tts_core::ENGINE_ID_POLLY => Some(CloudEngineKind::Polly),
            _ => None,
        }
    }

    pub(crate) fn credential_id(self) -> &'static str {
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

    fn fields(self) -> &'static [CredentialField] {
        match self {
            CloudEngineKind::Azure => &AZURE_FIELDS,
            CloudEngineKind::ElevenLabs => &ELEVENLABS_FIELDS,
            CloudEngineKind::OpenAI => &OPENAI_FIELDS,
            CloudEngineKind::Polly => &POLLY_FIELDS,
        }
    }

    fn creds(self, entered: &EnteredCredentials) -> CloudCreds {
        match self {
            CloudEngineKind::Azure => CloudCreds::Azure(AzureCredentials {
                api_key: Secret::new(entered.text(CredentialSlot::ApiKey)),
                region: entered.text(CredentialSlot::Region),
                base_url: None,
            }),
            CloudEngineKind::ElevenLabs => CloudCreds::ElevenLabs(ElevenLabsCredentials {
                api_key: Secret::new(entered.text(CredentialSlot::ApiKey)),
                base_url: None,
            }),
            CloudEngineKind::OpenAI => CloudCreds::OpenAi(OpenAiCredentials {
                api_key: Secret::new(entered.text(CredentialSlot::ApiKey)),
                base_url: None,
            }),
            CloudEngineKind::Polly => CloudCreds::Polly(PollyCredentials {
                access_key_id: Secret::new(entered.text(CredentialSlot::ApiKey)),
                secret_access_key: Secret::new(entered.text(CredentialSlot::SecretKey)),
                region: entered.text(CredentialSlot::Region),
                base_url: None,
            }),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CredentialSlot {
    ApiKey,
    SecretKey,
    Region,
}

struct CredentialField {
    slot: CredentialSlot,
    label: fn() -> String,
    placeholder: fn() -> String,
    secure: bool,
    id: &'static str,
}

const AZURE_FIELDS: [CredentialField; 2] = [
    CredentialField {
        slot: CredentialSlot::ApiKey,
        label: || tr!("tts_cloud_field_api_key"),
        placeholder: || tr!("tts_cloud_field_placeholder_subscription_key"),
        secure: true,
        id: "azure-api",
    },
    CredentialField {
        slot: CredentialSlot::Region,
        label: || tr!("tts_cloud_field_region"),
        placeholder: || tr!("tts_cloud_field_placeholder_azure_region"),
        secure: false,
        id: "azure-region",
    },
];

const ELEVENLABS_FIELDS: [CredentialField; 1] = [CredentialField {
    slot: CredentialSlot::ApiKey,
    label: || tr!("tts_cloud_field_api_key"),
    placeholder: || "xi-api-key".to_owned(),
    secure: true,
    id: "eleven-api",
}];

const OPENAI_FIELDS: [CredentialField; 1] = [CredentialField {
    slot: CredentialSlot::ApiKey,
    label: || tr!("tts_cloud_field_api_key"),
    placeholder: || "sk-...".to_owned(),
    secure: true,
    id: "openai-api",
}];

const POLLY_FIELDS: [CredentialField; 3] = [
    CredentialField {
        slot: CredentialSlot::ApiKey,
        label: || tr!("tts_cloud_field_access_key_id"),
        placeholder: || "AKIA...".to_owned(),
        secure: true,
        id: "polly-access",
    },
    CredentialField {
        slot: CredentialSlot::SecretKey,
        label: || tr!("tts_cloud_field_secret_key"),
        placeholder: || tr!("tts_cloud_field_placeholder_secret_key"),
        secure: true,
        id: "polly-secret",
    },
    CredentialField {
        slot: CredentialSlot::Region,
        label: || tr!("tts_cloud_field_region"),
        placeholder: || tr!("tts_cloud_field_placeholder_polly_region"),
        secure: false,
        id: "polly-region",
    },
];

struct EnteredCredentials(Vec<(CredentialSlot, String)>);

impl EnteredCredentials {
    fn text(&self, slot: CredentialSlot) -> String {
        self.0
            .iter()
            .find(|(entered, _)| *entered == slot)
            .map(|(_, text)| text.clone())
            .unwrap_or_default()
    }
}

#[derive(Deserialize)]
struct StoredRegion {
    region: Option<String>,
}

pub(crate) async fn stored_region(
    credentials: &dyn CredentialsRepo,
    kind: CloudEngineKind,
) -> Result<Option<String>, RepoFailure> {
    let stored = credentials
        .load(&CredentialId::new(kind.credential_id()))
        .await
        .map_err(RepoFailure::from)?;
    let Some(json) = stored else {
        return Ok(None);
    };
    let parsed: StoredRegion = serde_json::from_str(&json)
        .map_err(|_| RepoFailure::operation("stored cloud credentials are not valid JSON"))?;
    Ok(parsed.region)
}

pub(crate) struct CloudEngineRegistered(pub EngineId);

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

    async fn test(self, gateways: CloudGateways) -> Result<(), RepoFailure> {
        let engine = match self {
            CloudCreds::Azure(c) => AzureEngineFactory::new(c, &gateways).create(),
            CloudCreds::ElevenLabs(c) => ElevenLabsEngineFactory::new(c, &gateways).create(),
            CloudCreds::OpenAi(c) => OpenAiEngineFactory::new(c, &gateways).create(),
            CloudCreds::Polly(c) => PollyEngineFactory::new(c, &gateways).create(),
        }
        .map_err(RepoFailure::from)?;
        engine.test_connection().await.map_err(RepoFailure::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TestStatus {
    Idle,
    Testing,
    Ok,
    Err(String),
}

struct CredentialInput {
    spec: &'static CredentialField,
    input: Entity<TextInput>,
    revealed: bool,
}

struct CloudEngineForm {
    kind: CloudEngineKind,
    inputs: Vec<CredentialInput>,
    dirty: bool,
    test_status: TestStatus,
    test_gen: Generation,
    is_registered: bool,
}

impl CloudEngineForm {
    fn all_entered(&self, cx: &App) -> bool {
        self.inputs.iter().all(|entry| nonempty(&entry.input, cx))
    }

    fn creds(&self, cx: &App) -> Option<CloudCreds> {
        self.all_entered(cx).then(|| {
            self.kind.creds(&EnteredCredentials(
                self.inputs
                    .iter()
                    .map(|entry| {
                        let text = entry.input.read(cx).content().trim().to_owned();
                        (entry.spec.slot, text)
                    })
                    .collect(),
            ))
        })
    }
}

pub(crate) struct CloudCredentialsView {
    gateways: CloudGateways,
    credentials: Arc<dyn CredentialsRepo>,
    rt_handle: tokio::runtime::Handle,
    speak: Option<SpeakQueueHandle>,
    active: Option<CloudEngineKind>,
    forms: Vec<CloudEngineForm>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<CloudEngineRegistered> for CloudCredentialsView {}

impl CloudCredentialsView {
    pub(crate) fn new(
        gateways: CloudGateways,
        credentials: Arc<dyn CredentialsRepo>,
        rt_handle: tokio::runtime::Handle,
        speak: Option<SpeakQueueHandle>,
        cx: &mut Context<Self>,
    ) -> Self {
        let registered = speak
            .as_ref()
            .map(|queue| queue.registered_engine_ids())
            .unwrap_or_default();
        let mut subs = Vec::new();
        let forms = CloudEngineKind::ALL
            .into_iter()
            .map(|kind| CloudEngineForm {
                kind,
                inputs: kind
                    .fields()
                    .iter()
                    .map(|spec| {
                        let input = field((spec.placeholder)(), spec.secure, cx);
                        subs.push(cx.subscribe(
                            &input,
                            move |this, _input, event: &InputEvent, cx| {
                                if let InputEvent::Changed(_) = event {
                                    this.mark_dirty(kind, cx);
                                }
                            },
                        ));
                        CredentialInput {
                            spec,
                            input,
                            revealed: false,
                        }
                    })
                    .collect(),
                dirty: false,
                test_status: TestStatus::Idle,
                test_gen: Generation::default(),
                is_registered: registered.contains(&kind.engine_id()),
            })
            .collect();

        Self {
            gateways,
            credentials,
            rt_handle,
            speak,
            active: None,
            forms,
            _subs: subs,
        }
    }

    fn form(&self, kind: CloudEngineKind) -> Option<&CloudEngineForm> {
        self.forms.iter().find(|form| form.kind == kind)
    }

    fn form_mut(&mut self, kind: CloudEngineKind) -> Option<&mut CloudEngineForm> {
        self.forms.iter_mut().find(|form| form.kind == kind)
    }

    pub(crate) fn set_active(&mut self, kind: Option<CloudEngineKind>, cx: &mut Context<Self>) {
        if self.active != kind {
            self.active = kind;
            cx.notify();
        }
    }

    fn mark_dirty(&mut self, kind: CloudEngineKind, cx: &mut Context<Self>) {
        if let Some(form) = self.form_mut(kind) {
            form.dirty = true;
            form.test_gen.next();
            form.test_status = TestStatus::Idle;
        }
        cx.notify();
    }

    fn toggle_reveal(&mut self, kind: CloudEngineKind, index: usize, cx: &mut Context<Self>) {
        let Some(entry) = self
            .form_mut(kind)
            .and_then(|form| form.inputs.get_mut(index))
        else {
            return;
        };
        entry.revealed = !entry.revealed;
        let (input, revealed) = (entry.input.clone(), entry.revealed);
        input.update(cx, |input, cx| input.set_secure(!revealed, cx));
        cx.notify();
    }

    fn build_creds(&self, kind: CloudEngineKind, cx: &App) -> Option<CloudCreds> {
        self.form(kind)?.creds(cx)
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
                        error = RepoFailure::operation(e).text()
                    ),
                );
                return;
            }
        };
        let repo = Arc::clone(&self.credentials);
        let credential_id = kind.credential_id();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                repo.store(&CredentialId::new(credential_id), &json)
                    .await
                    .map_err(RepoFailure::from)
            },
            move |this, result, cx| match result {
                Ok(()) => this.on_saved(kind, creds, cx),
                Err(message) => this.on_save_failed(kind, &message.text(), cx),
            },
            cx,
        );
    }

    fn on_saved(&mut self, kind: CloudEngineKind, creds: CloudCreds, cx: &mut Context<Self>) {
        if let Some(form) = self.form_mut(kind) {
            form.dirty = false;
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
        let Some(queue) = self.speak.clone() else {
            return;
        };
        let registration = match creds {
            CloudCreds::Azure(c) => cloud_tts_boot::azure_registration(c, &self.gateways),
            CloudCreds::ElevenLabs(c) => cloud_tts_boot::elevenlabs_registration(c, &self.gateways),
            CloudCreds::OpenAi(c) => cloud_tts_boot::openai_registration(c, &self.gateways),
            CloudCreds::Polly(c) => cloud_tts_boot::polly_registration(c, &self.gateways),
        };
        if let Some(form) = self.form_mut(kind) {
            form.is_registered = true;
        }
        self.rt_handle.spawn(async move {
            if let Err(e) = queue.send(SpeakCommand::RegisterEngine(registration)).await {
                tracing::warn!(error = %e, "cloud engine registration failed");
            }
        });
        cx.emit(CloudEngineRegistered(kind.engine_id()));
    }

    fn test(&mut self, kind: CloudEngineKind, cx: &mut Context<Self>) {
        let Some(creds) = self.build_creds(kind, cx) else {
            return;
        };
        let Some(form) = self.form_mut(kind) else {
            return;
        };
        form.test_status = TestStatus::Testing;
        let ticket = form.test_gen.next();
        cx.notify();
        let gateways = self.gateways.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { creds.test(gateways).await },
            move |this, result: Result<(), RepoFailure>, cx| {
                if !this
                    .form(kind)
                    .is_some_and(|form| form.test_gen.is_current(ticket))
                {
                    return;
                }
                let status = match result {
                    Ok(()) => TestStatus::Ok,
                    Err(e) => TestStatus::Err(e.text()),
                };
                this.set_test_status(kind, status);
                cx.notify();
            },
            cx,
        );
    }

    fn set_test_status(&mut self, kind: CloudEngineKind, status: TestStatus) {
        if let Some(form) = self.form_mut(kind) {
            form.test_status = status;
        }
    }

    fn credential_rows(
        &self,
        form: &CloudEngineForm,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let fields: Vec<AnyElement> = (0..form.inputs.len())
            .map(|index| self.cred_field(form, index, palette, cx))
            .collect();
        let mut rows = Vec::new();
        let mut fields = fields.into_iter();
        while let Some(left) = fields.next() {
            rows.push(match fields.next() {
                Some(right) => two_col(left, right, density),
                None => left,
            });
        }
        rows
    }

    fn credentials_card(
        &self,
        kind: CloudEngineKind,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.form(kind) else {
            return div().into_any_element();
        };
        let entered = form.all_entered(cx);
        let can_save = form.dirty && entered;
        let can_test = entered && form.test_status != TestStatus::Testing;

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .children(self.credential_rows(form, palette, density, cx))
            .child(encryption_note(palette, density, cx))
            .child(self.action_row(form, can_save, can_test, palette, density, cx));
        if let Some(result) = test_result_row(&form.test_status, palette, density, cx) {
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
                cx,
            ))
            .child(
                card(body, palette)
                    .padding(scaled(CARD_PAD, density))
                    .full_width(),
            )
            .into_any_element()
    }

    fn cred_field(
        &self,
        form: &CloudEngineForm,
        index: usize,
        palette: &ForgePalette,
        cx: &Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let Some(entry) = form.inputs.get(index) else {
            return div().into_any_element();
        };
        let label = (entry.spec.label)().to_uppercase();

        let mut value_box = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(scaled(VALUE_BOX_GAP, density))
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .rounded(BOX_RADIUS)
            .px(scaled(BOX_PAD_X, density))
            .py(scaled(BOX_PAD_Y, density))
            .child(div().flex_1().min_w(px(0.0)).child(entry.input.clone()));

        if entry.spec.secure {
            let glyph = if entry.revealed {
                Icon::EyeOff
            } else {
                Icon::Eye
            };
            let kind = form.kind;
            value_box = value_box.child(
                div()
                    .id(SharedString::from(format!("cred-eye-{}", entry.spec.id)))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .key_activated(palette.text_primary)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_reveal(kind, index, cx)
                    }))
                    .child(icon(glyph, EYE_GLYPH, palette.text_faint)),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(label),
            )
            .child(value_box)
            .into_any_element()
    }

    fn action_row(
        &self,
        form: &CloudEngineForm,
        can_save: bool,
        can_test: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kind = form.kind;
        let test_label = if form.test_status == TestStatus::Testing {
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
            .font_family(cx.body_family())
            .text_size(FONT_XS)
            .text_color(test_fg)
            .child(test_label);
        let test_btn: AnyElement = if can_test {
            test_base
                .id(SharedString::from(format!("cloud-test-{}", kind.key())))
                .cursor_pointer()
                .key_activated(palette.text_primary)
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
                .key_activated(palette.text_primary)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.save(kind, cx)))
                .font_family(cx.body_family())
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
                .font_family(cx.body_family())
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
                &form.test_status,
                form.is_registered,
                palette,
                density,
                cx,
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

fn two_col(left: AnyElement, right: AnyElement, density: Density) -> AnyElement {
    div()
        .w_full()
        .flex()
        .gap(scaled(GRID_GAP, density))
        .child(div().flex_1().min_w(px(0.0)).child(left))
        .child(div().flex_1().min_w(px(0.0)).child(right))
        .into_any_element()
}

fn encryption_note(palette: &ForgePalette, density: Density, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(scaled(NOTE_GAP, density))
        .child(icon(Icon::Lock, LOCK_GLYPH, palette.success))
        .child(
            div()
                .font_family(cx.body_family())
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
    cx: &App,
) -> impl IntoElement {
    let (label, color) = if is_registered {
        (tr!("tts_cloud_configured"), palette.success)
    } else if matches!(test_status, TestStatus::Err(_)) {
        (tr!("tts_cloud_connection_failed"), palette.danger)
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
                .font_family(cx.mono_family())
                .text_size(FONT_XS)
                .text_color(color)
                .child(label),
        )
}

fn test_result_row(
    test_status: &TestStatus,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> Option<AnyElement> {
    let (color, message): (Rgba, String) = match test_status {
        TestStatus::Ok => (palette.success, tr!("tts_cloud_connection_verified")),
        TestStatus::Err(error) => (palette.danger, error.clone()),
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
                    .font_family(cx.body_family())
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
    cx: &mut Context<CloudCredentialsView>,
) -> Entity<TextInput> {
    cx.new(|cx| {
        TextInput::new(placeholder, cx)
            .plain()
            .mono()
            .with_font_size(FONT_XS)
            .secure(secure)
    })
}

fn nonempty(input: &Entity<TextInput>, cx: &App) -> bool {
    !input.read(cx).content().trim().is_empty()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use forge_components::ToastKind;
    use forge_storage::credentials::MockCredentialsRepo;
    use forge_storage::{Language, StorageError};
    use gpui::TestAppContext;

    use super::*;
    use crate::test_support::{
        alive_tasks, install_presentation, pump_until, runtime, settle_until_tasks_drain_to,
        spawn_speak_queue_never_listing,
    };

    const ENGINES: [CloudEngineKind; 4] = [
        CloudEngineKind::Azure,
        CloudEngineKind::ElevenLabs,
        CloudEngineKind::OpenAI,
        CloudEngineKind::Polly,
    ];
    const UNADDRESSABLE_REGION: &str = "not a region";

    fn view_on(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
    ) -> Entity<CloudCredentialsView> {
        crate::app::i18n::install_language(Language::En);
        install_presentation(cx);
        cx.new(|cx| {
            CloudCredentialsView::new(
                CloudGateways::new(),
                Arc::new(MockCredentialsRepo::new()),
                rt.handle().clone(),
                None,
                cx,
            )
        })
    }

    fn form_of(this: &CloudCredentialsView, kind: CloudEngineKind) -> &CloudEngineForm {
        this.form(kind).unwrap()
    }

    fn is_registered(this: &CloudCredentialsView, kind: CloudEngineKind) -> bool {
        form_of(this, kind).is_registered
    }

    fn status_of(this: &CloudCredentialsView, kind: CloudEngineKind) -> TestStatus {
        form_of(this, kind).test_status.clone()
    }

    fn key_field(this: &CloudCredentialsView, kind: CloudEngineKind) -> Entity<TextInput> {
        form_of(this, kind).inputs[0].input.clone()
    }

    fn edit_key(
        view: &Entity<CloudCredentialsView>,
        kind: CloudEngineKind,
        cx: &mut TestAppContext,
    ) {
        let field = view.read_with(cx, |this, _| key_field(this, kind));
        field.update(cx, |_, cx| {
            cx.emit(InputEvent::Changed(SharedString::new_static("edited")));
        });
        cx.run_until_parked();
    }

    fn azure_probe_that_fails_offline(
        view: &Entity<CloudCredentialsView>,
        cx: &mut TestAppContext,
    ) {
        view.update(cx, |this, cx| {
            form_of(this, CloudEngineKind::Azure).inputs[0]
                .input
                .update(cx, |input, cx| input.set_content("key", cx));
            form_of(this, CloudEngineKind::Azure).inputs[1]
                .input
                .update(cx, |input, cx| input.set_content(UNADDRESSABLE_REGION, cx));
            this.test(CloudEngineKind::Azure, cx);
        });
    }

    #[gpui::test]
    fn editing_a_key_forgets_only_that_engines_verified_connection(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = view_on(cx, &rt);
        for edited in ENGINES {
            view.update(cx, |this, _| {
                for kind in ENGINES {
                    this.set_test_status(kind, TestStatus::Ok);
                }
            });

            edit_key(&view, edited, cx);

            view.read_with(cx, |this, _| {
                for kind in ENGINES {
                    let expected = if kind == edited {
                        TestStatus::Idle
                    } else {
                        TestStatus::Ok
                    };
                    assert_eq!(status_of(this, kind), expected, "{edited:?} then {kind:?}");
                }
            });
        }
    }

    #[gpui::test]
    fn a_probe_of_fields_edited_since_it_started_reports_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = view_on(cx, &rt);
        let background_tasks = alive_tasks(&rt);
        azure_probe_that_fails_offline(&view, cx);

        edit_key(&view, CloudEngineKind::Azure, cx);
        pump_until(&rt, || alive_tasks(&rt) == background_tasks);
        cx.run_until_parked();

        assert_eq!(
            view.read_with(cx, |this, _| status_of(this, CloudEngineKind::Azure)),
            TestStatus::Idle
        );
    }

    #[gpui::test]
    fn a_stale_probe_does_not_settle_the_probe_that_replaced_it(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = view_on(cx, &rt);
        let background_tasks = alive_tasks(&rt);
        azure_probe_that_fails_offline(&view, cx);

        edit_key(&view, CloudEngineKind::Azure, cx);
        view.update(cx, |this, _| {
            this.set_test_status(CloudEngineKind::Azure, TestStatus::Testing);
        });
        pump_until(&rt, || alive_tasks(&rt) == background_tasks);
        cx.run_until_parked();

        assert_eq!(
            view.read_with(cx, |this, _| status_of(this, CloudEngineKind::Azure)),
            TestStatus::Testing
        );
    }

    #[gpui::test]
    fn a_probe_of_untouched_fields_reports_its_result(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = view_on(cx, &rt);
        let background_tasks = alive_tasks(&rt);
        azure_probe_that_fails_offline(&view, cx);

        pump_until(&rt, || alive_tasks(&rt) == background_tasks);
        cx.run_until_parked();

        assert!(matches!(
            view.read_with(cx, |this, _| status_of(this, CloudEngineKind::Azure)),
            TestStatus::Err(_)
        ));
    }

    const PADDED_VALUE: &str = "  stored-value  ";
    const TRIMMED_VALUE: &str = "\"stored-value\"";
    const BLANK_VALUE: &str = "   ";
    const STORE_FAILURE: &str = "credential vault is locked";

    type StoredCredentials = Arc<Mutex<Vec<(String, String)>>>;

    struct Rig {
        rt: tokio::runtime::Runtime,
        view: Entity<CloudCredentialsView>,
        speak: Option<SpeakQueueHandle>,
        stored: StoredCredentials,
        registered: Arc<Mutex<Vec<EngineId>>>,
    }

    fn rig_with(
        cx: &mut TestAppContext,
        registered_on: Option<()>,
        store: Result<(), &'static str>,
    ) -> Rig {
        let rt = runtime();
        crate::app::i18n::install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let stored = StoredCredentials::default();
        let recorded = Arc::clone(&stored);
        let mut repo = MockCredentialsRepo::new();
        repo.expect_store().returning(move |id, json| {
            store.map_err(|reason| StorageError::Connection {
                reason: reason.to_owned(),
            })?;
            recorded
                .lock()
                .unwrap()
                .push((id.as_str().to_owned(), json.to_owned()));
            Ok(())
        });
        let speak = registered_on.map(|()| {
            let _runtime = rt.enter();
            spawn_speak_queue_never_listing(ENGINES.map(|kind| kind.engine_id()))
        });
        let handle = rt.handle().clone();
        let view = cx.new(|cx| {
            CloudCredentialsView::new(
                CloudGateways::new(),
                Arc::new(repo),
                handle,
                speak.clone(),
                cx,
            )
        });
        let registered = Arc::new(Mutex::new(Vec::new()));
        let heard = Arc::clone(&registered);
        cx.update(|cx| {
            cx.subscribe(&view, move |_, event: &CloudEngineRegistered, _| {
                heard.lock().unwrap().push(event.0.clone());
            })
            .detach();
        });
        Rig {
            rt,
            view,
            speak,
            stored,
            registered,
        }
    }

    fn required_fields(
        this: &CloudCredentialsView,
        kind: CloudEngineKind,
    ) -> Vec<Entity<TextInput>> {
        form_of(this, kind)
            .inputs
            .iter()
            .map(|entry| entry.input.clone())
            .collect()
    }

    fn fill(rig: &Rig, kind: CloudEngineKind, value: &str, cx: &mut TestAppContext) {
        rig.view.update(cx, |this, cx| {
            for field in required_fields(this, kind) {
                field.update(cx, |input, cx| input.set_content(value, cx));
            }
        });
        cx.run_until_parked();
    }

    fn dirty(this: &CloudCredentialsView, kind: CloudEngineKind) -> bool {
        form_of(this, kind).dirty
    }

    fn save_and_settle(rig: &Rig, kind: CloudEngineKind, cx: &mut TestAppContext) {
        let background_tasks = alive_tasks(&rig.rt);
        rig.view.update(cx, |this, cx| this.save(kind, cx));
        settle_until_tasks_drain_to(cx, &rig.rt, background_tasks);
    }

    fn toast_kinds(cx: &mut TestAppContext) -> Vec<(ToastKind, String)> {
        cx.update(|cx| {
            cx.global::<forge_components::Toasts>()
                .items()
                .iter()
                .map(|toast| (toast.kind, toast.message.to_string()))
                .collect()
        })
    }

    #[gpui::test]
    fn every_required_field_left_blank_refuses_the_credentials(cx: &mut TestAppContext) {
        let rig = rig_with(cx, Some(()), Ok(()));

        for kind in ENGINES {
            fill(&rig, kind, PADDED_VALUE, cx);
            let fields = rig
                .view
                .read_with(cx, |this, _| required_fields(this, kind));
            let complete = rig
                .view
                .read_with(cx, |this, cx| this.build_creds(kind, cx).is_some());
            assert!(complete, "{kind:?} refused a complete form");
            for blanked in fields {
                let kept = blanked.read_with(cx, |input, _| input.content().to_owned());
                blanked.update(cx, |input, cx| input.set_content(BLANK_VALUE, cx));
                let refused = rig
                    .view
                    .read_with(cx, |this, cx| this.build_creds(kind, cx).is_none());
                blanked.update(cx, |input, cx| input.set_content(&kept, cx));

                assert!(refused, "{kind:?} accepted a blank required field");
            }
        }
    }

    #[gpui::test]
    fn a_blank_form_neither_stores_nor_probes(cx: &mut TestAppContext) {
        let rig = rig_with(cx, Some(()), Ok(()));

        for kind in ENGINES {
            fill(&rig, kind, BLANK_VALUE, cx);
            rig.view.update(cx, |this, cx| this.test(kind, cx));
            save_and_settle(&rig, kind, cx);

            let status = rig.view.read_with(cx, |this, _| status_of(this, kind));
            assert_eq!(status, TestStatus::Idle, "{kind:?}");
        }
        assert_eq!(rig.stored.lock().unwrap().len(), 0);
    }

    #[gpui::test]
    fn saving_stores_trimmed_values_under_the_engines_id_and_puts_it_live(cx: &mut TestAppContext) {
        let rig = rig_with(cx, Some(()), Ok(()));

        for kind in ENGINES {
            fill(&rig, kind, PADDED_VALUE, cx);
            edit_key(&rig.view, kind, cx);
            save_and_settle(&rig, kind, cx);

            let (id, json) = rig.stored.lock().unwrap().last().cloned().unwrap();
            assert_eq!(id, kind.credential_id());
            assert!(json.contains(TRIMMED_VALUE), "{kind:?} stored {json}");
            assert!(!json.contains(PADDED_VALUE), "{kind:?} stored {json}");
            let (registered, clean) = rig.view.read_with(cx, |this, _| {
                (is_registered(this, kind), !dirty(this, kind))
            });
            assert_eq!((registered, clean), (true, true), "{kind:?}");
        }
        let mut heard = rig.registered.lock().unwrap().clone();
        heard.sort_by(|a, b| a.0.cmp(&b.0));
        let speak = rig.speak.as_ref().unwrap();
        pump_until(&rig.rt, || {
            speak.registered_engine_ids().len() == ENGINES.len()
        });
        assert_eq!(heard, speak.registered_engine_ids());
        assert_eq!(heard.len(), ENGINES.len());
    }

    #[gpui::test]
    fn a_failed_store_toasts_keeps_the_form_dirty_and_goes_live_nowhere(cx: &mut TestAppContext) {
        let rig = rig_with(cx, Some(()), Err(STORE_FAILURE));
        let kind = CloudEngineKind::OpenAI;
        fill(&rig, kind, PADDED_VALUE, cx);
        edit_key(&rig.view, kind, cx);

        save_and_settle(&rig, kind, cx);

        let (registered, still_dirty) = rig
            .view
            .read_with(cx, |this, _| (is_registered(this, kind), dirty(this, kind)));
        let toasts = toast_kinds(cx);
        assert_eq!(
            (
                registered,
                still_dirty,
                rig.registered.lock().unwrap().len()
            ),
            (false, true, 0)
        );
        assert!(
            matches!(toasts.as_slice(), [(ToastKind::Error, _)]),
            "{toasts:?}"
        );
    }

    #[gpui::test]
    fn saving_with_no_registry_stores_without_claiming_the_engine_is_live(cx: &mut TestAppContext) {
        let rig = rig_with(cx, None, Ok(()));
        let kind = CloudEngineKind::ElevenLabs;
        fill(&rig, kind, PADDED_VALUE, cx);

        save_and_settle(&rig, kind, cx);

        let registered = rig.view.read_with(cx, |this, _| is_registered(this, kind));
        assert_eq!(
            (
                registered,
                rig.stored.lock().unwrap().len(),
                rig.registered.lock().unwrap().len()
            ),
            (false, 1, 0)
        );
    }

    fn repo_answering(stored: Result<Option<&'static str>, ()>) -> MockCredentialsRepo {
        let mut repo = MockCredentialsRepo::new();
        repo.expect_load().returning(move |_| {
            stored
                .map(|json| json.map(str::to_owned))
                .map_err(|()| StorageError::Connection {
                    reason: "database is locked".to_owned(),
                })
        });
        repo
    }

    #[tokio::test]
    async fn stored_region_reads_the_region_and_treats_its_absence_as_none() {
        for (stored, expected) in [
            (None, None),
            (Some(r#"{"api_key":"k"}"#), None),
            (Some(r#"{"region":null}"#), None),
            (Some(r#"{"region":"eu-west-1"}"#), Some("eu-west-1")),
        ] {
            let repo = repo_answering(Ok(stored));
            let region = stored_region(&repo, CloudEngineKind::Polly).await;
            assert_eq!(region.ok().flatten().as_deref(), expected, "{stored:?}");
        }
    }

    #[tokio::test]
    async fn stored_region_reports_corrupt_json_and_a_storage_failure() {
        for repo in [
            repo_answering(Ok(Some("not json"))),
            repo_answering(Err(())),
        ] {
            assert!(stored_region(&repo, CloudEngineKind::Azure).await.is_err());
        }
    }
}
