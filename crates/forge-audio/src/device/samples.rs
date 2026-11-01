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
