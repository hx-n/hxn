//! A library small enough to read at a glance, with one of each thing a
//! binding has to carry: a value with a destructor, a fallible method, text
//! and bytes in, text out, a function called back, and results that arrive
//! later.

use std::fmt;
use std::sync::atomic::{AtomicI32, Ordering};
use std::task::Poll;
use std::thread::sleep;
use std::time::Duration;

use hxn_future::{Completer, Failure, Future, pending, spawn, spawn_blocking, stepped};

static LIVE: AtomicI32 = AtomicI32::new(0);

/// How many counters exist. A binding that leaks or double-frees shows here.
pub fn live() -> i32 {
    LIVE.load(Ordering::SeqCst)
}

pub struct Counter {
    value: i32,
}

#[derive(Debug)]
pub struct Overflow;

impl fmt::Display for Overflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("counter overflow")
    }
}

impl Counter {
    pub fn new(start: i32) -> Self {
        LIVE.fetch_add(1, Ordering::SeqCst);
        Self { value: start }
    }

    pub fn add(&mut self, amount: i32) -> Result<i32, Overflow> {
        self.value = self.value.checked_add(amount).ok_or(Overflow)?;
        Ok(self.value)
    }

    /// Replaces the value with `f` of it, `times` over, and stops at the
    /// first failure with the value as the last success left it.
    pub fn apply<E>(
        &mut self,
        times: i32,
        mut f: impl FnMut(i32) -> Result<i32, E>,
    ) -> Result<i32, E> {
        for _ in 0..times {
            self.value = f(self.value)?;
        }
        Ok(self.value)
    }

    pub fn label(&self, name: &str) -> String {
        format!("{name}={}", self.value)
    }

    pub fn sum(&self, data: &[u8]) -> i32 {
        data.iter().map(|&byte| i32::from(byte)).sum()
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn square_later(value: i32, delay: Duration) -> Future<i32> {
    spawn_blocking(move || {
        sleep(delay);
        value.checked_mul(value).ok_or(Overflow)
    })
}

pub fn greet_later(name: String, delay: Duration) -> Future<String> {
    spawn_blocking(move || {
        sleep(delay);
        Ok::<_, Overflow>(format!("hello {name}"))
    })
}

/// The sum of 1 to `limit`, added one term a step.
pub fn sum_later(limit: i32) -> Future<i32> {
    let mut total: i32 = 0;
    let mut next = 1;
    stepped(move || {
        if next > limit {
            return Poll::Ready(Ok(total));
        }
        match total.checked_add(next) {
            Some(sum) => total = sum,
            None => return Poll::Ready(Err(Overflow)),
        }
        next += 1;
        Poll::Pending
    })
}

/// The square of `value` plus the sum of 1 to `limit`, each worked out by
/// one of the operations above.
pub fn combined_later(value: i32, limit: i32) -> Future<i32> {
    spawn(async move {
        let square = square_later(value, Duration::from_millis(5)).await?;
        let sum = sum_later(limit).await?;
        Ok::<_, Failure>(square.checked_add(sum).ok_or(Overflow)?)
    })
}

/// A future nothing will settle.
pub fn abandoned() -> Future<i32> {
    pending().1
}

/// A future settled from outside the library, as a host callback would.
pub struct Promise {
    completer: Completer<i32>,
    future: Option<Future<i32>>,
}

impl Promise {
    pub fn new() -> Self {
        let (completer, future) = pending();
        Self {
            completer,
            future: Some(future),
        }
    }

    /// The future, the first time it is asked for.
    pub fn future(&mut self) -> Option<Future<i32>> {
        self.future.take()
    }

    /// A future of twice the value, in place of the plain one.
    pub fn doubled(&mut self) -> Option<Future<i32>> {
        let future = self.future.take()?;
        Some(spawn(async move { Ok::<_, Failure>(future.await? * 2) }))
    }

    pub fn resolve(&self, value: i32) -> bool {
        self.completer.resolve(value)
    }
}

impl Default for Promise {
    fn default() -> Self {
        Self::new()
    }
}
