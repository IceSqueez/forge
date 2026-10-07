use forge_components::{
    Density, FONT_SM, FONT_XS, ForgePalette, InputEvent, OverlayPosition, Platform, PlatformKind,
    Spacing, TextInput, body_family, field_label, modal, mono_family, overlay, primary_button,
    secondary_button, segment, segmented, spacing, toggle, tr,
};
use forge_storage::{AliasId, VoiceAlias};
use forge_types::PlatformId;
use forge_voice::{AliasState, EngineId, VoiceId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};

use crate::presentation::ActivePresentation;

const MODAL_W: Pixels = px(440.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlatformScope {
    Any,
    Twitch,
    YouTube,
    Kick,
}

impl PlatformScope {
    pub(crate) const ALL: [PlatformScope; 4] = [
        PlatformScope::Any,
        PlatformScope::Twitch,
        PlatformScope::YouTube,
        PlatformScope::Kick,
    ];

    pub(crate) fn token(self) -> Option<&'static str> {
        match self {
            PlatformScope::Any => None,
            PlatformScope::Twitch => Some(PlatformId::Twitch.as_str()),
            PlatformScope::YouTube => Some(PlatformId::YouTube.as_str()),
            PlatformScope::Kick => Some(PlatformId::Kick.as_str()),
        }
    }

    pub(crate) fn from_token(token: &str) -> Option<PlatformScope> {
        PlatformScope::ALL
            .into_iter()
            .find(|scope| scope.token() == Some(token))
    }

    pub(crate) fn label(self) -> String {
        match self {
            PlatformScope::Any => tr!("tts_aliases_platform_any"),
            PlatformScope::Twitch => "Twitch".to_owned(),
            PlatformScope::YouTube => "YouTube".to_owned(),
            PlatformScope::Kick => "Kick".to_owned(),
        }
    }

    pub(crate) fn kind(self) -> Option<PlatformKind> {
        match self {
            PlatformScope::Any => None,
            PlatformScope::Twitch => Some(PlatformKind::Twitch),
            PlatformScope::YouTube => Some(PlatformKind::YouTube),
            PlatformScope::Kick => Some(PlatformKind::Kick),
        }
    }
}

pub(crate) fn platform_scope(platform: Platform) -> PlatformScope {
    match platform {
        Platform::Twitch => PlatformScope::Twitch,
        Platform::YouTube => PlatformScope::YouTube,
        Platform::Kick => PlatformScope::Kick,
    }
}

pub(crate) fn split_alias_key(key: &str) -> (PlatformScope, &str) {
    key.split_once(':')
        .and_then(|(token, name)| PlatformScope::from_token(token).map(|scope| (scope, name)))
        .unwrap_or((PlatformScope::Any, key))
}

pub(crate) fn alias_key(scope: PlatformScope, name: &str) -> String {
    match scope.token() {
        Some(token) => format!("{token}:{name}"),
        None => name.to_owned(),
    }
}

pub(crate) fn fmt_field(value: Option<f32>) -> String {
    value.map(|v| format!("{v}")).unwrap_or_default()
}

struct EngineOption {
    id: &'static str,
    label: &'static str,
}

const ENGINE_OPTIONS: [EngineOption; 4] = [
    EngineOption {
        id: forge_tts_core::ENGINE_ID_PIPER,
        label: "Piper",
    },
    EngineOption {
        id: forge_tts_core::ENGINE_ID_ESPEAK_NG,
        label: "eSpeak-NG",
    },
    EngineOption {
        id: forge_tts_core::ENGINE_ID_POLLY,
        label: "Amazon Polly",
    },
    EngineOption {
        id: forge_tts_core::ENGINE_ID_ELEVENLABS,
        label: "ElevenLabs",
    },
];

pub(crate) struct AliasIdentity {
    pub key: String,
    pub name: String,
    pub platform: PlatformScope,
}

#[derive(Default)]
pub(crate) struct AliasValues {
    pub engine: Option<String>,
    pub voice: String,
    pub pitch: String,
    pub rate: String,
    pub blocked: bool,
}

impl AliasValues {
    pub(crate) fn of(alias: &VoiceAlias) -> Self {
        let blocked = matches!(alias.state, AliasState::Blocked);
        Self {
            engine: Some(alias.engine_id.0.clone()),
            voice: alias.voice_id.0.clone(),
            pitch: fmt_field(alias.pitch_semitones),
            rate: fmt_field(alias.rate_multiplier),
            blocked,
        }
    }
}

pub(crate) enum AliasFormEvent {
    Submit(VoiceAlias),
    Cancel,
}

pub(crate) struct AliasForm {
    editing: Option<AliasId>,
    platform: PlatformScope,
    locked_key: Option<String>,
    viewer: Entity<TextInput>,
    voice: Entity<TextInput>,
    pitch: Entity<TextInput>,
    rate: Entity<TextInput>,
    engine: Option<String>,
    blocked: bool,
    saving: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<AliasFormEvent> for AliasForm {}

impl AliasForm {
    pub(crate) fn new(
        editing: Option<AliasId>,
        identity: Option<AliasIdentity>,
        locked: bool,
        values: AliasValues,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let (platform, viewer_name, locked_key) = match identity {
            Some(identity) => (
                identity.platform,
                identity.name,
                locked.then_some(identity.key),
            ),
            None => (PlatformScope::Any, String::new(), None),
        };
        let viewer = text_field(
            tr!("tts_aliases_form_viewer_placeholder"),
            &viewer_name,
            locked_key.is_some(),
            palette,
            cx,
        );
        let voice = text_field(
            tr!("tts_aliases_form_voice_placeholder"),
            &values.voice,
            false,
            palette,
            cx,
        );
        let pitch = text_field(
            tr!("tts_aliases_form_pitch_placeholder"),
            &values.pitch,
            false,
            palette,
            cx,
        );
        let rate = text_field(
            tr!("tts_aliases_form_rate_placeholder"),
            &values.rate,
            false,
            palette,
            cx,
        );

        let subs = [&viewer, &voice, &pitch, &rate]
            .into_iter()
            .map(|field| {
                cx.subscribe(field, |this, _input, event: &InputEvent, cx| match event {
                    InputEvent::Changed(_) => cx.notify(),
                    InputEvent::Cancelled => this.cancel(cx),
                    InputEvent::Submitted(_) => this.submit(cx),
                    InputEvent::Blurred(_) => {}
                })
            })
            .collect();

        AliasForm {
            editing,
            platform,
            locked_key,
            viewer,
            voice,
            pitch,
            rate,
            engine: values.engine,
            blocked: values.blocked,
            saving: false,
            _subs: subs,
        }
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let field = if self.locked_key.is_some() {
            &self.voice
        } else {
            &self.viewer
        };
        field.update(cx, |f, cx| f.focus(window, cx));
    }

    pub(crate) fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    pub(crate) fn set_saving(&mut self, saving: bool, cx: &mut Context<Self>) {
        self.saving = saving;
        cx.notify();
    }

    fn set_engine(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.engine = Some(id.to_owned());
        cx.notify();
    }

    fn set_platform(&mut self, platform: PlatformScope, cx: &mut Context<Self>) {
        if self.locked_key.is_some() {
            return;
        }
        self.platform = platform;
        cx.notify();
    }

    fn toggle_blocked(&mut self, cx: &mut Context<Self>) {
        self.blocked = !self.blocked;
        cx.notify();
    }

    fn is_saveable(&self, cx: &App) -> bool {
        !self.saving && !self.viewer.read(cx).content().trim().is_empty()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.is_saveable(cx) {
            return;
        }
        let alias = form_to_alias(self, cx);
        cx.emit(AliasFormEvent::Submit(alias));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(AliasFormEvent::Cancel);
    }
}

impl Render for AliasForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let title = if self.editing.is_some() {
            tr!("tts_aliases_form_title_edit")
        } else {
            tr!("tts_aliases_form_title_assign")
        };

        let viewer_field = labelled(
            tr!("tts_aliases_form_viewer_label"),
            self.viewer.clone(),
            &palette,
            density,
        );

        let locked = self.locked_key.is_some();
        let platform_segments = PlatformScope::ALL
            .into_iter()
            .map(|scope| {
                segment(
                    SharedString::from(format!(
                        "va-form-platform-{}",
                        scope.token().unwrap_or("any")
                    )),
                    scope.label(),
                    self.platform == scope,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.set_platform(scope, cx)),
                )
                .disabled(locked && self.platform != scope)
            })
            .collect();
        let platform_field = labelled(
            tr!("tts_aliases_form_platform_label"),
            div().flex().child(segmented(platform_segments, &palette)),
            &palette,
            density,
        );

        let block_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("tts_aliases_form_block_label")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("tts_aliases_form_block_desc")),
                    ),
            )
            .child(toggle(self.blocked, &palette).on_click(
                "va-form-block",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_blocked(cx)),
            ));

        let config: AnyElement = if self.blocked {
            div()
                .font_family(mono_family())
                .text_size(FONT_SM)
                .text_color(palette.text_faint)
                .child(tr!("tts_aliases_form_blocked_note"))
                .into_any_element()
        } else {
            let engine_segments = ENGINE_OPTIONS
                .iter()
                .map(|opt| {
                    let active = self.engine.as_deref() == Some(opt.id);
                    let id = opt.id;
                    segment(
                        SharedString::from(format!("va-form-eng-{id}")),
                        opt.label,
                        active,
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.set_engine(id, cx)),
                    )
                })
                .collect();
            let chips = segmented(engine_segments, &palette).wrap(spacing(Spacing::Xxs, density));
            let engine_block = labelled(
                tr!("tts_aliases_form_engine_label"),
                chips,
                &palette,
                density,
            );
            let voice_block = labelled(
                tr!("tts_aliases_form_voice_label"),
                self.voice.clone(),
                &palette,
                density,
            );
            let pitch_block = labelled(
                tr!("tts_aliases_form_pitch_label"),
                self.pitch.clone(),
                &palette,
                density,
            );
            let rate_block = labelled(
                tr!("tts_aliases_form_rate_label"),
                self.rate.clone(),
                &palette,
                density,
            );
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Sm, density))
                .child(engine_block)
                .child(voice_block)
                .child(
                    div()
                        .flex()
                        .gap(spacing(Spacing::Sm, density))
                        .child(div().flex_1().child(pitch_block))
                        .child(div().flex_1().child(rate_block)),
                )
                .into_any_element()
        };

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(platform_field)
            .child(viewer_field)
            .child(block_row)
            .child(config);

        let save_label = if self.editing.is_some() {
            tr!("common_save")
        } else {
            tr!("tts_aliases_form_create")
        };
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(secondary_button(tr!("common_cancel"), &palette).on_click(
                "va-form-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            ))
            .child(
                primary_button(save_label, &palette)
                    .disabled(!self.is_saveable(cx))
                    .on_click(
                        "va-form-save",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
                    ),
            );

        let card = modal(title, body, &palette)
            .width(MODAL_W)
            .footer(footer)
            .on_close(
                "va-form-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                overlay(card, &palette)
                    .position(OverlayPosition::Center)
                    .busy(self.saving)
                    .on_dismiss("va-form-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.cancel(cx));
                    }),
            )
            .into_any_element()
    }
}

fn labelled(
    label: impl Into<SharedString>,
    control: impl IntoElement,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    field_label(palette, label, control)
        .tone(palette.text_muted)
        .size(FONT_XS)
        .density(density)
}

fn text_field(
    placeholder: impl Into<SharedString>,
    initial: &str,
    read_only: bool,
    palette: ForgePalette,
    cx: &mut Context<AliasForm>,
) -> Entity<TextInput> {
    let initial = initial.to_owned();
    cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx)
            .with_palette(palette)
            .read_only(read_only);
        if !initial.is_empty() {
            input.set_content(initial, cx);
        }
        input
    })
}

fn form_to_alias(form: &AliasForm, cx: &App) -> VoiceAlias {
    let viewer = form.viewer.read(cx).content().trim().to_owned();
    let engine = form.engine.clone().unwrap_or_default();
    let voice = form.voice.read(cx).content().trim().to_owned();
    let pitch = form.pitch.read(cx).content().trim().parse::<f32>().ok();
    let rate = form.rate.read(cx).content().trim().parse::<f32>().ok();
    VoiceAlias {
        id: form.editing.clone().unwrap_or_default(),
        viewer_id: form
            .locked_key
            .clone()
            .unwrap_or_else(|| alias_key(form.platform, &viewer)),
        viewer_name: viewer,
        engine_id: EngineId(engine.trim().to_owned()),
        voice_id: VoiceId(voice),
        pitch_semitones: pitch,
        rate_multiplier: rate,
        state: if form.blocked {
            AliasState::Blocked
        } else {
            AliasState::Active
        },
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_components::Platform;
    use forge_storage::Language;
    use gpui::{Entity, EntityInputHandler, Subscription, TestAppContext};

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{AliasShape, alias_shape, install_presentation};

    struct Submitted {
        seen: Vec<VoiceAlias>,
        _sub: Subscription,
    }

    struct Mounted {
        form: Entity<AliasForm>,
        submitted: Entity<Submitted>,
    }

    fn identity(key: &str, name: &str, platform: PlatformScope) -> AliasIdentity {
        AliasIdentity {
            key: key.to_owned(),
            name: name.to_owned(),
            platform,
        }
    }

    fn mount(
        cx: &mut TestAppContext,
        editing: Option<AliasId>,
        identity: Option<AliasIdentity>,
        locked: bool,
        values: AliasValues,
    ) -> Mounted {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| {
            let form = cx.new(|cx| AliasForm::new(editing, identity, locked, values, cx));
            let submitted = cx.new(|cx| Submitted {
                seen: Vec::new(),
                _sub: cx.subscribe(&form, |this: &mut Submitted, _form, event, _cx| {
                    if let AliasFormEvent::Submit(alias) = event {
                        this.seen.push(alias.clone());
                    }
                }),
            });
            Mounted { form, submitted }
        })
    }

    fn submit(cx: &mut TestAppContext, mounted: &Mounted) -> Vec<AliasShape> {
        mounted.form.update(cx, |form, cx| form.submit(cx));
        cx.run_until_parked();
        mounted.submitted.read_with(cx, |submitted, _| {
            submitted.seen.iter().map(alias_shape).collect()
        })
    }

    fn type_viewer(cx: &mut TestAppContext, mounted: &Mounted, text: &str) {
        mounted.form.update(cx, |form, cx| {
            form.viewer
                .update(cx, |field, cx| field.set_content(text.to_owned(), cx))
        });
    }

    #[gpui::test]
    fn submit_keeps_a_locked_identity_key_and_recomputes_an_unlocked_one(cx: &mut TestAppContext) {
        for (locked, viewer_id) in [(true, "twitch:141981764"), (false, "twitch:alice")] {
            let mounted = mount(
                cx,
                None,
                Some(identity("twitch:141981764", "alice", PlatformScope::Twitch)),
                locked,
                AliasValues::default(),
            );

            let seen = submit(cx, &mounted);

            assert_eq!(
                seen.iter()
                    .map(|shape| (shape.1.as_str(), shape.2.as_str()))
                    .collect::<Vec<_>>(),
                [(viewer_id, "alice")],
                "locked = {locked}"
            );
        }
    }

    #[gpui::test]
    fn a_new_alias_typed_by_hand_is_keyed_by_the_trimmed_name_on_the_picked_platform(
        cx: &mut TestAppContext,
    ) {
        for (platform, viewer_id) in [
            (PlatformScope::Any, "Зірка"),
            (PlatformScope::Kick, "kick:Зірка"),
        ] {
            let mounted = mount(cx, None, None, false, AliasValues::default());
            type_viewer(cx, &mounted, "  Зірка ");
            mounted
                .form
                .update(cx, |form, cx| form.set_platform(platform, cx));

            let seen = submit(cx, &mounted);

            assert_eq!(
                seen.iter()
                    .map(|shape| (shape.1.as_str(), shape.2.as_str()))
                    .collect::<Vec<_>>(),
                [(viewer_id, "Зірка")],
                "{platform:?}"
            );
        }
    }

    #[gpui::test]
    fn a_locked_form_keeps_the_viewer_name_it_was_opened_with(cx: &mut TestAppContext) {
        for (locked, viewer_name) in [(true, "alice"), (false, "alicex")] {
            let mounted = mount(
                cx,
                None,
                Some(identity("twitch:141981764", "alice", PlatformScope::Twitch)),
                locked,
                AliasValues::default(),
            );
            let vcx = cx.add_empty_window();
            let viewer = vcx.update(|_window, cx| mounted.form.read(cx).viewer.clone());
            vcx.update(|window, cx| {
                viewer.update(cx, |field, cx| {
                    field.replace_text_in_range(None, "x", window, cx)
                })
            });

            let seen = submit(cx, &mounted);

            assert_eq!(
                seen.iter()
                    .map(|shape| shape.2.as_str())
                    .collect::<Vec<_>>(),
                [viewer_name],
                "locked = {locked}"
            );
        }
    }

    #[gpui::test]
    fn a_locked_form_keeps_its_platform_when_another_is_picked(cx: &mut TestAppContext) {
        for (locked, kept) in [(true, PlatformScope::Twitch), (false, PlatformScope::Kick)] {
            let mounted = mount(
                cx,
                None,
                Some(identity("twitch:141981764", "alice", PlatformScope::Twitch)),
                locked,
                AliasValues::default(),
            );

            mounted
                .form
                .update(cx, |form, cx| form.set_platform(PlatformScope::Kick, cx));

            assert_eq!(
                mounted.form.read_with(cx, |form, _| form.platform),
                kept,
                "locked = {locked}"
            );
        }
    }

    fn stored(state: AliasState) -> VoiceAlias {
        VoiceAlias {
            id: AliasId("a1".to_owned()),
            viewer_id: "youtube:UCx9".to_owned(),
            viewer_name: "alice".to_owned(),
            engine_id: EngineId("elevenlabs".to_owned()),
            voice_id: VoiceId("en_US-amy".to_owned()),
            pitch_semitones: Some(-2.5),
            rate_multiplier: Some(1.25),
            state,
        }
    }

    fn mount_editing(cx: &mut TestAppContext, existing: &VoiceAlias) -> Mounted {
        mount(
            cx,
            Some(existing.id.clone()),
            Some(identity(
                &existing.viewer_id,
                &existing.viewer_name,
                platform_scope(Platform::YouTube),
            )),
            true,
            AliasValues::of(existing),
        )
    }

    #[gpui::test]
    fn an_alias_opened_for_editing_submits_back_unchanged_in_either_state(cx: &mut TestAppContext) {
        for state in [AliasState::Active, AliasState::Blocked] {
            let existing = stored(state.clone());
            let mounted = mount_editing(cx, &existing);

            assert_eq!(submit(cx, &mounted), [alias_shape(&existing)], "{state:?}");
        }
    }

    #[gpui::test]
    fn unblocking_an_edited_blocked_alias_restores_its_engine_voice_pitch_and_rate(
        cx: &mut TestAppContext,
    ) {
        let mounted = mount_editing(cx, &stored(AliasState::Blocked));

        mounted.form.update(cx, |form, cx| form.toggle_blocked(cx));

        assert_eq!(
            submit(cx, &mounted),
            [alias_shape(&stored(AliasState::Active))]
        );
    }

    #[gpui::test]
    fn submit_emits_nothing_without_a_viewer_name_or_while_saving(cx: &mut TestAppContext) {
        for (label, typed, saving) in [
            ("empty viewer", "", false),
            ("blank viewer", "   ", false),
            ("saving", "alice", true),
        ] {
            let mounted = mount(cx, None, None, false, AliasValues::default());
            type_viewer(cx, &mounted, typed);
            mounted
                .form
                .update(cx, |form, cx| form.set_saving(saving, cx));

            assert!(submit(cx, &mounted).is_empty(), "{label}");
        }
    }

    #[test]
    fn alias_key_round_trips_through_split_for_every_platform_scope() {
        for scope in PlatformScope::ALL {
            for name in ["Alice", "Зірка", "user_42"] {
                let key = alias_key(scope, name);

                assert_eq!(split_alias_key(&key), (scope, name), "{key:?}");
            }
        }
    }

    #[test]
    fn split_alias_key_keeps_an_unknown_prefix_as_part_of_a_bare_name() {
        for key in ["discord:Alice", ":Alice", "Twitch:Alice"] {
            assert_eq!(split_alias_key(key), (PlatformScope::Any, key), "{key:?}");
        }
    }
}
