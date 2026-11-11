//! A library small enough to read at a glance, with one of each thing a
//! binding has to carry: a value with a destructor, a fallible method, text
//! and bytes in, text out, and results that arrive later.

use std::fmt;
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread::sleep;
use std::time::Duration;

use hxn_future::{Future, pending, spawn};

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
    spawn(move || {
        sleep(delay);
        value.checked_mul(value).ok_or(Overflow)
    })
}

pub fn greet_later(name: String, delay: Duration) -> Future<String> {
    spawn(move || {
        sleep(delay);
        Ok::<_, Overflow>(format!("hello {name}"))
    })
}

/// A future nothing will settle.
pub fn abandoned() -> Future<i32> {
    pending().1
}
