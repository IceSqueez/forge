use std::sync::Arc;

use async_trait::async_trait;

use crate::error::AudioError;
use crate::sink::AudioSink;
use crate::sink::playback::{ControlledPlayback, PlaybackHandle};
use forge_speech_types::PcmBuffer;

const ROUTE_REASON_SEPARATOR: &str = "; ";

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
    async fn start(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let futures: Vec<_> = self
            .sinks
            .iter()
            .map(|sink| {
                let sink = Arc::clone(sink);
                let buf = buffer.clone();
                async move { sink.start(buf).await }
            })
            .collect();

        let starts = futures::future::join_all(futures).await;
        let routes: Vec<RouteStart> = starts
            .into_iter()
            .map(|result| match result {
                Ok(playback) => RouteStart::Started(playback),
                Err(e) => RouteStart::Refused(e),
            })
            .collect();

        if !routes.iter().any(RouteStart::is_started) {
            return Err(route_failure(
                routes
                    .into_iter()
                    .filter_map(RouteStart::into_refusal)
                    .collect(),
            ));
        }
        warn_failed_routes(routes.iter().filter_map(RouteStart::refusal));

        let handle = PlaybackHandle::merge(routes.iter().filter_map(RouteStart::handle));
        let completion = async move {
            let verdicts =
                futures::future::join_all(routes.into_iter().map(RouteStart::verdict)).await;

            if verdicts.iter().any(|verdict| verdict.outcome.is_ok()) {
                let settled_failures = verdicts
                    .iter()
                    .filter(|verdict| verdict.started)
                    .filter_map(|verdict| verdict.outcome.as_ref().err());
                warn_failed_routes(settled_failures);
                Ok(())
            } else {
                Err(route_failure(
                    verdicts
                        .into_iter()
                        .filter_map(|verdict| verdict.outcome.err())
                        .collect(),
                ))
            }
        };
        Ok(ControlledPlayback::merged(handle, completion))
    }
}

enum RouteStart {
    Started(ControlledPlayback),
    Refused(AudioError),
}

struct RouteVerdict {
    started: bool,
    outcome: Result<(), AudioError>,
}

impl RouteStart {
    fn is_started(&self) -> bool {
        matches!(self, Self::Started(_))
    }

    fn handle(&self) -> Option<PlaybackHandle> {
        match self {
            Self::Started(playback) => Some(playback.handle()),
            Self::Refused(_) => None,
        }
    }

    fn refusal(&self) -> Option<&AudioError> {
        match self {
            Self::Started(_) => None,
            Self::Refused(reason) => Some(reason),
        }
    }

    fn into_refusal(self) -> Option<AudioError> {
        match self {
            Self::Started(_) => None,
            Self::Refused(reason) => Some(reason),
        }
    }

    async fn verdict(self) -> RouteVerdict {
        match self {
            Self::Started(playback) => RouteVerdict {
                started: true,
                outcome: playback.await,
            },
            Self::Refused(reason) => RouteVerdict {
                started: false,
                outcome: Err(reason),
            },
        }
    }
}

fn warn_failed_routes<'a>(reasons: impl IntoIterator<Item = &'a AudioError>) {
    for reason in reasons {
        tracing::warn!(error = %reason, "audio route failed; playback continues on the surviving routes");
    }
}

fn route_failure(refusals: Vec<AudioError>) -> AudioError {
    if refusals.is_empty() {
        AudioError::NoRoute
    } else {
        let reasons: Vec<String> = refusals.iter().map(ToString::to_string).collect();
        AudioError::AllRoutesFailed(reasons.join(ROUTE_REASON_SEPARATOR))
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::sink::playback::PlaybackControl;

    use futures::poll;
    use tokio::sync::oneshot;

    use super::*;

    const CLIP_RATE: u32 = 22_050;
    const CLIP_CHANNELS: u16 = 1;
    const START_REFUSAL_A: &str = "first route has no device";
    const START_REFUSAL_B: &str = "second route reached nobody";
    const VERDICT_FAILURE_A: &str = "first route never reported a verdict";
    const VERDICT_FAILURE_B: &str = "second route refused to play";
    const VERDICT_ABANDONED: &str = "the test dropped this route's verdict";

    type FinishOutcome = Result<(), String>;

    struct MockRoute {
        start_refusal: Option<String>,
        finish: Mutex<Option<oneshot::Receiver<FinishOutcome>>>,
        control: Arc<PlaybackControl>,
        completions: Arc<AtomicUsize>,
        received: Mutex<Vec<PcmBuffer>>,
    }

    #[async_trait]
    impl AudioSink for MockRoute {
        async fn start(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
            self.received.lock().unwrap().push(buffer);
            if let Some(reason) = &self.start_refusal {
                return Err(AudioError::Host(reason.clone()));
            }
            let finish = self.finish.lock().unwrap().take().unwrap();
            let completions = Arc::clone(&self.completions);
            let completion = async move {
                let outcome = finish
                    .await
                    .unwrap_or_else(|_| Err(VERDICT_ABANDONED.to_owned()));
                completions.fetch_add(1, Ordering::Relaxed);
                outcome.map_err(AudioError::Host)
            };
            Ok(ControlledPlayback::new(
                Arc::clone(&self.control),
                completion,
            ))
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
            self.route.control.is_stopped()
        }

        fn paused(&self) -> bool {
            self.route.control.is_paused()
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
                control: PlaybackControl::new(),
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
            empty.start(clip()).await.map(|_| ()),
        ] {
            assert!(
                matches!(&outcome, Err(AudioError::NoRoute)),
                "a fan-out with nowhere to play must not report {outcome:?}"
            );
        }
    }

    #[tokio::test]
    async fn start_succeeds_while_any_route_starts_and_carries_every_reason_when_none_do() {
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
            let outcome = fan(&probes).start(clip()).await.map(|_| ());
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
            let playback = fan(&probes).start(clip()).await;
            assert!(
                playback.is_ok(),
                "{case}: one surviving route must keep the clip playing"
            );

            playback.unwrap().stop();
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
    async fn a_controlled_fan_out_completes_only_once_every_started_route_has() {
        let mut probes = [route(None), route(None)];
        let playback = fan(&probes).start(clip()).await.unwrap();
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
            let playback = fan(&probes).start(clip()).await.unwrap();
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
                .start(clip())
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
        let playback = fan(&probes).start(clip()).await.unwrap();
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
        let playback = fan(&probes).start(clip()).await.unwrap();
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
        assert!(fan(&probes).start(clip()).await.is_ok());
        for probe in &probes {
            assert_eq!(
                probe.received(),
                vec![clip()],
                "a route saw a different clip"
            );
        }
    }

    struct NeverStarts;

    #[async_trait]
    impl AudioSink for NeverStarts {
        async fn start(&self, _buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn an_abandoned_start_stops_every_route_that_already_started() {
        for hung_position in 0..3 {
            let probes = [route(None), route(None)];
            let mut sinks: Vec<Arc<dyn AudioSink>> = probes.iter().map(Probe::sink).collect();
            sinks.insert(hung_position, Arc::new(NeverStarts));
            let fan_out = FanOutSink::new(sinks);
            let mut start = fan_out.start(clip());

            assert!(
                poll!(start.as_mut()).is_pending(),
                "the hung route at {hung_position} must hold the start open"
            );
            assert!(
                probes.iter().all(|probe| !probe.stopped()),
                "a pending start must leave its routes playing"
            );

            drop(start);

            assert!(
                probes.iter().all(Probe::stopped),
                "the hung route at {hung_position}: a started route kept playing"
            );
        }
    }
}
