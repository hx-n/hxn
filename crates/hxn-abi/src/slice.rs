use std::marker::PhantomData;

use crate::Fault;

/// A byte slice borrowed for `'a`: a pointer and a length with C layout.
///
/// As a parameter of an `extern "C"` function, `'a` is the duration of the
/// call, which is how long a host keeps an argument alive. The code that
/// constructs one guarantees that `ptr` is readable for `len` bytes for `'a`.
/// The accessors check everything else.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Slice<'a> {
    ptr: *const u8,
    len: usize,
    life: PhantomData<&'a [u8]>,
}

impl<'a> Slice<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
            life: PhantomData,
        }
    }

    /// # Safety
    /// `ptr` is readable for `len` bytes for `'a` and nothing writes to them
    /// during `'a`, or `len` is zero.
    pub const unsafe fn from_raw(ptr: *const u8, len: usize) -> Self {
        Self {
            ptr,
            len,
            life: PhantomData,
        }
    }

    pub fn as_bytes(self) -> Result<&'a [u8], Fault> {
        check(self.ptr, self.len)?;
        if self.len == 0 {
            return Ok(&[]);
        }
        Ok(unsafe { std::slice::from_raw_parts(self.ptr, self.len) })
    }

    pub fn as_str(self) -> Result<&'a str, Fault> {
        std::str::from_utf8(self.as_bytes()?).map_err(|_| Fault::Encoding)
    }
}

impl<'a> From<&'a [u8]> for Slice<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Self::new(bytes)
    }
}

impl<'a> From<&'a str> for Slice<'a> {
    fn from(text: &'a str) -> Self {
        Self::new(text.as_bytes())
    }
}

/// A byte slice borrowed mutably for `'a`. Not `Copy`, because there must be
/// only one writer.
#[repr(C)]
pub struct SliceMut<'a> {
    ptr: *mut u8,
    len: usize,
    life: PhantomData<&'a mut [u8]>,
}

impl<'a> SliceMut<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
            life: PhantomData,
        }
    }

    /// # Safety
    /// `ptr` is writable for `len` bytes for `'a` and nothing else reads or
    /// writes them during `'a`, or `len` is zero.
    pub const unsafe fn from_raw(ptr: *mut u8, len: usize) -> Self {
        Self {
            ptr,
            len,
            life: PhantomData,
        }
    }

    pub fn into_bytes(self) -> Result<&'a mut [u8], Fault> {
        check(self.ptr, self.len)?;
        if self.len == 0 {
            return Ok(&mut []);
        }
        Ok(unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) })
    }
}

fn check(ptr: *const u8, len: usize) -> Result<(), Fault> {
    if len > isize::MAX as usize {
        Err(Fault::TooLarge)
    } else if len != 0 && ptr.is_null() {
        Err(Fault::Null)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_it_was_built_from() {
        assert_eq!(Slice::from("héllo").as_str(), Ok("héllo"));
        assert_eq!(Slice::new(&[1, 2, 3]).as_bytes(), Ok(&[1u8, 2, 3][..]));
    }

    #[test]
    fn empty_needs_no_pointer() {
        let empty = unsafe { Slice::from_raw(std::ptr::null(), 0) };
        assert_eq!(empty.as_bytes(), Ok(&[][..]));
        assert_eq!(empty.as_str(), Ok(""));
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        let null = unsafe { Slice::from_raw(std::ptr::null(), 4) };
        assert_eq!(null.as_bytes(), Err(Fault::Null));
        let huge = unsafe { Slice::from_raw(std::ptr::dangling(), usize::MAX) };
        assert_eq!(huge.as_bytes(), Err(Fault::TooLarge));
        assert_eq!(Slice::new(&[0xff, 0xfe]).as_str(), Err(Fault::Encoding));
    }

    #[test]
    fn writes_through() {
        let mut bytes = [0u8; 3];
        SliceMut::new(&mut bytes)
            .into_bytes()
            .unwrap()
            .copy_from_slice(&[7, 8, 9]);
        assert_eq!(bytes, [7, 8, 9]);
        let null = unsafe { SliceMut::from_raw(std::ptr::null_mut(), 1) };
        assert_eq!(null.into_bytes().err(), Some(Fault::Null));
    }
}
