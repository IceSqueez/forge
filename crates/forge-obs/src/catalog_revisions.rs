use forge_platform_core::{CollectionRevisionSignal, CollectionRevisions, DetailSection};

use crate::mirror::ObsMirror;

#[derive(Debug, Default)]
pub(crate) struct CatalogRevisions {
    options: CollectionRevisionSignal,
    content: CollectionRevisionSignal,
}

pub(crate) struct CatalogBaseline {
    lists: u64,
    rows: Vec<DetailSection>,
}

impl CatalogBaseline {
    pub(crate) fn of(mirror: &ObsMirror) -> Self {
        Self {
            lists: mirror.option_fingerprint(),
            rows: mirror.content_rows(),
        }
    }
}

impl CatalogRevisions {
    pub(crate) fn options(&self) -> CollectionRevisions {
        self.options.subscribe()
    }

    pub(crate) fn content(&self) -> CollectionRevisions {
        self.content.subscribe()
    }

    pub(crate) fn lists_changed(&self) {
        self.options.bump();
        self.content.bump();
    }

    pub(crate) fn rows_changed(&self) {
        self.content.bump();
    }

    pub(crate) fn announce_difference(&self, mirror: &ObsMirror, baseline: &CatalogBaseline) {
        if mirror.option_fingerprint() != baseline.lists {
            self.lists_changed();
        } else if mirror.content_rows() != baseline.rows {
            self.rows_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(revisions: &mut CollectionRevisions) -> bool {
        futures_util::FutureExt::now_or_never(revisions.changed()).is_some()
    }

    #[test]
    fn a_reconcile_that_found_nothing_new_moves_no_revision() {
        let mirror = ObsMirror::default();
        mirror.edit(|catalog| catalog.scenes = vec!["Main".to_owned()]);
        let signals = CatalogRevisions::default();
        let (mut options, mut content) = (signals.options(), signals.content());

        signals.announce_difference(&mirror, &CatalogBaseline::of(&mirror));

        assert!(!moved(&mut options));
        assert!(!moved(&mut content));
    }

    #[test]
    fn a_reconcile_that_changed_a_list_moves_both_revisions() {
        let mirror = ObsMirror::default();
        let signals = CatalogRevisions::default();
        let (mut options, mut content) = (signals.options(), signals.content());
        let baseline = CatalogBaseline::of(&mirror);

        mirror.edit(|catalog| catalog.scenes = vec!["Main".to_owned()]);
        signals.announce_difference(&mirror, &baseline);

        assert!(moved(&mut options));
        assert!(moved(&mut content));
    }

    #[test]
    fn a_reconcile_that_changed_only_a_row_moves_only_the_content_revision() {
        use crate::source::SourceInfo;

        let mirror = ObsMirror::default();
        mirror.edit(|catalog| {
            catalog.scenes = vec!["Main".to_owned()];
            catalog.current_scene = Some("Main".to_owned());
            catalog.sources.insert(
                "Main".to_owned(),
                vec![SourceInfo {
                    name: "Camera".to_owned(),
                    visible: true,
                    locked: false,
                    kind: None,
                    audio_db: None,
                }],
            );
        });
        let signals = CatalogRevisions::default();
        let (mut options, mut content) = (signals.options(), signals.content());
        let baseline = CatalogBaseline::of(&mirror);

        mirror.edit(|catalog| {
            if let Some(sources) = catalog.sources.get_mut("Main") {
                sources[0].locked = true;
            }
        });
        signals.announce_difference(&mirror, &baseline);

        assert!(!moved(&mut options), "a lock is on no picker list");
        assert!(moved(&mut content));
    }
}
