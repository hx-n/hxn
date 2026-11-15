//! A future settled by a callback from the host, after the call that made it has returned: on
//! Emscripten a JavaScript timer, through `emscripten_async_call`; elsewhere a thread that
//! sleeps. A program in a browser sees the callback only while it leaves the page's event loop
//! free to run. `chunks` runs blocking work on threads where the build has them.

use hxn_future::Future;

#[hxn::interface]
pub struct Timer;

#[hxn::methods]
impl Timer {
    /// Settles with `ms` after about `ms` milliseconds.
    pub fn after(ms: i32) -> Future<i32> {
        later(ms)
    }

    /// Settles with `count` after `count` pieces of blocking work, each on a thread of its own
    /// where the build has threads, and each but the first started from the thread the work
    /// before it resumed on.
    pub fn chunks(count: i32) -> Future<i32> {
        hxn_future::spawn(async move {
            for _ in 0..count {
                let _ = hxn_future::spawn_blocking(|| {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    Ok::<_, hxn_future::Failure>(())
                })
                .await;
            }
            Ok::<_, hxn_future::Failure>(count)
        })
    }
}

#[cfg(target_os = "emscripten")]
fn later(ms: i32) -> Future<i32> {
    use std::ffi::c_void;

    unsafe extern "C" {
        fn emscripten_async_call(f: extern "C" fn(*mut c_void), arg: *mut c_void, millis: i32);
    }

    extern "C" fn fire(arg: *mut c_void) {
        // Made by `Box::into_raw` below, and called once.
        let (completer, ms) =
            *unsafe { Box::from_raw(arg.cast::<(hxn_future::Completer<i32>, i32)>()) };
        completer.resolve(ms);
    }

    let (completer, future) = hxn_future::pending();
    let arg = Box::into_raw(Box::new((completer, ms))).cast::<c_void>();
    unsafe { emscripten_async_call(fire, arg, ms) };
    future
}

#[cfg(not(target_os = "emscripten"))]
fn later(ms: i32) -> Future<i32> {
    hxn_future::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
        Ok::<_, hxn_future::Failure>(ms)
    })
}
