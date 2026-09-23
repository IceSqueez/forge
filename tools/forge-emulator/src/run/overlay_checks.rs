use std::collections::BTreeMap;

use serde_json::Value;
use tokio::time::Instant;

use super::outcome::{
    EVIDENCE_LIMIT, Evidence, FailureCause, OverlayEvidence, ReceivedContent, RunClock, Verdict,
};
use crate::overlay::{OverlayPage, ReceivedFrame};
use crate::scenario::OverlayContent;

const FRAME_KEY: &str = "frame";
const CONTENT_FRAME: &str = "content";
const CONTENT_KEY: &str = "content";
const DURATION_KEY: &str = "durationMs";
/// The two keys `forge_types::Variant` serializes itself under.
const TAG_KEY: &str = "type";
const TAGGED_VALUE_KEY: &str = "value";
const POINTER_SEPARATOR: char = '/';

pub(crate) async fn observe_overlay_content(
    page: &OverlayPage,
    from: usize,
    deadline: Instant,
    expected: &OverlayContent,
    clock: RunClock,
) -> (Verdict, Evidence) {
    page.wait_until(deadline, |frames| {
        frames
            .get(from..)
            .unwrap_or_default()
            .iter()
            .filter_map(content_of)
            .any(|content| satisfies(content, &expected.values.0))
    })
    .await;

    let frames = page.read(|frames| collect(frames.get(from..).unwrap_or_default(), clock));
    verdict_for(&expected.overlay, frames, &expected.values.0)
}

/// Split from the wait so the whole judgement is a pure function of the frames observed.
pub(crate) fn verdict_for(
    overlay: &str,
    frames: Vec<ReceivedContent>,
    expected: &BTreeMap<String, String>,
) -> (Verdict, Evidence) {
    let matched = frames.iter().find(|frame| {
        satisfies(&frame.content, expected) && tagged_pointers(&frame.content).is_empty()
    });
    let evidence = |mismatched: Vec<String>, tagged: Vec<String>, frames: Vec<ReceivedContent>| {
        Evidence::Overlay(OverlayEvidence {
            overlay: overlay.to_owned(),
            frames,
            mismatched,
            tagged,
        })
    };
    if matched.is_some() {
        return (
            Verdict::Passed,
            evidence(Vec::new(), Vec::new(), listed(frames)),
        );
    }

    let closest = frames
        .iter()
        .min_by_key(|frame| mismatched_keys(&frame.content, expected).len());
    let (mismatched, tagged) = closest.map_or_else(
        || (Vec::new(), Vec::new()),
        |frame| {
            (
                mismatched_keys(&frame.content, expected),
                tagged_pointers(&frame.content),
            )
        },
    );
    let observed = frames.len();
    let cause = if tagged.is_empty() {
        FailureCause::NoOverlayContent { observed }
    } else {
        FailureCause::TaggedOverlayValue {
            pointers: tagged.clone(),
        }
    };
    (
        Verdict::Failed(cause),
        evidence(mismatched, tagged, listed(frames)),
    )
}

fn listed(mut frames: Vec<ReceivedContent>) -> Vec<ReceivedContent> {
    frames.truncate(EVIDENCE_LIMIT);
    frames
}

fn collect(frames: &[ReceivedFrame], clock: RunClock) -> Vec<ReceivedContent> {
    frames
        .iter()
        .filter_map(|frame| {
            content_of(frame).map(|content| ReceivedContent {
                arrived_ms: clock.millis(frame.arrived),
                content: content.clone(),
                duration_ms: frame.raw.get(DURATION_KEY).and_then(Value::as_u64),
            })
        })
        .collect()
}

fn content_of(frame: &ReceivedFrame) -> Option<&Value> {
    (frame.raw.get(FRAME_KEY).and_then(Value::as_str) == Some(CONTENT_FRAME))
        .then(|| frame.raw.get(CONTENT_KEY))
        .flatten()
}

fn satisfies(content: &Value, expected: &BTreeMap<String, String>) -> bool {
    mismatched_keys(content, expected).is_empty()
}

/// Every expected key whose value is not exactly that plain JSON string.
pub(crate) fn mismatched_keys(content: &Value, expected: &BTreeMap<String, String>) -> Vec<String> {
    expected
        .iter()
        .filter(|(key, text)| content.get(key.as_str()) != Some(&Value::String((*text).clone())))
        .map(|(key, _)| key.clone())
        .collect()
}

/// JSON pointers to every node shaped like a tagged `Variant`, at any depth.
pub(crate) fn tagged_pointers(content: &Value) -> Vec<String> {
    let mut found = Vec::new();
    walk(content, &mut String::new(), &mut found);
    found
}

fn walk(value: &Value, pointer: &mut String, found: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            if fields.contains_key(TAG_KEY) && fields.contains_key(TAGGED_VALUE_KEY) {
                found.push(if pointer.is_empty() {
                    POINTER_SEPARATOR.to_string()
                } else {
                    pointer.clone()
                });
                return;
            }
            for (key, field) in fields {
                descend(pointer, &escaped(key), field, found);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                descend(pointer, &index.to_string(), item, found);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn descend(pointer: &mut String, segment: &str, value: &Value, found: &mut Vec<String>) {
    let restore = pointer.len();
    pointer.push(POINTER_SEPARATOR);
    pointer.push_str(segment);
    walk(value, pointer, found);
    pointer.truncate(restore);
}

/// RFC 6901 escaping, so a content key holding a slash cannot forge a pointer.
fn escaped(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;

    fn expected(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn frame(content: Value) -> ReceivedContent {
        ReceivedContent {
            arrived_ms: 0,
            content,
            duration_ms: None,
        }
    }

    #[test]
    fn a_plain_string_frame_carrying_every_expected_value_passes() {
        let (verdict, _) = verdict_for(
            "Alert Box",
            vec![frame(
                json!({ "headline": "alice raised an alert", "subline": "via !alert" }),
            )],
            &expected(&[
                ("headline", "alice raised an alert"),
                ("subline", "via !alert"),
            ]),
        );

        assert_eq!(verdict, Verdict::Passed);
    }

    #[test]
    fn the_field_bug_shape_fails_as_tagged_rather_than_as_a_plain_mismatch() {
        let (verdict, evidence) = verdict_for(
            "Alert Box",
            vec![frame(json!({
                "headline": { "type": "string", "value": "alice raised an alert" },
                "subline": { "type": "string", "value": "via !alert" },
            }))],
            &expected(&[("headline", "alice raised an alert")]),
        );

        assert!(
            matches!(&verdict, Verdict::Failed(FailureCause::TaggedOverlayValue { pointers })
                if pointers == &["/headline".to_owned(), "/subline".to_owned()]),
            "got {verdict:?}"
        );
        let Evidence::Overlay(overlay) = evidence else {
            panic!("a content failure must carry the frames the page received");
        };
        assert_eq!(
            overlay.frames[0].content["headline"]["value"],
            json!("alice raised an alert"),
            "the report must show the raw frame, not a normalized one"
        );
    }

    #[test]
    fn a_tagged_value_outside_the_expected_keys_still_fails_the_whole_frame() {
        let (verdict, _) = verdict_for(
            "Goal Bar",
            vec![frame(json!({
                "label": "Sub goal",
                "value": { "type": "int", "value": 42 },
            }))],
            &expected(&[("label", "Sub goal")]),
        );

        assert!(
            matches!(&verdict, Verdict::Failed(FailureCause::TaggedOverlayValue { pointers })
                if pointers == &["/value".to_owned()]),
            "a key the scenario does not name still renders on the page; got {verdict:?}"
        );
    }

    #[test]
    fn tagged_shapes_are_found_at_every_depth_and_never_in_a_plain_object() {
        for (content, expected_pointers, label) in [
            (
                json!({ "items": [{ "type": "int", "value": 1 }] }),
                vec!["/items/0".to_owned()],
                "a tagged element inside an array",
            ),
            (
                json!({ "a/b": { "type": "bool", "value": true } }),
                vec!["/a~1b".to_owned()],
                "a content key holding a pointer separator",
            ),
            (
                json!({ "headline": "type and value", "meta": { "type": "note" } }),
                Vec::new(),
                "an object carrying only one of the two tag keys",
            ),
            (
                json!({ "headline": "plain" }),
                Vec::new(),
                "the shape a fixed forge sends",
            ),
        ] {
            assert_eq!(tagged_pointers(&content), expected_pointers, "{label}");
        }
    }

    #[test]
    fn a_missing_or_non_string_value_is_reported_as_the_key_that_differs() {
        for (content, differing, label) in [
            (json!({}), vec!["headline"], "a frame the key never reached"),
            (
                json!({ "headline": 42 }),
                vec!["headline"],
                "a number where the page needs text",
            ),
            (
                json!({ "headline": "other wording" }),
                vec!["headline"],
                "text the run did not expand to",
            ),
            (
                json!({ "headline": "expected" }),
                Vec::<&str>::new(),
                "the value the scenario pins",
            ),
        ] {
            assert_eq!(
                mismatched_keys(&content, &expected(&[("headline", "expected")])),
                differing
                    .iter()
                    .map(|key| (*key).to_owned())
                    .collect::<Vec<String>>(),
                "{label}"
            );
        }
    }

    #[test]
    fn a_window_with_no_content_frame_at_all_reports_none_observed() {
        let (verdict, evidence) =
            verdict_for("Alert Box", Vec::new(), &expected(&[("headline", "x")]));

        assert_eq!(
            verdict,
            Verdict::Failed(FailureCause::NoOverlayContent { observed: 0 })
        );
        let Evidence::Overlay(overlay) = evidence else {
            panic!("the failure must still name the overlay it watched");
        };
        assert_eq!(overlay.overlay, "Alert Box");
        assert!(overlay.frames.is_empty());
    }

    #[test]
    fn the_closest_frame_is_the_one_whose_differences_are_reported() {
        let (_, evidence) = verdict_for(
            "Alert Box",
            vec![
                frame(json!({ "headline": "wrong", "subline": "wrong" })),
                frame(json!({ "headline": "right", "subline": "wrong" })),
            ],
            &expected(&[("headline", "right"), ("subline", "right")]),
        );

        let Evidence::Overlay(overlay) = evidence else {
            panic!("expected overlay evidence");
        };
        assert_eq!(overlay.mismatched, ["subline".to_owned()]);
        assert_eq!(
            overlay.frames.len(),
            2,
            "every content frame in the window belongs in the report"
        );
    }
}
