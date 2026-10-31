use forge_events::test_support::NullPublisher;
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::sub_action_runners::{
    CoreFileDeleteRunner, CoreFileListRunner, CoreFileReadRunner, CoreFileWriteRunner,
};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, Variant};

#[expect(unsafe_code, reason = "env mutation is unsafe in edition 2024")]
#[tokio::test]
async fn every_file_step_fails_when_the_files_folder_cannot_be_created() {
    let base = tempfile::tempdir().unwrap();
    let not_a_directory = base.path().join("plain-file");
    std::fs::write(&not_a_directory, "occupied").unwrap();
    // SAFETY: this binary holds a single test, so no other thread reads the environment while it is written.
    unsafe {
        std::env::set_var("FORGE_DATA_DIR", &not_a_directory);
    }
    let mut config = SubActionConfig::new();
    config.insert("path".to_owned(), Variant::String("note.txt".to_owned()));
    config.insert("content".to_owned(), Variant::String("hello".to_owned()));
    config.insert("target_var".to_owned(), Variant::String("out".to_owned()));
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);

    let outcomes = [
        CoreFileReadRunner.execute(&config, &ctx).await.0.outcome,
        CoreFileWriteRunner.execute(&config, &ctx).await.0.outcome,
        CoreFileListRunner.execute(&config, &ctx).await.0.outcome,
        CoreFileDeleteRunner.execute(&config, &ctx).await.0.outcome,
    ];

    for outcome in outcomes {
        assert!(
            matches!(&outcome, SubActionOutcome::Failed(reason) if reason.code_or_text() == "core.file_io_failed"),
            "{outcome:?}"
        );
    }
}
