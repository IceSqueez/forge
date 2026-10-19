use std::rc::Rc;

use forge_components::{
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize, Radius, Spacing, body_family,
    destructive_button, failure_note, ghost_button, icon, modal, modal_width, mono_family, radius,
    spacing, tr,
};
use forge_transfer::BundleSummary;
use gpui::{AnyElement, App, ClickEvent, Rgba, Window, div, prelude::*, px};

use crate::restore_problem::RestoreProblem;
use crate::scheduled_run_labels::{local_stamp, system_offset_at};

const UNKNOWN_FACT: &str = "-";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfirmFacts {
    pub created: String,
    pub forge_version: String,
    pub schema: String,
}

impl ConfirmFacts {
    pub(crate) fn of(summary: &BundleSummary) -> Self {
        match summary {
            BundleSummary::Encrypted(manifest) => Self {
                created: local_stamp(manifest.created_at, system_offset_at(manifest.created_at)),
                forge_version: manifest.forge_version.clone(),
                schema: manifest.schema_version.to_string(),
            },
            BundleSummary::LegacyUnencrypted => Self {
                created: UNKNOWN_FACT.to_owned(),
                forge_version: UNKNOWN_FACT.to_owned(),
                schema: UNKNOWN_FACT.to_owned(),
            },
        }
    }
}

pub(crate) struct ConfirmCard<'a> {
    pub facts: &'a ConfirmFacts,
    pub recording: bool,
    pub problem: Option<&'a RestoreProblem>,
}

pub(crate) fn fact(label: String, value: &str, palette: &ForgePalette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(label.to_uppercase()),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(value.to_owned()),
        )
        .into_any_element()
}

pub(crate) fn statement(
    glyph: Icon,
    tint: Rgba,
    text: String,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(spacing(Spacing::Xs, density))
        .child(icon(glyph, FONT_XS, tint))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(text),
        )
        .into_any_element()
}

impl ConfirmCard<'_> {
    pub(crate) fn render(
        self,
        palette: &ForgePalette,
        density: Density,
        on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_confirm: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let on_cancel = Rc::new(on_cancel);
        let cancel_from_footer = Rc::clone(&on_cancel);
        let facts = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .bg(palette.surface_overlay)
            .child(fact(
                tr!("restore_confirm_created"),
                &self.facts.created,
                palette,
            ))
            .child(fact(
                tr!("restore_confirm_forge_version"),
                &self.facts.forge_version,
                palette,
            ))
            .child(fact(
                tr!("restore_confirm_schema"),
                &self.facts.schema,
                palette,
            ));

        let statements = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(statement(
                Icon::Database,
                palette.success,
                tr!("restore_confirm_snapshot_kept"),
                palette,
                density,
            ))
            .child(statement(
                Icon::Refresh,
                palette.info,
                tr!("restore_confirm_closes"),
                palette,
                density,
            ))
            .child(statement(
                Icon::Key,
                palette.warning,
                tr!("restore_confirm_relogin"),
                palette,
                density,
            ))
            .child(statement(
                Icon::AlertTriangle,
                palette.random,
                tr!("restore_confirm_one_machine"),
                palette,
                density,
            ));

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(facts)
            .child(statements);
        if let Some(problem) = self.problem {
            body = body.child(failure_note(&problem.message(), palette));
        }

        let confirm_label = if self.recording {
            tr!("restore_confirm_btn_busy")
        } else {
            tr!("restore_confirm_btn")
        };
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(
                ghost_button(tr!("common_cancel"), palette)
                    .disabled(self.recording)
                    .on_click("restore-confirm-cancel", move |event, window, cx| {
                        cancel_from_footer(event, window, cx)
                    }),
            )
            .child(
                destructive_button(confirm_label, palette)
                    .busy(self.recording)
                    .on_click("restore-confirm-apply", on_confirm),
            );

        modal(tr!("restore_confirm_title"), body, palette)
            .header_icon(Icon::History, palette.warning)
            .subtitle(tr!("restore_confirm_subtitle"))
            .width(modal_width(ModalSize::Sm))
            .footer(footer)
            .on_close("restore-confirm-close", move |event, window, cx| {
                on_cancel(event, window, cx)
            })
            .into_any_element()
    }
}
