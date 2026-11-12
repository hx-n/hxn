use std::any::TypeId;
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::Fault;

/// An opaque type. A host holds a pointer to it in place of a Rust value and
/// only passes that pointer back.
#[repr(C)]
pub struct Opaque {
    _private: [u8; 0],
}

pub type RawHandle = *mut Opaque;

const MAGIC: u64 = u64::from_le_bytes(*b"hxn:hndl");
const EXCLUSIVE: usize = usize::MAX;

// The first field of every `Handle<T>`, so it can be read before `T` is known.
#[repr(C)]
struct Header {
    magic: AtomicU64,
    type_id: TypeId,
    // 0 when free, `EXCLUSIVE` when mutably borrowed, otherwise the number of
    // shared borrows.
    borrows: AtomicUsize,
    free: unsafe fn(*mut Header),
}

/// A Rust value owned by a host object.
///
/// The value and its allocation have separate lifetimes. [`take`](Self::take)
/// ends the value's lifetime and is what `close()` calls. The allocation lives
/// until the host's garbage collector calls [`release`], so a pointer the host
/// can still reach is never dangling, and using a handle after close returns
/// an error instead of crashing.
///
/// Borrows are checked at run time, because a host is not bound by `&` and
/// `&mut`: it can call from two threads, or re-enter from a callback.
/// A conflicting borrow fails with [`Fault::Busy`] instead of blocking.
///
/// `T: Send` because a garbage collector runs finalizers on whichever thread
/// it runs on.
#[repr(C)]
pub struct Handle<T> {
    header: Header,
    value: UnsafeCell<Option<T>>,
}

// The borrow count makes this equivalent to a non-blocking
// `RwLock<Option<T>>`. `borrow` adds its own `T: Sync` bound.
unsafe impl<T: Send> Send for Handle<T> {}
unsafe impl<T: Send> Sync for Handle<T> {}

impl<T: Send + 'static> Handle<T> {
    /// Moves `value` to the heap and returns the pointer the host keeps.
    pub fn into_raw(value: T) -> RawHandle {
        let handle = Box::new(Self {
            header: Header {
                magic: AtomicU64::new(MAGIC),
                type_id: TypeId::of::<T>(),
                borrows: AtomicUsize::new(0),
                free: free::<T>,
            },
            value: UnsafeCell::new(Some(value)),
        });
        Box::into_raw(handle).cast()
    }

    /// Returns the handle `raw` points to, if it is a handle to a `T`.
    ///
    /// # Safety
    /// `raw` is null or was returned by `into_raw` in this library, for any
    /// type, and is not released during `'a`.
    pub unsafe fn from_raw<'a>(raw: *const Opaque) -> Result<&'a Self, Fault> {
        let header = raw.cast::<Header>();
        if header.is_null() {
            return Err(Fault::Null);
        }
        if !header.is_aligned() {
            return Err(Fault::InvalidHandle);
        }
        let found = unsafe { &*header };
        if found.magic.load(Ordering::Relaxed) != MAGIC || found.type_id != TypeId::of::<T>() {
            return Err(Fault::InvalidHandle);
        }
        Ok(unsafe { &*raw.cast::<Self>() })
    }

    pub fn borrow(&self) -> Result<Ref<'_, T>, Fault>
    where
        T: Sync,
    {
        let borrows = &self.header.borrows;
        let mut current = borrows.load(Ordering::Relaxed);
        loop {
            if current >= EXCLUSIVE - 1 {
                return Err(Fault::Busy);
            }
            match borrows.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(seen) => current = seen,
            }
        }
        match unsafe { &*self.value.get() } {
            Some(value) => Ok(Ref { value, borrows }),
            None => {
                borrows.fetch_sub(1, Ordering::Release);
                Err(Fault::Closed)
            }
        }
    }

    pub fn borrow_mut(&self) -> Result<RefMut<'_, T>, Fault> {
        self.lock()?;
        let borrows = &self.header.borrows;
        match unsafe { &mut *self.value.get() } {
            Some(value) => Ok(RefMut { value, borrows }),
            None => {
                borrows.store(0, Ordering::Release);
                Err(Fault::Closed)
            }
        }
    }

    /// Takes the value out and leaves the handle closed. Returns `None` if it
    /// was already closed, so closing twice is not an error.
    pub fn take(&self) -> Result<Option<T>, Fault> {
        self.lock()?;
        let value = unsafe { (*self.value.get()).take() };
        self.header.borrows.store(0, Ordering::Release);
        Ok(value)
    }

    fn lock(&self) -> Result<(), Fault> {
        self.header
            .borrows
            .compare_exchange(0, EXCLUSIVE, Ordering::Acquire, Ordering::Relaxed)
            .map(drop)
            .map_err(|_| Fault::Busy)
    }
}

pub struct Ref<'a, T> {
    value: &'a T,
    borrows: &'a AtomicUsize,
}

impl<T> Deref for Ref<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

impl<T> Drop for Ref<'_, T> {
    fn drop(&mut self) {
        self.borrows.fetch_sub(1, Ordering::Release);
    }
}

pub struct RefMut<'a, T> {
    value: &'a mut T,
    borrows: &'a AtomicUsize,
}

impl<T> Deref for RefMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

impl<T> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<T> Drop for RefMut<'_, T> {
    fn drop(&mut self) {
        self.borrows.store(0, Ordering::Release);
    }
}

unsafe fn free<T>(header: *mut Header) {
    drop(unsafe { Box::from_raw(header.cast::<Handle<T>>()) });
}

/// Frees a handle of any type, dropping its value if it was never taken.
/// Only a host object's finalizer calls this.
///
/// A handle that is still borrowed is leaked instead: a call is using it, so
/// the host was wrong to treat it as unreachable. A panic from the value's
/// `Drop` is caught and discarded here, because there is no caller to report
/// it to.
///
/// # Safety
/// `raw` is null or was returned by [`Handle::into_raw`] in this library, and
/// the host does not use it again.
pub unsafe extern "C" fn release(raw: RawHandle) {
    let header = raw.cast::<Header>();
    if header.is_null() || !header.is_aligned() {
        return;
    }
    let found = unsafe { &*header };
    if found.magic.load(Ordering::Relaxed) != MAGIC
        || found
            .borrows
            .compare_exchange(0, EXCLUSIVE, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
    {
        return;
    }
    found.magic.store(0, Ordering::Relaxed);
    let free = found.free;
    let _ = std::panic::catch_unwind(|| unsafe { free(header) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Tracked {
        _alive: Arc<()>,
        count: i32,
    }

    fn tracked(value: i32) -> (Arc<()>, RawHandle) {
        let alive = Arc::new(());
        (
            alive.clone(),
            Handle::into_raw(Tracked {
                _alive: alive,
                count: value,
            }),
        )
    }

    fn open<'a, T: Send + 'static>(raw: RawHandle) -> Result<&'a Handle<T>, Fault> {
        unsafe { Handle::from_raw(raw) }
    }

    #[test]
    fn borrows_then_releases() {
        let (alive, raw) = tracked(41);
        let handle = open::<Tracked>(raw).unwrap();
        handle.borrow_mut().unwrap().count += 1;
        assert_eq!(handle.borrow().unwrap().count, 42);
        assert_eq!(Arc::strong_count(&alive), 2);
        unsafe { release(raw) };
        assert_eq!(Arc::strong_count(&alive), 1);
    }

    #[test]
    fn conflicting_borrows_are_busy() {
        let raw = Handle::into_raw(7i32);
        let handle = open::<i32>(raw).unwrap();
        {
            let first = handle.borrow().unwrap();
            let second = handle.borrow().unwrap();
            assert_eq!(*first + *second, 14);
            assert_eq!(handle.borrow_mut().err(), Some(Fault::Busy));
            assert_eq!(handle.take().err(), Some(Fault::Busy));
        }
        {
            let _writer = handle.borrow_mut().unwrap();
            assert_eq!(handle.borrow().err(), Some(Fault::Busy));
            assert_eq!(handle.borrow_mut().err(), Some(Fault::Busy));
        }
        assert!(handle.borrow_mut().is_ok());
        unsafe { release(raw) };
    }

    #[test]
    fn closed_is_an_error_and_closing_twice_is_not() {
        let (alive, raw) = tracked(1);
        let handle = open::<Tracked>(raw).unwrap();
        assert!(handle.take().unwrap().is_some());
        assert_eq!(Arc::strong_count(&alive), 1);
        assert!(handle.take().unwrap().is_none());
        assert_eq!(handle.borrow().err(), Some(Fault::Closed));
        assert_eq!(handle.borrow_mut().err(), Some(Fault::Closed));
        // A failed borrow restores the borrow count.
        assert!(handle.take().is_ok());
        unsafe { release(raw) };
    }

    #[test]
    fn refuses_what_is_not_its_handle() {
        let raw = Handle::into_raw(7i32);
        assert_eq!(open::<u32>(raw).err(), Some(Fault::InvalidHandle));
        assert_eq!(open::<i32>(std::ptr::null_mut()).err(), Some(Fault::Null));
        assert_eq!(
            open::<i32>(raw.wrapping_byte_add(1)).err(),
            Some(Fault::InvalidHandle)
        );
        let mut other = [0u64; 8];
        assert_eq!(
            open::<i32>(other.as_mut_ptr().cast()).err(),
            Some(Fault::InvalidHandle)
        );
        unsafe { release(other.as_mut_ptr().cast()) };
        unsafe { release(std::ptr::null_mut()) };
        unsafe { release(raw) };
    }

    #[test]
    fn a_borrowed_handle_is_not_freed() {
        let (alive, raw) = tracked(1);
        let handle = open::<Tracked>(raw).unwrap();
        {
            let _reader = handle.borrow().unwrap();
            unsafe { release(raw) };
            assert_eq!(Arc::strong_count(&alive), 2);
        }
        unsafe { release(raw) };
        assert_eq!(Arc::strong_count(&alive), 1);
    }

    #[test]
    fn a_second_thread_is_refused_not_blocked() {
        let raw = Handle::into_raw(0u64);
        let handle = open::<u64>(raw).unwrap();
        let writer = handle.borrow_mut().unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| assert_eq!(handle.borrow_mut().err(), Some(Fault::Busy)));
        });
        drop(writer);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..1000 {
                        if let Ok(mut value) = handle.borrow_mut() {
                            *value += 1;
                        }
                    }
                });
            }
        });
        assert!(*handle.borrow().unwrap() > 0);
        unsafe { release(raw) };
    }

    #[test]
    fn a_panicking_drop_stops_at_release() {
        struct Loud;
        impl Drop for Loud {
            fn drop(&mut self) {
                panic!("dropped loudly");
            }
        }
        unsafe { release(Handle::into_raw(Loud)) };
    }
}
