use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use cpal::{FromSample, Sample};

use crate::device::samples::write_frames;
use crate::sink::playback::PlaybackControl;

pub(crate) struct OutputWatch {
    pub(crate) control: Arc<PlaybackControl>,
    pub(crate) failure: OnceLock<String>,
    drained: AtomicBool,
    hiccups: AtomicU64,
}

impl OutputWatch {
    pub(crate) fn new(control: Arc<PlaybackControl>) -> Arc<Self> {
        Arc::new(Self {
            control,
            failure: OnceLock::new(),
            drained: AtomicBool::new(false),
            hiccups: AtomicU64::new(0),
        })
    }

    pub(crate) fn fail(&self, reason: String) {
        if self.failure.set(reason).is_ok() {
            self.control.wake_worker();
        }
    }

    pub(crate) fn note_hiccup(&self) {
        self.hiccups.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn hiccups(&self) -> u64 {
        self.hiccups.load(Ordering::Relaxed)
    }

    fn mark_drained(&self) {
        if !self.drained.swap(true, Ordering::AcqRel) {
            self.control.wake_worker();
        }
    }

    pub(crate) fn is_drained(&self) -> bool {
        self.drained.load(Ordering::Acquire)
    }
}

pub(crate) struct PlaybackFeed {
    pub(crate) samples: Vec<i16>,
    pub(crate) cursor: usize,
    pub(crate) channels: usize,
    pub(crate) tail_left: usize,
    pub(crate) watch: Arc<OutputWatch>,
}

impl PlaybackFeed {
    pub(crate) fn fill<T: Sample + FromSample<i16>>(&mut self, data: &mut [T]) {
        if self.watch.control.is_stopped() || self.watch.control.is_paused() {
            data.fill(T::EQUILIBRIUM);
            return;
        }
        let queued = self.samples.len() - self.cursor;
        let available = data.len().min(queued);
        let whole_frames = available - available % self.channels;
        let block = &self.samples[self.cursor..self.cursor + whole_frames];
        write_frames(block, data);
        self.cursor += whole_frames;
        if self.cursor == self.samples.len() {
            self.tail_left = self.tail_left.saturating_sub(data.len() - whole_frames);
            if self.tail_left == 0 {
                self.watch.mark_drained();
            }
        }
    }
}

pub(crate) fn wait_for_completion(watch: &OutputWatch, stall_limit: Duration) {
    let control = &watch.control;
    let mut active = Duration::ZERO;
    loop {
        if control.is_stopped() || watch.failure.get().is_some() || watch.is_drained() {
            return;
        }
        if control.is_paused() {
            std::thread::park();
            continue;
        }
        let left = stall_limit.saturating_sub(active);
        if left.is_zero() {
            return;
        }
        let parked_at = Instant::now();
        std::thread::park_timeout(left);
        if !control.is_paused() {
            active += parked_at.elapsed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG_STALL: Duration = Duration::from_secs(600);
    const WAIT_HANG_GUARD: Duration = Duration::from_secs(60);

    const TAIL_SAMPLES: usize = 8;

    fn feed_with(samples: &[i16], stopped: bool, paused: bool) -> PlaybackFeed {
        let control = PlaybackControl::new();
        if stopped {
            control.stop();
        }
        if paused {
            control.pause();
        }
        PlaybackFeed {
            samples: samples.to_vec(),
            cursor: 0,
            channels: 1,
            tail_left: TAIL_SAMPLES,
            watch: OutputWatch::new(control),
        }
    }

    #[test]
    fn a_block_takes_only_whole_frames_and_the_next_one_resumes_after_them() {
        let mut feed = feed_with(&[1, 2, 3, 4], false, false);
        feed.channels = 2;
        let mut block = [9_i16; 3];

        feed.fill(&mut block);
        assert_eq!(block, [1, 2, 0], "half a frame was played");

        feed.fill(&mut block);
        assert_eq!(block, [3, 4, 0], "the next block lost its place");
    }

    #[test]
    fn a_paused_or_stopped_feed_writes_silence_without_draining_the_queue() {
        let queued = [i16::MAX, i16::MAX, i16::MAX];
        for (stopped, paused) in [(true, false), (false, true), (true, true)] {
            let mut feed = feed_with(&queued, stopped, paused);
            let mut out = vec![u16::MAX; 8];

            feed.fill(&mut out);

            assert!(
                out.iter().all(|slot| *slot == 32_768),
                "stopped={stopped} paused={paused}: wrote audio instead of silence"
            );
            assert_eq!(
                feed.cursor, 0,
                "stopped={stopped} paused={paused}: queued audio was consumed"
            );
        }
    }

    fn idle_watch() -> Arc<OutputWatch> {
        OutputWatch::new(PlaybackControl::new())
    }

    fn assert_the_wait_returns(watch: &Arc<OutputWatch>) {
        let (returned_tx, returned_rx) = std::sync::mpsc::channel();
        let waited_on = Arc::clone(watch);
        std::thread::spawn(move || {
            wait_for_completion(&waited_on, LONG_STALL);
            returned_tx.send(()).unwrap();
        });

        assert!(
            returned_rx.recv_timeout(WAIT_HANG_GUARD).is_ok(),
            "the wait sat out its stall limit instead of ending at once"
        );
    }

    #[test]
    fn the_playback_wait_ends_as_soon_as_the_stream_reports_a_failure() {
        let watch = idle_watch();
        watch.fail("device unplugged".to_owned());

        assert_the_wait_returns(&watch);
    }

    #[test]
    fn the_playback_wait_ends_once_the_feed_has_rendered_its_silent_tail() {
        let watch = idle_watch();
        let mut feed = PlaybackFeed {
            samples: vec![1; 4],
            cursor: 0,
            channels: 1,
            tail_left: TAIL_SAMPLES,
            watch: Arc::clone(&watch),
        };
        let mut block = [0_i16; 4];

        feed.fill(&mut block);
        assert!(!watch.is_drained(), "the clip body is not the tail");
        feed.fill(&mut block);
        assert!(!watch.is_drained(), "half the tail is not the whole tail");
        feed.fill(&mut block);

        assert!(watch.is_drained());
        assert_the_wait_returns(&watch);
    }

    #[test]
    fn the_first_failure_is_the_one_reported() {
        let watch = idle_watch();

        watch.fail("device unplugged".to_owned());
        watch.fail("stream invalidated".to_owned());

        assert_eq!(
            watch.failure.get().map(String::as_str),
            Some("device unplugged")
        );
    }

    #[test]
    fn the_playback_wait_gives_up_on_a_stream_that_never_asks_for_audio() {
        let watch = idle_watch();

        wait_for_completion(&watch, Duration::ZERO);

        assert!(!watch.is_drained());
    }

    #[test]
    fn the_playback_wait_ends_when_stop_is_requested_with_audio_still_queued() {
        let watch = idle_watch();
        watch.control.stop();

        assert_the_wait_returns(&watch);

        assert!(!watch.is_drained());
    }
}
