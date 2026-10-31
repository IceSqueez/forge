use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use forge_types::{VARIANT_MAX_COLLECTION_LEN, VARIANT_MAX_DEPTH};
use rhai::debugger::{DebuggerCommand, DebuggerEvent};
use rhai::{Dynamic, EvalAltResult, EvalContext, Position};

const MAX_FRAME_DATA_BYTES: usize = 64 << 20;
const ENTRY_BYTES: usize = size_of::<Dynamic>();
const READ_SCRAMBLER: u32 = 0x9E37_79B9;
const SAMPLED_READ_SHIFT: u32 = 24;
const MAX_STEPS_BETWEEN_AUDITS: usize = 1 << 10;
const REFUSED_DATA_NAME: &str = "Script data";

#[derive(Default)]
struct AuditPacing {
    steps: AtomicU32,
    steps_to_skip: AtomicUsize,
    external_variables: AtomicUsize,
}

impl AuditPacing {
    fn is_due(&self) -> bool {
        let skipped = self.steps_to_skip.load(Ordering::Relaxed);
        if skipped > 0 {
            self.steps_to_skip.store(skipped - 1, Ordering::Relaxed);
            return false;
        }
        is_sampled(self.steps.fetch_add(1, Ordering::Relaxed))
    }

    fn rest_after(&self, spent_bytes: usize) {
        let visited = spent_bytes / ENTRY_BYTES;
        self.steps_to_skip
            .store(visited.min(MAX_STEPS_BETWEEN_AUDITS), Ordering::Relaxed);
    }
}

pub(crate) fn install_data_audit(engine: &mut rhai::Engine) {
    let pacing = AuditPacing::default();
    #[expect(
        deprecated,
        reason = "the debugger hook is only marked volatile upstream"
    )]
    engine.register_debugger(
        |_, debugger| debugger,
        move |context, event, _, _, _| {
            if matches!(event, DebuggerEvent::Start) {
                pacing
                    .external_variables
                    .store(context.scope().len(), Ordering::Relaxed);
            }
            if matches!(event, DebuggerEvent::Step) && pacing.is_due() {
                let mut bytes_left = MAX_FRAME_DATA_BYTES;
                let externals = pacing.external_variables.load(Ordering::Relaxed);
                if !frame_fits_budget(&context, externals, &mut bytes_left) {
                    return Err(data_too_large());
                }
                pacing.rest_after(MAX_FRAME_DATA_BYTES - bytes_left);
            }
            Ok(DebuggerCommand::StepInto)
        },
    );
}

fn data_too_large() -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorDataTooLarge(
        REFUSED_DATA_NAME.to_owned(),
        Position::NONE,
    ))
}

pub(crate) fn ensure_result_fits(value: &Dynamic) -> Result<(), Box<EvalAltResult>> {
    if value_fits(value, 0, &mut MAX_FRAME_DATA_BYTES.clone()) {
        Ok(())
    } else {
        Err(data_too_large())
    }
}

fn is_sampled(read: u32) -> bool {
    read.wrapping_mul(READ_SCRAMBLER) >> SAMPLED_READ_SHIFT == 0
}

fn frame_fits_budget(context: &EvalContext, externals: usize, bytes_left: &mut usize) -> bool {
    let bound_below = if context.call_level() == 0 {
        externals
    } else {
        0
    };
    let newest_index = context.scope().len().saturating_sub(1);
    context
        .scope()
        .iter_raw()
        .enumerate()
        .filter(|(from_newest, (_, is_constant, _))| {
            !is_constant || newest_index - from_newest >= bound_below
        })
        .map(|(_, (_, _, value))| value)
        .chain(context.this_ptr())
        .all(|value| value_fits(value, 0, bytes_left))
}

fn value_fits(value: &Dynamic, depth: u8, bytes_left: &mut usize) -> bool {
    if !(value.is_string() || value.is_array() || value.is_map()) {
        return true;
    }
    if let Ok(text) = value.as_immutable_string_ref() {
        return charge(bytes_left, text.len());
    }
    if let Ok(items) = value.as_array_ref() {
        return collection_fits(items.len(), depth, bytes_left)
            && items
                .iter()
                .all(|item| value_fits(item, depth + 1, bytes_left));
    }
    if let Ok(entries) = value.as_map_ref() {
        return collection_fits(entries.len(), depth, bytes_left)
            && entries.iter().all(|(key, item)| {
                charge(bytes_left, key.len()) && value_fits(item, depth + 1, bytes_left)
            });
    }
    true
}

fn collection_fits(len: usize, depth: u8, bytes_left: &mut usize) -> bool {
    depth <= VARIANT_MAX_DEPTH
        && len <= VARIANT_MAX_COLLECTION_LEN
        && charge(bytes_left, len * ENTRY_BYTES)
}

fn charge(bytes_left: &mut usize, bytes: usize) -> bool {
    match bytes_left.checked_sub(bytes) {
        Some(left) => {
            *bytes_left = left;
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn array_of(len: usize) -> Dynamic {
        Dynamic::from_array(vec![Dynamic::UNIT; len])
    }

    fn map_of(len: usize) -> Dynamic {
        Dynamic::from_map(
            (0..len)
                .map(|i| (i.to_string().into(), Dynamic::UNIT))
                .collect(),
        )
    }

    fn nested_arrays(collections: usize) -> Dynamic {
        (1..collections).fold(array_of(0), |inner, _| Dynamic::from_array(vec![inner]))
    }

    fn fits(value: &Dynamic) -> bool {
        value_fits(value, 0, &mut MAX_FRAME_DATA_BYTES.clone())
    }

    #[test]
    fn a_collection_may_reach_the_length_limit_but_not_pass_it() {
        for (kind, at_limit, past_limit) in [
            (
                "array",
                array_of(VARIANT_MAX_COLLECTION_LEN),
                array_of(VARIANT_MAX_COLLECTION_LEN + 1),
            ),
            (
                "map",
                map_of(VARIANT_MAX_COLLECTION_LEN),
                map_of(VARIANT_MAX_COLLECTION_LEN + 1),
            ),
        ] {
            assert!(fits(&at_limit), "{kind} at the limit");
            assert!(!fits(&past_limit), "{kind} past the limit");
        }
    }

    #[test]
    fn nesting_may_reach_the_depth_limit_but_not_pass_it() {
        let deepest_legal = usize::from(VARIANT_MAX_DEPTH) + 1;

        assert!(fits(&nested_arrays(deepest_legal)));
        assert!(!fits(&nested_arrays(deepest_legal + 1)));
    }

    #[test]
    fn the_collection_past_the_limit_is_found_at_any_depth() {
        let buried = Dynamic::from_array(vec![Dynamic::from_array(vec![array_of(
            VARIANT_MAX_COLLECTION_LEN + 1,
        )])]);

        assert!(!fits(&buried));
    }

    #[test]
    fn the_charge_may_spend_the_whole_budget_but_not_one_byte_more() {
        let mut left = 10;
        assert!(charge(&mut left, 10));
        assert_eq!(left, 0);

        let mut left = 10;
        assert!(!charge(&mut left, 11));
        assert_eq!(left, 10);
    }

    #[test]
    fn texts_and_keys_and_slots_all_draw_on_one_budget() {
        let text = "x".repeat(8);
        let value = Dynamic::from_map([(text.as_str().into(), text.clone().into())].into());
        let spent = 2 * text.len() + ENTRY_BYTES;

        assert!(value_fits(&value, 0, &mut spent.clone()));
        assert!(!value_fits(&value, 0, &mut (spent - 1)));
    }

    #[test]
    fn a_scalar_costs_nothing() {
        let mut left = 0;

        assert!(value_fits(&Dynamic::from_int(1), 0, &mut left));
    }

    #[test]
    fn a_rest_skips_one_step_per_visited_entry() {
        const VISITED: usize = 3;
        let pacing = AuditPacing::default();

        pacing.rest_after(VISITED * ENTRY_BYTES);

        assert!((0..VISITED).all(|_| !pacing.is_due()));
        assert!(pacing.is_due());
    }

    #[test]
    fn a_rest_never_outlasts_the_cap_however_much_was_visited() {
        for spent in [
            MAX_STEPS_BETWEEN_AUDITS * ENTRY_BYTES,
            (MAX_STEPS_BETWEEN_AUDITS + 1) * ENTRY_BYTES,
            MAX_FRAME_DATA_BYTES,
            usize::MAX,
        ] {
            let pacing = AuditPacing::default();

            pacing.rest_after(spent);

            assert!((0..MAX_STEPS_BETWEEN_AUDITS).all(|_| !pacing.is_due()));
            assert!(pacing.is_due(), "spent {spent}");
        }
    }

    #[test]
    fn every_read_window_holds_a_sample_at_about_one_read_in_two_hundred_fifty_six() {
        const READS: u32 = 1 << 20;
        const EXPECTED_SAMPLES: usize = 1 << 12;
        const SAMPLE_COUNT_TOLERANCE: usize = 1 << 4;
        const LONGEST_TOLERATED_GAP: u32 = 1 << 10;
        let sampled: Vec<u32> = (0..READS).filter(|read| is_sampled(*read)).collect();

        let longest_gap = sampled.windows(2).map(|w| w[1] - w[0]).max();

        assert!(is_sampled(0), "the first read of a run is audited");
        assert!(sampled.len().abs_diff(EXPECTED_SAMPLES) <= SAMPLE_COUNT_TOLERANCE);
        assert!(longest_gap.is_some_and(|gap| gap <= LONGEST_TOLERATED_GAP));
    }
}
