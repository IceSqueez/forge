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
    use cpal::{I24, U24};

    use super::*;

    const LONG_STALL: Duration = Duration::from_secs(600);
    const WAIT_HANG_GUARD: Duration = Duration::from_secs(60);

    const SCALE_TOLERANCE: f64 = 0.01;
    const TAIL_SAMPLES: usize = 8;

    type LevelRenderer = fn(&[i16]) -> Vec<f64>;

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

    fn rendered_levels<T: Sample + FromSample<i16>>(samples: &[i16]) -> Vec<f64>
    where
        f64: FromSample<T>,
    {
        let mut feed = feed_with(samples, false, false);
        let mut out = vec![T::EQUILIBRIUM; samples.len()];
        feed.fill(&mut out);
        out.into_iter().map(f64::from_sample).collect()
    }

    fn underrun_tail_is<T: Sample + FromSample<i16>>(queued: &[i16], expected_silence: T) -> bool {
        let mut feed = feed_with(queued, false, false);
        let mut out = vec![T::from_sample(i16::MAX); queued.len() + 4];
        feed.fill(&mut out);
        out[queued.len()..]
            .iter()
            .all(|slot| *slot == expected_silence)
    }

    #[test]
    fn every_output_format_receives_the_pcm_scaled_to_its_full_range() {
        let pcm = [i16::MAX, i16::MIN, 0, 16_384, -16_384];
        let expected = [1.0, -1.0, 0.0, 0.5, -0.5];
        let formats: [(&str, LevelRenderer); 12] = [
            ("i8", rendered_levels::<i8>),
            ("i16", rendered_levels::<i16>),
            ("i24", rendered_levels::<I24>),
            ("i32", rendered_levels::<i32>),
            ("i64", rendered_levels::<i64>),
            ("u8", rendered_levels::<u8>),
            ("u16", rendered_levels::<u16>),
            ("u24", rendered_levels::<U24>),
            ("u32", rendered_levels::<u32>),
            ("u64", rendered_levels::<u64>),
            ("f32", rendered_levels::<f32>),
            ("f64", rendered_levels::<f64>),
        ];
        for (name, render) in formats {
            let levels = render(&pcm);
            for ((level, want), source) in levels.iter().zip(expected).zip(pcm) {
                assert!(
                    (level - want).abs() < SCALE_TOLERANCE,
                    "{name}: pcm {source} played at level {level}, expected about {want}"
                );
            }
        }
    }

    #[test]
    fn an_underrun_pads_the_buffer_with_each_formats_silence() {
        let queued = [i16::MAX, i16::MIN];
        let cases = [
            ("i16", underrun_tail_is::<i16>(&queued, 0)),
            ("i32", underrun_tail_is::<i32>(&queued, 0)),
            ("u8", underrun_tail_is::<u8>(&queued, 128)),
            ("u16", underrun_tail_is::<u16>(&queued, 32_768)),
            ("f32", underrun_tail_is::<f32>(&queued, 0.0)),
            ("empty u16", underrun_tail_is::<u16>(&[], 32_768)),
            ("empty u8", underrun_tail_is::<u8>(&[], 128)),
        ];
        for (name, silent) in cases {
            assert!(
                silent,
                "{name}: underrun did not write the format's silence"
            );
        }
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
