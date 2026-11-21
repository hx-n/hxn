//! Typed buffers: each function borrows a Haxe buffer's elements for the
//! call, one function for each element type.

#[hxn::interface]
pub struct Buffers;

#[hxn::methods]
impl Buffers {
    pub fn sum(values: &[f32]) -> f64 {
        values.iter().copied().map(f64::from).sum()
    }

    /// Multiplies each of `values` by `by`, in place.
    pub fn scale(values: &mut [f32], by: f64) {
        for value in values {
            *value = (f64::from(*value) * by) as f32;
        }
    }

    pub fn mean(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        values.iter().sum::<f64>() / values.len() as f64
    }

    /// Writes each of `from`, widened, to `into`, and returns how many fit.
    pub fn widen(from: &[i16], into: &mut [i32]) -> i32 {
        let count = from.len().min(into.len());
        for (out, value) in into.iter_mut().zip(from) {
            *out = i32::from(*value);
        }
        i32::try_from(count).unwrap_or(i32::MAX)
    }

    /// The number of bits set across `words`.
    pub fn ones(words: &[u32]) -> u32 {
        words.iter().map(|word| word.count_ones()).sum()
    }

    /// Writes each element's index to it, counting down from `last`.
    pub fn countdown(indices: &mut [u16], last: u16) {
        for (at, index) in indices.iter_mut().enumerate() {
            *index = last.wrapping_sub(at as u16);
        }
    }

    /// Adds `by` to each of `values`, in place.
    pub fn shift(values: &mut [i32], by: i32) {
        for value in values {
            *value = value.wrapping_add(by);
        }
    }

    /// The sum of `values`, which the work copies before the call returns.
    pub async fn later_sum(values: &[f32]) -> f64 {
        Self::sum(values)
    }
}
