//! The wire format for plain-data values: a value is serialized as its parts
//! in order, written by one side's generated code and read by the other's.
//!
//! | Part | Bytes |
//! |---|---|
//! | `bool` | one, 0 or 1 |
//! | an `i32`, a narrower integer, a C-like enum | four, little-endian |
//! | an `i64`, a `u64` | eight, little-endian |
//! | an `f64`, an `f32` | eight, little-endian, as an `f64` |
//! | text, bytes | a length as an `i32`, then that many bytes; text is UTF-8 |
//! | an `Option` | one byte, 0 or 1, then the value if 1 |
//! | a list or set | a count as an `i32`, then each item |
//! | a defaulted record field | a presence byte, then the value if present; Rust fills absent values |
//! | a record | its fields in declaration order, or one zero byte if empty |
//! | a tagged enum | the variant's index, starting at 0, as an `i32`, then its fields |
//! | a map | a count as an `i32`, then each key and its value |
//!
//! The format is not self-describing: both sides are generated from the same
//! declaration, and a reader that runs out of bytes or has bytes left over
//! fails.

use crate::Fault;

/// Deserializes the parts of a value from bytes a host sent.
pub struct Reader<'a> {
    left: &'a [u8],
    depth: u32,
}

impl<'a> Reader<'a> {
    /// The maximum nesting depth of a recursive value. A recursive value is
    /// read by a recursive function, and the input bytes determine how deep
    /// it recurses.
    pub const DEPTH: u32 = 128;

    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            left: bytes,
            depth: 0,
        }
    }

    /// Called on entry by the reader of a record or an enum, either of which
    /// may contain another.
    pub fn enter(&mut self) -> Result<(), Fault> {
        if self.depth == Self::DEPTH {
            return Err(Fault::Depth);
        }
        self.depth += 1;
        Ok(())
    }

    /// Called once that value has been read.
    pub fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], Fault> {
        if count > self.left.len() {
            return Err(Fault::Encoding);
        }
        let (taken, left) = self.left.split_at(count);
        self.left = left;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Fault> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub fn bool(&mut self) -> Result<bool, Fault> {
        match self.take(1)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Fault::Encoding),
        }
    }

    pub fn i32(&mut self) -> Result<i32, Fault> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn i64(&mut self) -> Result<i64, Fault> {
        Ok(i64::from_le_bytes(self.array()?))
    }

    pub fn f64(&mut self) -> Result<f64, Fault> {
        Ok(f64::from_le_bytes(self.array()?))
    }

    /// Reads a length or a count: a non-negative `i32`.
    pub fn count(&mut self) -> Result<usize, Fault> {
        usize::try_from(self.i32()?).map_err(|_| Fault::Encoding)
    }

    pub fn bytes(&mut self) -> Result<Vec<u8>, Fault> {
        let length = self.count()?;
        Ok(self.take(length)?.to_vec())
    }

    pub fn text(&mut self) -> Result<String, Fault> {
        String::from_utf8(self.bytes()?).map_err(|_| Fault::Encoding)
    }

    /// Reads the item count of a list whose items take at least `least` bytes
    /// each. A count larger than the remaining bytes can hold is rejected
    /// before anything is allocated for it.
    pub fn items(&mut self, least: usize) -> Result<usize, Fault> {
        let count = self.count()?;
        if count.saturating_mul(least.max(1)) > self.left.len() {
            return Err(Fault::Encoding);
        }
        Ok(count)
    }

    /// Checks that every byte was read, which holds after a complete value.
    pub fn end(&self) -> Result<(), Fault> {
        if self.left.is_empty() {
            Ok(())
        } else {
            Err(Fault::Encoding)
        }
    }
}

pub fn bool(out: &mut Vec<u8>, value: bool) {
    out.push(u8::from(value));
}

pub fn i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Writes a length or a count. Fails if it does not fit in an `i32`.
pub fn count(out: &mut Vec<u8>, value: usize) -> Result<(), Fault> {
    i32(out, i32::try_from(value).map_err(|_| Fault::TooLarge)?);
    Ok(())
}

pub fn bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<(), Fault> {
    count(out, value.len())?;
    out.extend_from_slice(value);
    Ok(())
}

pub fn text(out: &mut Vec<u8>, value: &str) -> Result<(), Fault> {
    bytes(out, value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_is_read_back_in_order() {
        let mut out = Vec::new();
        bool(&mut out, true);
        i32(&mut out, -7);
        i64(&mut out, 1 << 40);
        f64(&mut out, 2.5);
        text(&mut out, "naïve ☃").unwrap();
        bytes(&mut out, &[1, 2, 3]).unwrap();
        count(&mut out, 2).unwrap();
        i32(&mut out, 10);
        i32(&mut out, 20);

        let mut input = Reader::new(&out);
        assert_eq!(input.bool(), Ok(true));
        assert_eq!(input.i32(), Ok(-7));
        assert_eq!(input.i64(), Ok(1 << 40));
        assert_eq!(input.f64(), Ok(2.5));
        assert_eq!(input.text().as_deref(), Ok("naïve ☃"));
        assert_eq!(input.bytes(), Ok(vec![1, 2, 3]));
        assert_eq!(input.items(4), Ok(2));
        assert_eq!((input.i32(), input.i32()), (Ok(10), Ok(20)));
        assert_eq!(input.end(), Ok(()));
    }

    #[test]
    fn a_value_nests_only_so_deep() {
        let mut input = Reader::new(&[]);
        for _ in 0..Reader::DEPTH {
            assert_eq!(input.enter(), Ok(()));
        }
        assert_eq!(input.enter(), Err(Fault::Depth));
        input.leave();
        assert_eq!(input.enter(), Ok(()));
    }

    #[test]
    fn bytes_that_are_not_a_value_are_refused() {
        // Runs out of bytes.
        assert_eq!(Reader::new(&[1, 2]).i32(), Err(Fault::Encoding));
        // A bool that is neither 0 nor 1.
        assert_eq!(Reader::new(&[2]).bool(), Err(Fault::Encoding));
        // A negative length, and one longer than what follows.
        assert_eq!(
            Reader::new(&(-1_i32).to_le_bytes()).bytes(),
            Err(Fault::Encoding)
        );
        let mut long = 9_i32.to_le_bytes().to_vec();
        long.extend_from_slice(b"short");
        assert_eq!(Reader::new(&long).text(), Err(Fault::Encoding));
        // Text that is not UTF-8.
        let mut bad = 1_i32.to_le_bytes().to_vec();
        bad.push(0xff);
        assert_eq!(Reader::new(&bad).text(), Err(Fault::Encoding));
        // A count larger than the remaining bytes can hold, rejected before a
        // list is allocated for it.
        assert_eq!(
            Reader::new(&i32::MAX.to_le_bytes()).items(4),
            Err(Fault::Encoding)
        );
        // Bytes left over.
        let mut input = Reader::new(&[0, 0]);
        assert_eq!(input.bool(), Ok(false));
        assert_eq!(input.end(), Err(Fault::Encoding));
    }
}
