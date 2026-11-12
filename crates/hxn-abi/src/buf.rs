use std::mem::ManuallyDrop;

/// A byte buffer the library allocated and has transferred to the host.
///
/// The host copies the bytes into its own heap and calls `free` with the
/// other three fields, exactly once. The buffer carries its own `free` because
/// each library has its own allocator, and only the allocator that allocated
/// the bytes can free them. An empty buffer has a null `free` and does not
/// need freeing.
#[repr(C)]
pub struct Buf {
    ptr: *mut u8,
    len: usize,
    cap: usize,
    free: Option<unsafe extern "C" fn(*mut u8, usize, usize)>,
}

impl Buf {
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
            cap: 0,
            free: None,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Default for Buf {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<Vec<u8>> for Buf {
    fn from(bytes: Vec<u8>) -> Self {
        if bytes.capacity() == 0 {
            return Self::empty();
        }
        let mut bytes = ManuallyDrop::new(bytes);
        Self {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
            cap: bytes.capacity(),
            free: Some(free_vec),
        }
    }
}

impl From<String> for Buf {
    fn from(text: String) -> Self {
        text.into_bytes().into()
    }
}

impl From<&str> for Buf {
    fn from(text: &str) -> Self {
        text.as_bytes().to_vec().into()
    }
}

impl From<&[u8]> for Buf {
    fn from(bytes: &[u8]) -> Self {
        bytes.to_vec().into()
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        if let Some(free) = self.free.take() {
            unsafe { free(self.ptr, self.len, self.cap) };
        }
    }
}

// Owns its bytes and shares nothing.
unsafe impl Send for Buf {}
unsafe impl Sync for Buf {}

unsafe extern "C" fn free_vec(ptr: *mut u8, len: usize, cap: usize) {
    drop(unsafe { Vec::from_raw_parts(ptr, len, cap) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_text_and_bytes() {
        assert_eq!(Buf::from("héllo").as_bytes(), "héllo".as_bytes());
        assert_eq!(Buf::from(vec![1u8, 2, 3]).as_bytes(), &[1, 2, 3]);
        let mut spare = Vec::with_capacity(64);
        spare.push(9u8);
        assert_eq!(Buf::from(spare).len(), 1);
    }

    #[test]
    fn empty_has_nothing_to_free() {
        for buf in [Buf::empty(), Buf::from(Vec::new()), Buf::from("")] {
            assert!(buf.is_empty());
            assert!(buf.free.is_none());
            assert_eq!(buf.as_bytes(), &[]);
        }
    }

    #[test]
    fn host_frees_through_the_buffer() {
        // Mimics a host: reads the fields, calls `free`, never runs `Drop`.
        let buf = ManuallyDrop::new(Buf::from(String::from("released by the host")));
        unsafe { (buf.free.unwrap())(buf.ptr, buf.len, buf.cap) };
    }
}
