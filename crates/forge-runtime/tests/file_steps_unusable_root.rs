use forge_events::test_support::NullPublisher;
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::{
    CoreFileDeleteRunner, CoreFileListRunner, CoreFileReadRunner, CoreFileWriteRunner,
};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, Variant};

#[tokio::test]
async fn every_file_step_fails_when_the_files_folder_cannot_be_created() {
    let base = tempfile::tempdir().unwrap();
    let not_a_directory = base.path().join("plain-file");
    std::fs::write(&not_a_directory, "occupied").unwrap();
    let unusable = not_a_directory.join("assets");
    let mut config = SubActionConfig::new();
    config.insert("path".to_owned(), Variant::String("note.txt".to_owned()));
    config.insert("content".to_owned(), Variant::String("hello".to_owned()));
    config.insert("target_var".to_owned(), Variant::String("out".to_owned()));
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);

    let outcomes = [
        CoreFileReadRunner::new(unusable.clone())
            .execute(&config, &ctx)
            .await
            .0
            .outcome,
        CoreFileWriteRunner::new(unusable.clone())
            .execute(&config, &ctx)
            .await
            .0
            .outcome,
        CoreFileListRunner::new(unusable.clone())
            .execute(&config, &ctx)
            .await
            .0
            .outcome,
        CoreFileDeleteRunner::new(unusable.clone())
            .execute(&config, &ctx)
            .await
            .0
            .outcome,
    ];

    for outcome in outcomes {
        assert!(
            matches!(&outcome, SubActionOutcome::Failed(reason) if reason.code_or_text() == "core.file_io_failed"),
            "{outcome:?}"
        );
    }
}
