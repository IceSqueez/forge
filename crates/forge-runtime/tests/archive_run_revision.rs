use std::sync::Arc;

use forge_storage::action::MockActionRepo;
use forge_storage::{CatalogRevision, RevisingActionRepo};
use forge_types::ActionId;

#[tokio::test]
async fn an_archive_advances_the_run_revision_only_when_it_cancelled_runs() {
    for (cancelled, archived, run_revision_moves) in [
        (Some(2), true, true),
        (Some(0), true, false),
        (None, false, false),
    ] {
        let mut inner = MockActionRepo::new();
        inner
            .expect_archive_reporting_cancelled_runs()
            .returning(move |_| Ok(cancelled));
        let catalog = CatalogRevision::new();
        let runs = CatalogRevision::new();
        let repo = RevisingActionRepo::wrap_cascading_scheduled_runs(
            Arc::new(inner),
            catalog.clone(),
            runs.clone(),
        );

        let outcome = repo.archive(ActionId::new()).await.unwrap();

        assert_eq!(outcome, archived, "{cancelled:?}");
        assert_eq!(runs.current() > 0, run_revision_moves, "{cancelled:?}");
    }
}
