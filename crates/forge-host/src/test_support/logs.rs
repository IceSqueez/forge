use std::io;
use std::sync::{Arc, Mutex};

use forge_types::RecoverMutex;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt as _;

#[derive(Clone, Default)]
struct Console(Arc<Mutex<Vec<u8>>>);

impl io::Write for Console {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock_recover().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Console {
    type Writer = Console;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

pub fn console_line(emit: impl FnOnce()) -> String {
    let console = Console::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_ansi(false)
            .with_writer(console.clone()),
    );
    tracing::subscriber::with_default(subscriber, emit);
    let written = console.0.lock_recover().clone();
    String::from_utf8_lossy(&written).trim_end().to_owned()
}

pub fn signal_fields(line: &str) -> &str {
    line.find(" signal=").map_or("", |start| &line[start + 1..])
}

struct TracingInterestFloor;

impl tracing::Subscriber for TracingInterestFloor {
    fn register_callsite(
        &self,
        _: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::sometimes()
    }

    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        false
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, _: &tracing::Event<'_>) {}

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

pub fn install_tracing_interest_floor() {
    static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INSTALLED.get_or_init(|| {
        drop(tracing::subscriber::set_global_default(
            TracingInterestFloor,
        ));
        tracing::callsite::rebuild_interest_cache();
    });
}
