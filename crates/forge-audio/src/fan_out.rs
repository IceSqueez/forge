use std::sync::Arc;

use async_trait::async_trait;

use crate::error::AudioError;
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::pcm::PcmBuffer;
use crate::sink::AudioSink;

const ROUTE_REASON_SEPARATOR: &str = "; ";

/// Started via a single `join_all` to keep cross-sink start drift low; `stop` on the
/// returned handle cancels every child clip that reported one.
pub async fn fan_out_stoppable(
    buffer: PcmBuffer,
    sinks: &[Arc<dyn AudioSink>],
) -> (PlaybackHandle, Vec<Result<(), AudioError>>) {
    let futures: Vec<_> = sinks
        .iter()
        .map(|sink| {
            let sink = Arc::clone(sink);
            let buf = buffer.clone();
            async move { sink.play_stoppable(buf).await }
        })
        .collect();

    let mut handles = Vec::new();
    let mut outcomes = Vec::with_capacity(sinks.len());
    for result in futures::future::join_all(futures).await {
        match result {
            Ok(handle) => {
                handles.push(handle);
                outcomes.push(Ok(()));
            }
            Err(e) => outcomes.push(Err(e)),
        }
    }
    (PlaybackHandle::merge(handles), outcomes)
}

pub struct FanOutSink {
    sinks: Vec<Arc<dyn AudioSink>>,
}

impl FanOutSink {
    pub fn new(sinks: Vec<Arc<dyn AudioSink>>) -> Self {
        Self { sinks }
    }
}

#[async_trait]
impl AudioSink for FanOutSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        let futures: Vec<_> = self
            .sinks
            .iter()
            .map(|sink| {
                let sink = Arc::clone(sink);
                let buf = buffer.clone();
                async move { sink.play(buf).await }
            })
            .collect();
        settle(futures::future::join_all(futures).await)
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        let (handle, outcomes) = fan_out_stoppable(buffer, &self.sinks).await;
        settle(outcomes)?;
        Ok(handle)
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let futures: Vec<_> = self
            .sinks
            .iter()
            .map(|sink| {
                let sink = Arc::clone(sink);
                let buf = buffer.clone();
                async move { sink.play_controlled(buf).await }
            })
            .collect();

        let mut started = Vec::new();
        let mut start_reasons: Vec<Option<String>> = Vec::with_capacity(self.sinks.len());
        for result in futures::future::join_all(futures).await {
            match result {
                Ok(playback) => {
                    started.push(playback);
                    start_reasons.push(None);
                }
                Err(e) => start_reasons.push(Some(e.to_string())),
            }
        }

        if started.is_empty() {
            return Err(route_failure(start_reasons.into_iter().flatten().collect()));
        }
        warn_failed_routes(start_reasons.iter().filter_map(Option::as_deref));

        let handle = PlaybackHandle::merge(started.iter().map(ControlledPlayback::handle));
        let completion = async move {
            let mut outcomes = futures::future::join_all(started).await.into_iter();
            let verdicts: Vec<(bool, Result<(), String>)> = start_reasons
                .into_iter()
                .map(|slot| match slot {
                    Some(reason) => (false, Err(reason)),
                    None => (
                        true,
                        outcomes.next().unwrap_or(Ok(())).map_err(|e| e.to_string()),
                    ),
                })
                .collect();

            if verdicts.iter().any(|(_, verdict)| verdict.is_ok()) {
                let settled_failures = verdicts.iter().filter_map(|(is_settled, verdict)| {
                    is_settled.then(|| verdict.as_ref().err()).flatten()
                });
                warn_failed_routes(settled_failures.map(String::as_str));
                Ok(())
            } else {
                Err(route_failure(
                    verdicts
                        .into_iter()
                        .filter_map(|(_, verdict)| verdict.err())
                        .collect(),
                ))
            }
        };
        Ok(ControlledPlayback::merged(handle, Box::pin(completion)))
    }
}

fn settle(outcomes: Vec<Result<(), AudioError>>) -> Result<(), AudioError> {
    if outcomes.is_empty() {
        return Err(AudioError::NoRoute);
    }

    let mut played = false;
    let mut reasons = Vec::new();
    for outcome in outcomes {
        match outcome {
            Ok(()) => played = true,
            Err(e) => reasons.push(e.to_string()),
        }
    }

    if played {
        warn_failed_routes(reasons.iter().map(String::as_str));
        Ok(())
    } else {
        Err(route_failure(reasons))
    }
}

fn warn_failed_routes<'a>(reasons: impl IntoIterator<Item = &'a str>) {
    for reason in reasons {
        tracing::warn!(error = %reason, "audio route failed; playback continues on the surviving routes");
    }
}

fn route_failure(reasons: Vec<String>) -> AudioError {
    if reasons.is_empty() {
        AudioError::NoRoute
    } else {
        AudioError::AllRoutesFailed(reasons.join(ROUTE_REASON_SEPARATOR))
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::pin::pin;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use futures::poll;
    use tokio::sync::oneshot;

    use super::*;

    const CLIP_RATE: u32 = 22_050;
    const CLIP_CHANNELS: u16 = 1;
    const START_REFUSAL_A: &str = "first route has no device";
    const START_REFUSAL_B: &str = "second route reached nobody";
    const VERDICT_FAILURE_A: &str = "first route never reported a verdict";
    const VERDICT_FAILURE_B: &str = "second route refused to play";

    type FinishOutcome = Result<(), String>;

    struct MockRoute {
        start_refusal: Option<String>,
        finish: Mutex<Option<oneshot::Receiver<FinishOutcome>>>,
        stop: Arc<AtomicBool>,
        pause: Arc<AtomicBool>,
        completions: Arc<AtomicUsize>,
        received: Mutex<Vec<PcmBuffer>>,
    }

    #[async_trait]
    impl AudioSink for MockRoute {
        async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
            self.received.lock().unwrap().push(buffer);
            match &self.start_refusal {
                Some(reason) => Err(AudioError::Host(reason.clone())),
                None => Ok(()),
            }
        }

        async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
            self.play(buffer).await?;
            Ok(PlaybackHandle::from_flags(
                Arc::clone(&self.stop),
                Arc::clone(&self.pause),
            ))
        }

        async fn play_controlled(
            &self,
            buffer: PcmBuffer,
        ) -> Result<ControlledPlayback, AudioError> {
            let handle = self.play_stoppable(buffer).await?;
            let finish = self.finish.lock().unwrap().take().unwrap();
            let completions = Arc::clone(&self.completions);
            let completion = async move {
                let outcome = finish.await.unwrap_or(Ok(()));
                completions.fetch_add(1, Ordering::Relaxed);
                outcome.map_err(AudioError::Host)
            };
            Ok(ControlledPlayback::merged(handle, Box::pin(completion)))
        }
    }

    struct Probe {
        route: Arc<MockRoute>,
        finish: Option<oneshot::Sender<FinishOutcome>>,
    }

    impl Probe {
        fn sink(&self) -> Arc<dyn AudioSink> {
            Arc::clone(&self.route) as Arc<dyn AudioSink>
        }

        fn finish(&mut self, outcome: FinishOutcome) -> bool {
            self.finish.take().unwrap().send(outcome).is_ok()
        }

        fn stopped(&self) -> bool {
            self.route.stop.load(Ordering::Relaxed)
        }

        fn paused(&self) -> bool {
            self.route.pause.load(Ordering::Relaxed)
        }

        fn completions(&self) -> usize {
            self.route.completions.load(Ordering::Relaxed)
        }

        fn received(&self) -> Vec<PcmBuffer> {
            self.route.received.lock().unwrap().clone()
        }
    }

    fn route(start_refusal: Option<&str>) -> Probe {
        let (finish_tx, finish_rx) = oneshot::channel();
        Probe {
            route: Arc::new(MockRoute {
                start_refusal: start_refusal.map(str::to_owned),
                finish: Mutex::new(Some(finish_rx)),
                stop: Arc::new(AtomicBool::new(false)),
                pause: Arc::new(AtomicBool::new(false)),
                completions: Arc::new(AtomicUsize::new(0)),
                received: Mutex::new(Vec::new()),
            }),
            finish: Some(finish_tx),
        }
    }

    fn fan(probes: &[Probe]) -> FanOutSink {
        FanOutSink::new(probes.iter().map(Probe::sink).collect())
    }

    fn clip() -> PcmBuffer {
        PcmBuffer::new(vec![7, -7, 21, -21], CLIP_RATE, CLIP_CHANNELS)
    }

    fn reported_reasons(err: &AudioError) -> &str {
        match err {
            AudioError::AllRoutesFailed(reasons) => reasons,
            _ => "",
        }
    }

    fn assert_every_reason_survives(case: &str, err: &AudioError, expected: &[&str]) {
        assert!(
            matches!(err, AudioError::AllRoutesFailed(_)),
            "{case}: every route failed yet the fan-out reported {err:?}"
        );
        for reason in expected {
            assert!(
                reported_reasons(err).contains(reason),
                "{case}: {err} drops {reason:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_fan_out_over_no_routes_refuses_distinctly_on_every_entry_point() {
        let empty = FanOutSink::new(Vec::new());
        for outcome in [
            empty.play(clip()).await,
            empty.play_stoppable(clip()).await.map(|_| ()),
            empty.play_controlled(clip()).await.map(|_| ()),
        ] {
            assert!(
                matches!(&outcome, Err(AudioError::NoRoute)),
                "a fan-out with nowhere to play must not report {outcome:?}"
            );
        }
    }

    #[tokio::test]
    async fn play_succeeds_while_any_route_starts_and_carries_every_reason_when_none_do() {
        for (case, refusals) in [
            ("the only route plays", vec![None]),
            ("the only route refuses", vec![Some(START_REFUSAL_A)]),
            (
                "the second of two refuses",
                vec![None, Some(START_REFUSAL_B)],
            ),
            (
                "the first of two refuses",
                vec![Some(START_REFUSAL_A), None],
            ),
            ("both routes play", vec![None, None]),
            (
                "both routes refuse",
                vec![Some(START_REFUSAL_A), Some(START_REFUSAL_B)],
            ),
        ] {
            let probes: Vec<Probe> = refusals.iter().map(|r| route(*r)).collect();
            let outcome = fan(&probes).play(clip()).await;
            let expected: Vec<&str> = refusals.iter().flatten().copied().collect();

            if expected.len() < refusals.len() {
                assert!(
                    outcome.is_ok(),
                    "{case}: a surviving route must keep the clip successful, got {outcome:?}"
                );
            } else {
                assert_every_reason_survives(case, &outcome.unwrap_err(), &expected);
            }
        }
    }

    #[tokio::test]
    async fn a_stoppable_fan_out_hands_back_a_handle_for_the_routes_that_started() {
        for (case, refusals) in [
            ("the refusing route is first", [Some(START_REFUSAL_A), None]),
            (
                "the refusing route is second",
                [None, Some(START_REFUSAL_B)],
            ),
        ] {
            let probes: Vec<Probe> = refusals.iter().map(|r| route(*r)).collect();
            let handle = fan(&probes).play_stoppable(clip()).await;
            assert!(
                handle.is_ok(),
                "{case}: one surviving route must keep the clip playing"
            );

            handle.unwrap().stop();
            for (probe, refusal) in probes.iter().zip(refusals) {
                assert_eq!(
                    probe.stopped(),
                    refusal.is_none(),
                    "{case}: stop must reach every route that started and none that did not"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_controlled_fan_out_fails_with_every_reason_when_no_route_starts() {
        let probes = [route(Some(START_REFUSAL_A)), route(Some(START_REFUSAL_B))];
        let err = fan(&probes)
            .play_controlled(clip())
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(
            matches!(&err, AudioError::AllRoutesFailed(_)),
            "no route started yet the fan-out reported {err:?}"
        );
        for reason in [START_REFUSAL_A, START_REFUSAL_B] {
            assert!(
                reported_reasons(&err).contains(reason),
                "{err} drops {reason:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_controlled_fan_out_completes_only_once_every_started_route_has() {
        let mut probes = [route(None), route(None)];
        let playback = fan(&probes).play_controlled(clip()).await.unwrap();
        let mut playback = pin!(playback);

        assert!(
            probes[0].finish(Ok(())),
            "the fan-out must still be awaiting the fast route"
        );
        assert!(
            poll!(playback.as_mut()).is_pending(),
            "the utterance cannot be finished while the slow route is still playing"
        );

        assert!(
            probes[1].finish(Ok(())),
            "the fan-out must still be awaiting the slow route"
        );
        assert!(matches!(playback.await, Ok(())));
    }

    #[tokio::test]
    async fn a_route_that_fails_after_starting_cannot_fail_an_utterance_another_route_played() {
        for (case, first, second, expected) in [
            (
                "the failing route reports first",
                Err(VERDICT_FAILURE_A.to_owned()),
                Ok(()),
                Vec::new(),
            ),
            (
                "the failing route reports last",
                Ok(()),
                Err(VERDICT_FAILURE_B.to_owned()),
                Vec::new(),
            ),
            (
                "both routes fail",
                Err(VERDICT_FAILURE_A.to_owned()),
                Err(VERDICT_FAILURE_B.to_owned()),
                vec![VERDICT_FAILURE_A, VERDICT_FAILURE_B],
            ),
        ] {
            let mut probes = [route(None), route(None)];
            let playback = fan(&probes).play_controlled(clip()).await.unwrap();
            let mut playback = pin!(playback);

            assert!(
                probes[0].finish(first),
                "{case}: the first route was dropped"
            );
            assert!(
                poll!(playback.as_mut()).is_pending(),
                "{case}: one route is still playing"
            );
            assert!(
                probes[1].finish(second),
                "{case}: the second route was dropped"
            );

            let outcome = playback.await;
            if expected.is_empty() {
                assert!(
                    outcome.is_ok(),
                    "{case}: a route that played must finish the utterance, got {outcome:?}"
                );
            } else {
                assert_every_reason_survives(case, &outcome.unwrap_err(), &expected);
            }
        }
    }

    #[tokio::test]
    async fn a_start_refusal_is_still_named_when_the_route_that_started_fails_to_settle() {
        for (case, plan, expected) in [
            (
                "the route that refused to start comes first",
                vec![
                    (Some(START_REFUSAL_A), None),
                    (None, Some(VERDICT_FAILURE_B)),
                ],
                Some(vec![START_REFUSAL_A, VERDICT_FAILURE_B]),
            ),
            (
                "the route that refused to start comes last",
                vec![
                    (None, Some(VERDICT_FAILURE_A)),
                    (Some(START_REFUSAL_B), None),
                ],
                Some(vec![VERDICT_FAILURE_A, START_REFUSAL_B]),
            ),
            (
                "the route that started plays to the end",
                vec![(Some(START_REFUSAL_A), None), (None, None)],
                None,
            ),
        ] {
            let mut probes: Vec<Probe> = plan.iter().map(|(refusal, _)| route(*refusal)).collect();
            let playback = fan(&probes)
                .play_controlled(clip())
                .await
                .unwrap_or_else(|e| panic!("{case}: a surviving route must start, got {e:?}"));
            let playback = pin!(playback);

            for (probe, (refusal, verdict)) in probes.iter_mut().zip(&plan) {
                if refusal.is_none() {
                    assert!(
                        probe.finish(verdict.map_or(Ok(()), |reason| Err(reason.to_owned()))),
                        "{case}: a started route was dropped"
                    );
                }
            }

            let outcome = playback.await;
            match expected {
                None => assert!(
                    outcome.is_ok(),
                    "{case}: a route that played must finish the utterance, got {outcome:?}"
                ),
                Some(reasons) => {
                    let err = outcome.unwrap_err();
                    assert!(
                        matches!(&err, AudioError::AllRoutesFailed(_)),
                        "{case}: every route failed yet the fan-out reported {err:?}"
                    );
                    assert_eq!(
                        reported_reasons(&err),
                        reasons
                            .iter()
                            .map(|reason| AudioError::Host((*reason).to_owned()).to_string())
                            .collect::<Vec<_>>()
                            .join(ROUTE_REASON_SEPARATOR),
                        "{case}: every reason must survive, in route order"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn control_reaches_every_started_route_including_one_that_already_finished() {
        let mut probes = [route(Some(START_REFUSAL_A)), route(None), route(None)];
        let playback = fan(&probes).play_controlled(clip()).await.unwrap();
        let mut playback = pin!(playback);

        assert!(
            probes[1].finish(Ok(())),
            "the fan-out dropped the fast route"
        );
        assert!(
            poll!(playback.as_mut()).is_pending(),
            "the slow route is still playing"
        );

        playback.stop();
        playback.pause();
        for probe in &probes[1..] {
            assert!(probe.stopped(), "a started route missed the stop");
            assert!(probe.paused(), "a started route missed the pause");
        }
        assert!(
            !probes[0].stopped(),
            "a route that never started owns no flag to set"
        );

        assert!(
            probes[2].finish(Ok(())),
            "the fan-out dropped the slow route"
        );
        assert!(
            matches!(playback.await, Ok(())),
            "stopping must not leave the utterance hanging"
        );
    }

    #[tokio::test]
    async fn dropping_a_merged_playback_leaves_no_route_reporting_late() {
        let mut probes = [route(None), route(None)];
        let playback = fan(&probes).play_controlled(clip()).await.unwrap();
        drop(playback);

        for probe in probes.iter_mut() {
            assert!(
                !probe.finish(Ok(())),
                "a dropped playback must have dropped every route's completion"
            );
        }
        tokio::task::yield_now().await;
        for probe in &probes {
            assert_eq!(
                probe.completions(),
                0,
                "a dropped playback must not record a late verdict"
            );
        }
    }

    #[tokio::test]
    async fn every_route_receives_the_clip_untouched_at_its_native_rate() {
        let probes = [route(None), route(None)];
        assert!(fan(&probes).play(clip()).await.is_ok());
        for probe in &probes {
            assert_eq!(
                probe.received(),
                vec![clip()],
                "a route saw a different clip"
            );
        }
    }
}
