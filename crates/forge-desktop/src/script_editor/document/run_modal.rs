use super::input_parsing::{input_syntax_example, parse_input_to_variant};
use forge_components::{
    ActivePresentation, FONT_SM, FONT_XS, ForgePalette, InputEvent, ModalSize, OverlayPosition,
    Radius, Spacing, TextInput, body_family, ghost_button, modal, overlay, primary_button, radius,
    spacing, tr, with_alpha,
};
use forge_types::{ArgStack, ScriptId, VariantKind};
use gpui::prelude::*;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, SharedString, Subscription,
    Window, div,
};

pub(super) struct RunInput {
    name: SharedString,
    label: SharedString,
    kind: VariantKind,
    input: Entity<TextInput>,
    _sub: Subscription,
}

pub(super) struct RunDraft {
    pub(super) script_id: ScriptId,
    pub(super) script_name: String,
    pub(super) args: ArgStack,
}

pub(super) enum RunModalEvent {
    Submit(RunDraft),
    Cancel,
}

pub(super) struct RunModal {
    pub(super) title: SharedString,
    pub(super) script_id: ScriptId,
    pub(super) script_name: String,
    pub(super) inputs: Vec<RunInput>,
    pub(super) error: Option<SharedString>,
    pub(super) running: bool,
}

impl EventEmitter<RunModalEvent> for RunModal {}

impl RunModal {
    pub(super) fn new(
        title: SharedString,
        script_id: ScriptId,
        script_name: String,
        fields: Vec<(String, VariantKind)>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let inputs = fields
            .iter()
            .map(|(n, k)| Self::build_run_input(n, *k, palette, cx))
            .collect();
        RunModal {
            title,
            script_id,
            script_name,
            inputs,
            error: None,
            running: false,
        }
    }

    fn build_run_input(
        name: &str,
        kind: VariantKind,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> RunInput {
        let label = kind.label().to_lowercase();
        let placeholder = match input_syntax_example(kind) {
            Some(example) => tr!(
                "script_editor_run_input_placeholder_example",
                label = label.as_str(),
                example = example
            ),
            None => tr!(
                "script_editor_run_input_placeholder",
                label = label.as_str()
            ),
        };
        let input = cx.new(|cx| {
            TextInput::new(placeholder, cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
                .on_surface()
                .static_chrome(palette.border_input, Radius::Sm)
        });
        let sub = cx.subscribe(&input, |this, _f, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                this.error = None;
                cx.notify();
            }
        });
        RunInput {
            name: name.to_owned().into(),
            label: label.into(),
            kind,
            input,
            _sub: sub,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let raws: Vec<(String, VariantKind, String)> = self
            .inputs
            .iter()
            .map(|f| {
                (
                    f.name.to_string(),
                    f.kind,
                    f.input.read(cx).content().trim().to_owned(),
                )
            })
            .collect();

        let mut args = ArgStack::new();
        for (fname, kind, raw) in &raws {
            match parse_input_to_variant(fname, *kind, raw) {
                Ok(v) => args = args.set(fname.clone(), v),
                Err(rejection) => {
                    self.error = Some(rejection.user_message().into());
                    cx.notify();
                    return;
                }
            }
        }
        cx.emit(RunModalEvent::Submit(RunDraft {
            script_id: self.script_id,
            script_name: self.script_name.clone(),
            args,
        }));
    }

    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(RunModalEvent::Cancel);
    }
}

impl Render for RunModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        let palette = &palette;
        let modal_state = &*self;

        let mut body = div().flex().flex_col().gap(spacing(Spacing::Sm, density));
        for field in &modal_state.inputs {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(format!("{}: {}", field.name, field.label)),
                    )
                    .child(field.input.clone()),
            );
        }
        if let Some(err) = &modal_state.error {
            body = body.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.danger)
                    .child(err.clone()),
            );
        }

        let submit: AnyElement = if modal_state.running {
            div()
                .flex()
                .items_center()
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Md, density))
                .rounded(radius(Radius::Sm))
                .bg(with_alpha(palette.success, 0.5))
                .child(
                    div()
                        .font_family(body_family())
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(FONT_XS)
                        .text_color(palette.shell)
                        .child(tr!("script_editor_running")),
                )
                .into_any_element()
        } else {
            primary_button(tr!("script_editor_run"), palette)
                .on_click(
                    "script-run-submit",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
                )
                .into_any_element()
        };

        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                ghost_button(tr!("script_editor_run_modal_cancel"), palette).on_click(
                    "script-run-cancel",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                ),
            )
            .child(submit);

        let card = modal(modal_state.title.clone(), body, palette)
            .size(ModalSize::Md)
            .footer(footer)
            .on_close(
                "script-run-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("script-run-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element()
    }
}
