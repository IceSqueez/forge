use std::sync::Arc;

use crate::fan_out::FanOutSink;
use crate::remote::{PlaybackCorrelation, RemoteAudioDestination, RemoteDestinationId};
use crate::remote_sink::RemoteSink;
use crate::sink::AudioSink;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackTarget {
    pub destination: RemoteDestinationId,
    pub correlation: PlaybackCorrelation,
}

/// Builds the legs of one playback aimed at a single destination; the global route's remote leg
/// is never among them.
pub trait TargetedSinkFactory: Send + Sync {
    fn sink_for(&self, target: &PlaybackTarget) -> Arc<dyn AudioSink>;
}

pub struct RemoteLegFactory {
    remote: Arc<dyn RemoteAudioDestination>,
    local: Option<Arc<dyn AudioSink>>,
}

impl RemoteLegFactory {
    /// `local` is the monitoring leg, present only when the global route plays locally.
    pub fn new(remote: Arc<dyn RemoteAudioDestination>, local: Option<Arc<dyn AudioSink>>) -> Self {
        Self { remote, local }
    }
}

impl TargetedSinkFactory for RemoteLegFactory {
    fn sink_for(&self, target: &PlaybackTarget) -> Arc<dyn AudioSink> {
        let remote: Arc<dyn AudioSink> = Arc::new(
            RemoteSink::new(Arc::clone(&self.remote), target.destination.clone())
                .correlated(target.correlation.clone()),
        );
        match &self.local {
            Some(local) => Arc::new(FanOutSink::new(vec![Arc::clone(local), remote])),
            None => remote,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::error::AudioError;
    use crate::pcm::PcmBuffer;
    use crate::remote::{RemoteClip, RemoteClipId, RemoteCommand, RemoteDelivery, RemoteVerdict};

    const SHOWING_OVERLAY: &str = "stage-alert";
    const SHOW_TOKEN: &str = "01J9ZC4W6R7Q2N3M4K5P6S7T8V";

    #[derive(Default)]
    struct FakePage {
        delivered: Mutex<Vec<(RemoteDestinationId, Option<PlaybackCorrelation>)>>,
    }

    #[async_trait]
    impl RemoteAudioDestination for FakePage {
        async fn deliver(
            &self,
            destination: &RemoteDestinationId,
            clip: RemoteClip,
        ) -> Result<RemoteDelivery, AudioError> {
            self.delivered
                .lock()
                .unwrap()
                .push((destination.clone(), clip.correlation));
            Ok(RemoteDelivery {
                clip_id: RemoteClipId::new("clip-7"),
                live_players: 1,
            })
        }

        async fn control(
            &self,
            _: &RemoteDestinationId,
            _: &RemoteClipId,
            _: RemoteCommand,
        ) -> Result<(), AudioError> {
            Ok(())
        }

        async fn verdict(&self, _: &RemoteClipId) -> Result<RemoteVerdict, AudioError> {
            Ok(RemoteVerdict::Played)
        }
    }

    #[derive(Default)]
    struct CountingSink {
        plays: AtomicUsize,
    }

    #[async_trait]
    impl AudioSink for CountingSink {
        async fn play(&self, _: PcmBuffer) -> Result<(), AudioError> {
            self.plays.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn target() -> PlaybackTarget {
        PlaybackTarget {
            destination: RemoteDestinationId::new(SHOWING_OVERLAY),
            correlation: PlaybackCorrelation::new(SHOW_TOKEN),
        }
    }

    async fn play_through(factory: &RemoteLegFactory) {
        factory
            .sink_for(&target())
            .play_controlled(PcmBuffer::new(vec![0; 16], 1_000, 1))
            .await
            .expect("the targeted legs start")
            .await
            .expect("the targeted legs finish");
    }

    #[tokio::test]
    async fn the_remote_leg_reaches_the_target_overlay_carrying_the_show_token() {
        let page = Arc::new(FakePage::default());
        let factory = RemoteLegFactory::new(page.clone(), None);

        play_through(&factory).await;

        assert_eq!(
            *page.delivered.lock().unwrap(),
            vec![(
                RemoteDestinationId::new(SHOWING_OVERLAY),
                Some(PlaybackCorrelation::new(SHOW_TOKEN)),
            )],
            "a show's speech went to another overlay or lost the token that joins it to its show"
        );
    }

    #[tokio::test]
    async fn a_supplied_local_leg_plays_alongside_the_remote_one() {
        let page = Arc::new(FakePage::default());
        let local = Arc::new(CountingSink::default());
        let factory = RemoteLegFactory::new(page.clone(), Some(local.clone()));

        play_through(&factory).await;

        assert_eq!(
            (
                local.plays.load(Ordering::SeqCst),
                page.delivered.lock().unwrap().len()
            ),
            (1, 1),
            "the monitoring leg and the showing overlay did not each play the speech once"
        );
    }
}
