use cpal::{FromSample, Sample};

pub(crate) fn write_frames<T: Sample + FromSample<i16>>(source: &[i16], frames: &mut [T]) {
    for (slot, &sample) in frames.iter_mut().zip(source) {
        *slot = T::from_sample(sample);
    }
    let written = source.len().min(frames.len());
    frames[written..].fill(T::EQUILIBRIUM);
}

pub(crate) fn peak<T>(input: &[T]) -> f32
where
    T: Copy,
    f32: FromSample<T>,
{
    input.iter().fold(0.0f32, |loudest, sample| {
        loudest.max(<f32 as FromSample<T>>::from_sample_(*sample).abs())
    })
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use cpal::{I24, U24};

    use super::*;

    const LEVEL_TOLERANCE: f32 = 1e-5;
    const SOURCE: [i16; 3] = [i16::MIN, 0, 0x4000];
    const OVERSIZED_BY: usize = 3;

    fn i24(value: i32) -> I24 {
        I24::new(value).unwrap()
    }

    fn u24(value: i32) -> U24 {
        U24::new(value).unwrap()
    }

    fn assert_scaled<T>(format: &str, expected: [T; 3])
    where
        T: Sample + FromSample<i16> + Debug,
    {
        let [_, silence, _] = expected;
        let mut frames = vec![T::from_sample(i16::MAX); SOURCE.len() + OVERSIZED_BY];

        write_frames(&SOURCE, &mut frames);

        assert_eq!(frames[..SOURCE.len()], expected, "{format}: scaled audio");
        assert_eq!(
            frames[SOURCE.len()..],
            [silence; OVERSIZED_BY],
            "{format}: the frames the source did not reach"
        );
    }

    #[test]
    fn every_format_gets_the_source_at_full_scale_and_its_own_silence_after_it() {
        assert_scaled::<i8>("i8", [i8::MIN, 0, 0x40]);
        assert_scaled::<i16>("i16", SOURCE);
        assert_scaled::<I24>("i24", [i24(-0x80_0000), i24(0), i24(0x40_0000)]);
        assert_scaled::<i32>("i32", [i32::MIN, 0, 0x4000_0000]);
        assert_scaled::<i64>("i64", [i64::MIN, 0, 0x4000_0000_0000_0000]);
        assert_scaled::<u8>("u8", [0, 0x80, 0xC0]);
        assert_scaled::<u16>("u16", [0, 0x8000, 0xC000]);
        assert_scaled::<U24>("u24", [u24(0), u24(0x80_0000), u24(0xC0_0000)]);
        assert_scaled::<u32>("u32", [0, 0x8000_0000, 0xC000_0000]);
        assert_scaled::<u64>("u64", [0, 0x8000_0000_0000_0000, 0xC000_0000_0000_0000]);
        assert_scaled::<f32>("f32", [-1.0, 0.0, 0.5]);
        assert_scaled::<f64>("f64", [-1.0, 0.0, 0.5]);
    }

    #[test]
    fn a_short_destination_takes_what_fits_and_leaves_the_rest_of_the_source() {
        let mut frames = [0_i16; 2];

        write_frames(&[1, 2, 3], &mut frames);

        assert_eq!(frames, [1, 2]);
    }

    #[test]
    fn an_empty_source_silences_the_whole_destination() {
        let mut frames = [0_u8; 4];

        write_frames(&[], &mut frames);

        assert_eq!(frames, [0x80; 4]);
    }

    fn levels<T>(silence: T, half: T, full: T) -> [f32; 3]
    where
        T: Copy,
        f32: FromSample<T>,
    {
        [
            peak(&[silence, silence]),
            peak(&[silence, half]),
            peak(&[half, full, silence]),
        ]
    }

    #[test]
    fn every_format_peaks_on_the_same_unit_scale() {
        for (format, measured) in [
            ("i8", levels::<i8>(0, 0x40, i8::MIN)),
            ("i16", levels::<i16>(0, 0x4000, i16::MIN)),
            ("i24", levels(i24(0), i24(0x40_0000), i24(-0x80_0000))),
            ("i32", levels::<i32>(0, 0x4000_0000, i32::MIN)),
            ("i64", levels::<i64>(0, 0x4000_0000_0000_0000, i64::MIN)),
            ("u8", levels::<u8>(0x80, 0xC0, 0)),
            ("u16", levels::<u16>(0x8000, 0xC000, 0)),
            ("u24", levels(u24(0x80_0000), u24(0xC0_0000), u24(0))),
            ("u32", levels::<u32>(0x8000_0000, 0xC000_0000, 0)),
            (
                "u64",
                levels::<u64>(0x8000_0000_0000_0000, 0xC000_0000_0000_0000, 0),
            ),
            ("f32", levels::<f32>(0.0, 0.5, -1.0)),
            ("f64", levels::<f64>(0.0, 0.5, -1.0)),
        ] {
            for (measured, expected) in measured.into_iter().zip([0.0, 0.5, 1.0]) {
                assert!(
                    (measured - expected).abs() < LEVEL_TOLERANCE,
                    "{format}: measured {measured}, expected {expected}"
                );
            }
        }
    }

    #[test]
    fn a_peak_ignores_nan_wherever_it_sits_in_the_block() {
        for (block, expected) in [
            (&[f32::NAN][..], 0.0),
            (&[f32::NAN, 0.25], 0.25),
            (&[0.25, f32::NAN], 0.25),
            (&[-0.25, f32::NAN, 0.125], 0.25),
        ] {
            assert_eq!(peak(block), expected, "block {block:?}");
        }
    }

    #[test]
    fn an_empty_block_measures_silence() {
        assert_eq!(peak::<i16>(&[]), 0.0);
    }
}
