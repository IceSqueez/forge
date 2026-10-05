#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::sub_action_runners::{CoreFileListRunner, CoreFileWriteRunner};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, Variant};

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

#[allow(unsafe_code)]
#[tokio::test]
async fn a_files_folder_that_is_itself_a_symlink_is_used_as_the_root() {
    let base = tempfile::tempdir().unwrap();
    let real_root = base.path().join("real-files");
    let data_dir = base.path().join("data");
    std::fs::create_dir_all(&real_root).unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();
    std::os::unix::fs::symlink(&real_root, data_dir.join("assets")).unwrap();
    // SAFETY: this binary holds a single test, so no other thread reads the environment while it is written.
    unsafe {
        std::env::set_var("FORGE_DATA_DIR", &data_dir);
    }

    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    let mut write = SubActionConfig::new();
    write.insert("path".to_owned(), Variant::String("note.txt".to_owned()));
    write.insert("content".to_owned(), Variant::String("hello".to_owned()));
    let (written, _) = CoreFileWriteRunner.execute(&write, &ctx).await;
    assert_eq!(written.outcome, SubActionOutcome::Success);
    assert_eq!(
        std::fs::read_to_string(real_root.join("note.txt")).unwrap(),
        "hello"
    );

    let mut list = SubActionConfig::new();
    list.insert("path".to_owned(), Variant::String(".".to_owned()));
    let (_, listed) = CoreFileListRunner.execute(&list, &ctx).await;
    assert_eq!(
        listed.unwrap().get("file.entries").cloned(),
        Some(Variant::Array(vec![Variant::String("note.txt".to_owned())]))
    );
}
