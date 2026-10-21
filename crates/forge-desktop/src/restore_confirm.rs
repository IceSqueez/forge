use std::rc::Rc;

use forge_components::{
    DateShape, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize, Radius, Spacing,
    body_family, destructive_button, failure_note, fmt_display, ghost_button, icon, modal,
    modal_width, mono_family, radius, spacing, tr,
};
use forge_transfer::{BackupCategory, BundleSummary};
use gpui::{AnyElement, App, ClickEvent, Rgba, Window, div, prelude::*, px};

use crate::backup_schedule::category_label;
use crate::restore_problem::RestoreProblem;

const UNKNOWN_FACT: &str = "-";
const PART_SEPARATOR: &str = ", ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestoreScope {
    pub replaced: Vec<BackupCategory>,
    pub kept: Vec<BackupCategory>,
}

impl RestoreScope {
    pub(crate) fn of(included: &[BackupCategory]) -> Option<Self> {
        let (replaced, kept): (Vec<_>, Vec<_>) = BackupCategory::ALL
            .into_iter()
            .partition(|category| included.contains(category));
        (!kept.is_empty()).then_some(Self { replaced, kept })
    }
}

fn joined_labels(categories: &[BackupCategory]) -> String {
    categories
        .iter()
        .map(|category| category_label(*category))
        .collect::<Vec<_>>()
        .join(PART_SEPARATOR)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfirmFacts {
    pub created: String,
    pub forge_version: String,
    pub schema: String,
    pub scope: Option<RestoreScope>,
}

impl ConfirmFacts {
    pub(crate) fn of(summary: &BundleSummary) -> Self {
        match summary {
            BundleSummary::Encrypted(manifest) => Self {
                created: fmt_display(DateShape::DateTime, manifest.created_at),
                forge_version: manifest.forge_version.clone(),
                schema: manifest.schema_version.to_string(),
                scope: RestoreScope::of(&manifest.categories),
            },
            BundleSummary::LegacyUnencrypted => Self {
                created: UNKNOWN_FACT.to_owned(),
                forge_version: UNKNOWN_FACT.to_owned(),
                schema: UNKNOWN_FACT.to_owned(),
                scope: None,
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

        let mut statements = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        if let Some(scope) = &self.facts.scope {
            statements = statements
                .child(statement(
                    Icon::Refresh,
                    palette.warning,
                    tr!(
                        "restore_confirm_scope_replaced",
                        parts = joined_labels(&scope.replaced)
                    ),
                    palette,
                    density,
                ))
                .child(statement(
                    Icon::Lock,
                    palette.success,
                    tr!(
                        "restore_confirm_scope_kept",
                        parts = joined_labels(&scope.kept)
                    ),
                    palette,
                    density,
                ));
        }
        let statements = statements
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

        let subtitle = if self.facts.scope.is_some() {
            tr!("restore_confirm_subtitle_scoped")
        } else {
            tr!("restore_confirm_subtitle")
        };
        modal(tr!("restore_confirm_title"), body, palette)
            .header_icon(Icon::History, palette.warning)
            .subtitle(subtitle)
            .width(modal_width(ModalSize::Sm))
            .footer(footer)
            .on_close("restore-confirm-close", move |event, window, cx| {
                on_cancel(event, window, cx)
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use forge_components::tr;
    use forge_storage::Language;
    use forge_transfer::{BackupCategory, BundleSummary};

    use super::{ConfirmFacts, RestoreScope, joined_labels};
    use crate::backup_schedule::category_label;
    use crate::i18n::install_language;

    #[test]
    fn a_bundle_holding_every_part_needs_no_replaced_and_kept_split() {
        assert_eq!(RestoreScope::of(&BackupCategory::ALL), None);
    }

    #[test]
    fn a_partial_bundle_splits_every_part_into_replaced_or_kept_in_canonical_order() {
        let scope = RestoreScope::of(&[BackupCategory::Overlays, BackupCategory::Database]);

        assert_eq!(
            scope,
            Some(RestoreScope {
                replaced: vec![BackupCategory::Database, BackupCategory::Overlays],
                kept: vec![
                    BackupCategory::Media,
                    BackupCategory::Voices,
                    BackupCategory::Assets,
                ],
            })
        );
    }

    #[test]
    fn a_legacy_bundle_shows_no_scope() {
        assert_eq!(
            ConfirmFacts::of(&BundleSummary::LegacyUnencrypted).scope,
            None
        );
    }

    #[test]
    fn scope_lines_name_exactly_their_own_parts_in_each_locale() {
        let replaced = [BackupCategory::Database, BackupCategory::Media];
        let kept = [
            BackupCategory::Voices,
            BackupCategory::Overlays,
            BackupCategory::Assets,
        ];
        for lang in [Language::En, Language::Uk] {
            install_language(lang);
            let names = |line: &str, parts: &[BackupCategory]| {
                parts
                    .iter()
                    .all(|part| line.contains(&category_label(*part)))
            };
            let unmarked = |text: String| text.replace(['\u{2068}', '\u{2069}'], "");
            let replaced_line = unmarked(tr!(
                "restore_confirm_scope_replaced",
                parts = joined_labels(&replaced)
            ));
            let kept_line = unmarked(tr!(
                "restore_confirm_scope_kept",
                parts = joined_labels(&kept)
            ));

            assert!(
                names(&replaced_line, &replaced)
                    && !kept
                        .iter()
                        .any(|part| replaced_line.contains(&category_label(*part)))
                    && names(&kept_line, &kept)
                    && !replaced
                        .iter()
                        .any(|part| kept_line.contains(&category_label(*part)))
                    && !tr!("restore_confirm_subtitle_scoped").starts_with("restore_confirm_"),
                "{lang:?}: {replaced_line} / {kept_line}"
            );
        }
    }
}
