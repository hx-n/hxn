//! One function for each numeric type a binding converts. Each returns its
//! argument so a test can check the value the library received.

/// Integer types narrower than the wire types, unsigned types, and 64-bit
/// types.
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

    /// `bits` with every bit inverted.
    pub fn inverted(bits: u32) -> u32 {
        !bits
    }

    /// Twice `count`, which may be too large to convert back.
    pub fn doubled(count: usize) -> usize {
        count.saturating_mul(2)
    }

    pub fn negated(by: isize) -> isize {
        -by
    }

    pub fn wide(value: i64) -> i64 {
        value
    }

    /// The square of `value`, which may not fit in 32 bits.
    pub fn squared(value: i32) -> i64 {
        i64::from(value) * i64::from(value)
    }

    /// `bits` with all 64 bits inverted.
    pub fn inverted_wide(bits: u64) -> u64 {
        !bits
    }

    /// A third of `value`, at `f32` precision.
    pub fn third(value: f32) -> f32 {
        value / 3.0
    }

    /// A length the library validates after the binding has accepted it.
    pub fn sized(count: usize) -> Result<usize, &'static str> {
        if count > 100 {
            return Err("too many");
        }
        Ok(count)
    }
}

/// Half of `value`: a free function in a submodule, bound to a class the
/// library names.
#[hxn::function(class = "Tools")]
pub fn halve(value: f64) -> f64 {
    value / 2.0
}
