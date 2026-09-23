use std::sync::{Arc, PoisonError, RwLock};

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, ControlledPlayback, PcmBuffer, PlaybackHandle};

/// Reads the installed sink once per playback call, so an install reaches the next clip and leaves a running one untouched.
pub struct RoutedSink {
    current: RwLock<Arc<dyn AudioSink>>,
}

impl RoutedSink {
    pub fn new(initial: Arc<dyn AudioSink>) -> Self {
        Self {
            current: RwLock::new(initial),
        }
    }

    pub fn install(&self, sink: Arc<dyn AudioSink>) {
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = sink;
    }

    fn current(&self) -> Arc<dyn AudioSink> {
        Arc::clone(&self.current.read().unwrap_or_else(PoisonError::into_inner))
    }
}

#[async_trait]
impl AudioSink for RoutedSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        self.current().play(buffer).await
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        self.current().play_stoppable(buffer).await
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        self.current().play_controlled(buffer).await
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::panic::AssertUnwindSafe;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::Notify;
    use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

    use super::*;
    use crate::test_support::{RecordingSink, tone};

    struct GatedSink {
        started: UnboundedSender<()>,
        release: Notify,
        plays: AtomicUsize,
    }

    impl GatedSink {
        fn new(started: UnboundedSender<()>) -> Self {
            Self {
                started,
                release: Notify::new(),
                plays: AtomicUsize::new(0),
            }
        }

        fn plays(&self) -> usize {
            self.plays.load(Ordering::SeqCst)
        }

        fn release(&self) {
            self.release.notify_one();
        }
    }

    #[async_trait]
    impl AudioSink for GatedSink {
        async fn play(&self, _: PcmBuffer) -> Result<(), AudioError> {
            self.plays.fetch_add(1, Ordering::SeqCst);
            let _ = self.started.send(());
            self.release.notified().await;
            Ok(())
        }
    }

    /// Why: replacing the held sink drops it under the write guard, which is the one
    /// unwind point that can poison this lock.
    struct PanicsWhenDropped;

    #[async_trait]
    impl AudioSink for PanicsWhenDropped {
        async fn play(&self, _: PcmBuffer) -> Result<(), AudioError> {
            Ok(())
        }
    }

    impl Drop for PanicsWhenDropped {
        fn drop(&mut self) {
            panic!("the replaced sink refuses to go quietly");
        }
    }

    #[tokio::test]
    async fn an_install_leaves_a_playback_already_under_way_on_the_earlier_sink() {
        let (started, mut starts) = unbounded_channel();
        let playing_sink = Arc::new(GatedSink::new(started));
        let installed = RecordingSink::new();
        let routed = Arc::new(RoutedSink::new(
            Arc::clone(&playing_sink) as Arc<dyn AudioSink>
        ));

        let in_flight = tokio::spawn({
            let routed = Arc::clone(&routed);
            async move { routed.play(tone()).await }
        });
        starts.recv().await.expect("the first sink began playing");

        routed.install(Arc::clone(&installed) as Arc<dyn AudioSink>);
        playing_sink.release();
        in_flight
            .await
            .expect("the playback task joins")
            .expect("the playback finishes");

        assert_eq!(playing_sink.plays(), 1);
        assert_eq!(installed.calls(), 0);
    }

    #[tokio::test]
    async fn every_playback_method_reaches_the_currently_installed_sink() {
        let replaced = RecordingSink::new();
        let installed = RecordingSink::new();
        let routed = RoutedSink::new(Arc::clone(&replaced) as Arc<dyn AudioSink>);

        routed.install(Arc::clone(&installed) as Arc<dyn AudioSink>);
        routed.play(tone()).await.expect("play reaches a sink");
        routed
            .play_stoppable(tone())
            .await
            .expect("play_stoppable reaches a sink");
        routed
            .play_controlled(tone())
            .await
            .expect("play_controlled reaches a sink");

        assert_eq!(
            (
                installed.plays(),
                installed.stoppables(),
                installed.controlled()
            ),
            (1, 1, 1)
        );
        assert_eq!(replaced.calls(), 0);
    }

    #[tokio::test]
    async fn playback_still_reaches_a_sink_after_the_lock_is_poisoned() {
        let routed = RoutedSink::new(Arc::new(PanicsWhenDropped) as Arc<dyn AudioSink>);
        let installed = RecordingSink::new();

        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let unwound = std::panic::catch_unwind(AssertUnwindSafe(|| {
            routed.install(Arc::clone(&installed) as Arc<dyn AudioSink>);
        }));
        std::panic::set_hook(quiet);

        assert!(
            unwound.is_err(),
            "the replaced sink did not poison the lock"
        );
        routed
            .play(tone())
            .await
            .expect("playback survives the poisoned lock");
        assert_eq!(installed.plays(), 1);
    }
}
