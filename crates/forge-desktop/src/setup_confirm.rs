use std::rc::Rc;

use forge_components::{
    Density, ForgePalette, Icon, ModalSize, Radius, Spacing, destructive_button, failure_note,
    ghost_button, modal, modal_width, radius, spacing, tr,
};
use forge_transfer::{PackItemKind, SetupDocuments};
use gpui::{AnyElement, App, ClickEvent, Window, div, prelude::*, px};

use crate::pack_import_modal::kind_label;
use crate::restore_confirm::{fact, statement};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SetupPart {
    Item(PackItemKind),
    VoiceAliases,
    FilterRules,
    ClipHotkeys,
    Settings,
}

impl SetupPart {
    fn label(self) -> String {
        match self {
            Self::Item(kind) => kind_label(kind),
            Self::VoiceAliases => tr!("setup_confirm_part_voice_aliases"),
            Self::FilterRules => tr!("setup_confirm_part_filter_rules"),
            Self::ClipHotkeys => tr!("setup_confirm_part_clip_hotkeys"),
            Self::Settings => tr!("setup_confirm_part_settings"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SetupCount {
    pub part: SetupPart,
    pub count: usize,
}

pub(crate) fn setup_counts(documents: &SetupDocuments) -> Vec<SetupCount> {
    let configuration = &documents.configuration;
    [
        (
            SetupPart::Item(PackItemKind::Action),
            configuration.actions.len(),
        ),
        (
            SetupPart::Item(PackItemKind::Trigger),
            configuration.triggers.len(),
        ),
        (
            SetupPart::Item(PackItemKind::Queue),
            configuration.queues.len(),
        ),
        (
            SetupPart::Item(PackItemKind::Script),
            configuration.scripts.len(),
        ),
        (
            SetupPart::Item(PackItemKind::Global),
            configuration.globals.len(),
        ),
        (
            SetupPart::Item(PackItemKind::Clip),
            configuration.clips.len(),
        ),
        (
            SetupPart::Item(PackItemKind::Overlay),
            configuration.overlays.len(),
        ),
        (
            SetupPart::Item(PackItemKind::Media),
            configuration.media.len(),
        ),
        (SetupPart::VoiceAliases, documents.voices.aliases.len()),
        (SetupPart::FilterRules, documents.tts.filter_rules.len()),
        (SetupPart::ClipHotkeys, documents.clip_hotkeys.len()),
        (SetupPart::Settings, documents.settings.len()),
    ]
    .into_iter()
    .map(|(part, count)| SetupCount { part, count })
    .collect()
}

pub(crate) struct SetupConfirmCard<'a> {
    pub counts: &'a [SetupCount],
    pub applying: bool,
    pub failure: Option<&'a str>,
}

fn count_column(counts: &[SetupCount], palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .children(
            counts
                .iter()
                .map(|count| fact(count.part.label(), &count.count.to_string(), palette)),
        )
        .into_any_element()
}

impl SetupConfirmCard<'_> {
    pub(crate) fn render(
        self,
        palette: &ForgePalette,
        density: Density,
        on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_confirm: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let on_cancel = Rc::new(on_cancel);
        let cancel_from_footer = Rc::clone(&on_cancel);
        let split = self.counts.len().div_ceil(2);
        let (left, right) = self.counts.split_at(split);
        let counts = div()
            .w_full()
            .flex()
            .gap(spacing(Spacing::Md, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .bg(palette.surface_overlay)
            .child(count_column(left, palette, density))
            .child(count_column(right, palette, density));

        let statements = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(statement(
                Icon::AlertTriangle,
                palette.warning,
                tr!("setup_confirm_replaces"),
                palette,
                density,
            ))
            .child(statement(
                Icon::Key,
                palette.info,
                tr!("setup_confirm_keeps"),
                palette,
                density,
            ))
            .child(statement(
                Icon::Database,
                palette.success,
                tr!("setup_confirm_snapshot"),
                palette,
                density,
            ));

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(counts)
            .child(statements);
        if let Some(failure) = self.failure {
            body = body.child(failure_note(failure, palette));
        }

        let confirm_label = if self.applying {
            tr!("setup_confirm_btn_busy")
        } else {
            tr!("setup_confirm_btn")
        };
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(
                ghost_button(tr!("common_cancel"), palette)
                    .disabled(self.applying)
                    .on_click("setup-confirm-cancel", move |event, window, cx| {
                        cancel_from_footer(event, window, cx)
                    }),
            )
            .child(
                destructive_button(confirm_label, palette)
                    .busy(self.applying)
                    .on_click("setup-confirm-apply", on_confirm),
            );

        modal(tr!("setup_confirm_title"), body, palette)
            .header_icon(Icon::Settings, palette.warning)
            .subtitle(tr!("setup_confirm_subtitle"))
            .width(modal_width(ModalSize::Sm))
            .footer(footer)
            .on_close("setup-confirm-close", move |event, window, cx| {
                on_cancel(event, window, cx)
            })
            .into_any_element()
    }
}
