use std::ops::{Deref, DerefMut};

use crate::Fault;

/// A number a typed buffer holds: `f32`, `f64`, `i32`, `u32`, `i16` or
/// `u16`. Every bit pattern of its size is one of its values, which is what
/// lets a buffer's bytes be viewed as its elements.
pub trait Element: Copy + sealed::Sealed {
    #[doc(hidden)]
    fn read(bytes: &[u8]) -> Self;
    #[doc(hidden)]
    fn write(self, bytes: &mut [u8]);
}

mod sealed {
    pub trait Sealed {}
}

macro_rules! element {
    ($($ty:ty),*) => {$(
        impl sealed::Sealed for $ty {}

        impl Element for $ty {
            fn read(bytes: &[u8]) -> Self {
                Self::from_ne_bytes(bytes.try_into().expect("one element's bytes"))
            }

            fn write(self, bytes: &mut [u8]) {
                bytes.copy_from_slice(&self.to_ne_bytes());
            }
        }
    )*};
}

element!(f32, f64, i32, u32, i16, u16);

// The number of elements `bytes` holds, failing when it is not whole.
fn count<T>(bytes: usize) -> Result<usize, Fault> {
    let size = size_of::<T>();
    if bytes.is_multiple_of(size) {
        Ok(bytes / size)
    } else {
        Err(Fault::TooLarge)
    }
}

fn aligned<T>(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.as_ptr().cast::<T>().is_aligned()
}

/// A typed buffer's bytes, borrowed for a call, viewed as its elements: in
/// place when they are aligned for `T`, and otherwise copied.
pub struct Elements<'a, T: Element>(View<'a, T>);

enum View<'a, T> {
    Lent(&'a [T]),
    Copied(Vec<T>),
}

impl<'a, T: Element> Elements<'a, T> {
    /// Fails with [`Fault::TooLarge`] when the length is not a whole number
    /// of elements.
    pub fn new(bytes: &'a [u8]) -> Result<Self, Fault> {
        let len = count::<T>(bytes.len())?;
        if aligned::<T>(bytes) {
            // SAFETY: the bytes are aligned for `T`, readable for `len`
            // elements for `'a`, and any bit pattern is a `T`.
            let lent = unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<T>(), len) };
            return Ok(Self(View::Lent(lent)));
        }
        let copied = bytes.chunks_exact(size_of::<T>()).map(T::read).collect();
        Ok(Self(View::Copied(copied)))
    }
}

impl<T: Element> Deref for Elements<'_, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        match &self.0 {
            View::Lent(lent) => lent,
            View::Copied(copied) => copied,
        }
    }
}

/// A typed buffer's bytes, borrowed mutably for a call, viewed as its
/// elements. When they are not aligned for `T`, the call works on a copy,
/// which is written back to the bytes when this is dropped.
pub struct ElementsMut<'a, T: Element>(ViewMut<'a, T>);

enum ViewMut<'a, T> {
    Lent(&'a mut [T]),
    Copied { bytes: &'a mut [u8], copy: Vec<T> },
}

impl<'a, T: Element> ElementsMut<'a, T> {
    /// Fails with [`Fault::TooLarge`] when the length is not a whole number
    /// of elements.
    pub fn new(bytes: &'a mut [u8]) -> Result<Self, Fault> {
        let len = count::<T>(bytes.len())?;
        if aligned::<T>(bytes) {
            // SAFETY: as for `Elements::new`, and the bytes are borrowed
            // mutably for `'a`, so this is their only writer.
            let lent =
                unsafe { std::slice::from_raw_parts_mut(bytes.as_mut_ptr().cast::<T>(), len) };
            return Ok(Self(ViewMut::Lent(lent)));
        }
        let copy = bytes.chunks_exact(size_of::<T>()).map(T::read).collect();
        Ok(Self(ViewMut::Copied { bytes, copy }))
    }
}

impl<T: Element> Deref for ElementsMut<'_, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        match &self.0 {
            ViewMut::Lent(lent) => lent,
            ViewMut::Copied { copy, .. } => copy,
        }
    }
}

impl<T: Element> DerefMut for ElementsMut<'_, T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match &mut self.0 {
            ViewMut::Lent(lent) => lent,
            ViewMut::Copied { copy, .. } => copy,
        }
    }
}

impl<T: Element> Drop for ElementsMut<'_, T> {
    fn drop(&mut self) {
        if let ViewMut::Copied { bytes, copy } = &mut self.0 {
            for (element, out) in copy.iter().zip(bytes.chunks_exact_mut(size_of::<T>())) {
                element.write(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(align(8))]
    struct Aligned([u8; 17]);

    fn floats(values: &[f32]) -> Aligned {
        let mut out = Aligned([0; 17]);
        for (value, bytes) in values.iter().zip(out.0.as_chunks_mut::<4>().0) {
            *bytes = value.to_ne_bytes();
        }
        out
    }

    #[test]
    fn aligned_bytes_are_viewed_in_place() {
        let storage = floats(&[1.0, 2.5, -3.0, 4.0]);
        let bytes = &storage.0[..16];
        let view = Elements::<f32>::new(bytes).unwrap();
        assert_eq!(&*view, &[1.0, 2.5, -3.0, 4.0]);
        assert_eq!(view.as_ptr().cast::<u8>(), bytes.as_ptr());
    }

    #[test]
    fn unaligned_bytes_are_copied() {
        let mut storage = Aligned([0; 17]);
        storage.0[1..5].copy_from_slice(&7i32.to_ne_bytes());
        storage.0[5..9].copy_from_slice(&(-9i32).to_ne_bytes());
        let view = Elements::<i32>::new(&storage.0[1..9]).unwrap();
        assert_eq!(&*view, &[7, -9]);
    }

    #[test]
    fn writes_reach_the_bytes_either_way() {
        let mut storage = floats(&[1.0, 2.0, 3.0, 4.0]);
        {
            let mut view = ElementsMut::<f32>::new(&mut storage.0[..16]).unwrap();
            view[3] = 8.0;
        }
        assert_eq!(&storage.0[12..16], &8f32.to_ne_bytes());
        {
            let mut view = ElementsMut::<u16>::new(&mut storage.0[1..7]).unwrap();
            view.copy_from_slice(&[0x0102, 0x0304, 0xfffe]);
        }
        let mut expected = Vec::new();
        for value in [0x0102u16, 0x0304, 0xfffe] {
            expected.extend_from_slice(&value.to_ne_bytes());
        }
        assert_eq!(&storage.0[1..7], &expected[..]);
    }

    #[test]
    fn a_partial_element_is_refused() {
        assert_eq!(Elements::<f64>::new(&[0; 12]).err(), Some(Fault::TooLarge));
        assert_eq!(
            ElementsMut::<i16>::new(&mut [0; 3]).err(),
            Some(Fault::TooLarge)
        );
    }

    #[test]
    fn empty_is_empty() {
        assert!(Elements::<f64>::new(&[]).unwrap().is_empty());
        assert!(ElementsMut::<u32>::new(&mut []).unwrap().is_empty());
    }
}
