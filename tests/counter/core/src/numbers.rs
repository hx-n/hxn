//! Each number a binding converts, given back so a test sees what arrived.

/// Numbers narrower than the two that cross, and the unsigned 32 bits.
#[hxn::interface]
pub struct Numbers;

#[hxn::methods]
impl Numbers {
    pub fn byte(value: u8) -> u8 {
        value
    }

    pub fn small(value: i8) -> i8 {
        value
    }

    pub fn word(value: u16) -> u16 {
        value
    }

    pub fn short(value: i16) -> i16 {
        value
    }

    /// Every bit of `bits` turned over.
    pub fn inverted(bits: u32) -> u32 {
        !bits
    }

    /// Twice `count`, which may be more than fits going back.
    pub fn doubled(count: usize) -> usize {
        count.saturating_mul(2)
    }

    pub fn negated(by: isize) -> isize {
        -by
    }

    /// A third of `value`, to the precision an `f32` has.
    pub fn third(value: f32) -> f32 {
        value / 3.0
    }

    /// A length, checked by the library after the binding let it through.
    pub fn sized(count: usize) -> Result<usize, &'static str> {
        if count > 100 {
            return Err("too many");
        }
        Ok(count)
    }
}
