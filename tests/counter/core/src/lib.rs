//! A library small enough to read at a glance, with one of each thing a
//! binding has to carry: a value with a destructor, a fallible method, text
//! and bytes in, text out, a function called back, and results that arrive
//! later.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::task::Poll;
use std::thread::sleep;
use std::time::Duration;

use hxn::AtomicFlag;
use hxn_future::{
    Completer, Failure, Future, Sender, Signal, Source, Stream, channel, pending, signal, spawn,
    spawn_blocking, stepped,
};

static LIVE: AtomicI32 = AtomicI32::new(0);

/// How many counters exist. A binding that leaks or double-frees shows here.
pub fn live() -> i32 {
    LIVE.load(Ordering::SeqCst)
}

/// A number that is added to, with one of each thing a binding carries.
#[hxn::interface]
pub struct Counter {
    value: i32,
    // Cells the host reads and writes without a call.
    adds: Arc<AtomicI32>,
    locked: Arc<AtomicFlag>,
}

#[derive(Debug)]
pub struct Overflow;

impl fmt::Display for Overflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("counter overflow")
    }
}

// What the generator exposes. The adapters under `c` and `hl` are written
// by hand and cover the rest of the file as well.
#[hxn::methods]
impl Counter {
    pub fn new(start: i32) -> Self {
        LIVE.fetch_add(1, Ordering::SeqCst);
        Self {
            value: start,
            adds: Arc::new(AtomicI32::new(0)),
            locked: Arc::new(AtomicFlag::new(false)),
        }
    }

    /// Adds to the value and returns it. Fails if it would overflow. A
    /// locked counter leaves the value as it is.
    pub fn add(&mut self, amount: i32) -> Result<i32, Overflow> {
        if self.locked.load() {
            return Ok(self.value);
        }
        self.value = self.value.checked_add(amount).ok_or(Overflow)?;
        self.adds.fetch_add(1, Ordering::SeqCst);
        Ok(self.value)
    }

    /// How many adds have been made, as a cell the host reads directly.
    pub fn adds(&self) -> Arc<AtomicI32> {
        self.adds.clone()
    }

    /// Set to make the counter ignore adds.
    pub fn locked(&self) -> Arc<AtomicFlag> {
        self.locked.clone()
    }

    /// Does `units` of work, counting each in `progress`, and stops early
    /// once `cancel` is set. Returns how many were done.
    #[hxn(blocking)]
    pub fn work(&self, units: i32, progress: &AtomicI32, cancel: &AtomicFlag) -> i32 {
        let mut done = 0;
        while done < units && !cancel.load() {
            progress.fetch_add(1, Ordering::SeqCst);
            done += 1;
        }
        done
    }

    pub fn label(&self, name: &str) -> String {
        format!("{name}={}", self.value)
    }

    pub fn sum(&self, data: &[u8]) -> i32 {
        data.iter().map(|&byte| i32::from(byte)).sum()
    }

    pub fn is_positive(&self) -> bool {
        self.value > 0
    }

    pub fn scaled(&self, factor: f64) -> f64 {
        f64::from(self.value) * factor
    }

    /// Sets the value, or its negation.
    pub fn set(&mut self, value: i32, negate: bool) {
        self.value = if negate { -value } else { value };
    }

    /// Adds another counter's value to this one.
    pub fn absorb(&mut self, other: &Counter) -> Result<i32, Overflow> {
        self.add(other.value)
    }

    /// Exchanges values with another counter.
    pub fn swap(&mut self, other: &mut Counter) {
        std::mem::swap(&mut self.value, &mut other.value);
    }

    /// A new counter starting at this one's value.
    pub fn fork(&self) -> Counter {
        Counter::new(self.value)
    }

    /// A counter at the larger of the two values, or nothing without
    /// another to compare with.
    pub fn larger(&self, other: Option<&Counter>) -> Option<Counter> {
        Some(Counter::new(self.value.max(other?.value)))
    }

    /// Gives the counter up for its value.
    pub fn into_value(self) -> i32 {
        self.value
    }

    /// How many counters exist.
    pub fn live() -> i32 {
        live()
    }

    /// Panics, to show what a binding makes of that.
    pub fn fail() {
        panic!("counter failed on request");
    }

    /// Sleeps, as a call that waits on something would.
    #[hxn(blocking)]
    pub fn nap(milliseconds: i32) {
        sleep(Duration::from_millis(milliseconds.max(0) as u64));
    }
}

impl Counter {
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
}

impl Drop for Counter {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Work on a thread of the library's own, watched and stopped through two
/// cells.
#[hxn::interface]
pub struct Job {
    done: Arc<AtomicI32>,
    stop: Arc<AtomicFlag>,
}

#[hxn::methods]
impl Job {
    /// Starts counting to `units`, one a millisecond.
    pub fn new(units: i32) -> Self {
        let done = Arc::new(AtomicI32::new(0));
        let stop = Arc::new(AtomicFlag::new(false));
        let (counted, stopped) = (done.clone(), stop.clone());
        // Nobody waits on this work; it is the cells that are watched.
        drop(spawn_blocking(move || {
            for _ in 0..units {
                if stopped.load() {
                    break;
                }
                sleep(Duration::from_millis(1));
                counted.fetch_add(1, Ordering::SeqCst);
            }
            Ok::<_, Failure>(())
        }));
        Self { done, stop }
    }

    pub fn done(&self) -> Arc<AtomicI32> {
        self.done.clone()
    }

    pub fn stop(&self) -> Arc<AtomicFlag> {
        self.stop.clone()
    }
}

/// A running total of counters: one interface that takes and makes another.
#[hxn::interface]
pub struct Tally {
    total: i32,
}

#[hxn::methods]
impl Tally {
    pub fn new() -> Self {
        Self { total: 0 }
    }

    /// Adds a counter's value to the total and returns it.
    pub fn record(&mut self, counter: &Counter) -> Result<i32, Overflow> {
        self.total = self.total.checked_add(counter.value).ok_or(Overflow)?;
        Ok(self.total)
    }

    pub fn total(&self) -> i32 {
        self.total
    }

    /// A counter starting at the total.
    pub fn to_counter(&self) -> Counter {
        Counter::new(self.total)
    }
}

impl Default for Tally {
    fn default() -> Self {
        Self::new()
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

/// 1 to `limit`, one a step, and then the end, or a failure in its place.
pub fn count_up(limit: i32, fail: bool) -> Stream<i32> {
    let (sender, stream) = channel();
    let mut next = 1;
    // Nobody waits on this work; it is the stream that is read.
    drop(stepped(move || {
        if next > limit {
            if fail {
                sender.fail(Overflow);
            }
            return Poll::Ready(Ok::<_, Failure>(()));
        }
        // A reader that has gone is the sign to stop.
        if !sender.send(next) {
            return Poll::Ready(Ok(()));
        }
        next += 1;
        Poll::Pending
    }));
    stream
}

/// A stream fed from outside the library, as a host's events would be.
pub struct Emitter {
    sender: Sender<i32>,
    stream: Option<Stream<i32>>,
}

impl Emitter {
    pub fn new() -> Self {
        let (sender, stream) = channel();
        Self {
            sender,
            stream: Some(stream),
        }
    }

    /// The stream, the first time it is asked for.
    pub fn stream(&mut self) -> Option<Stream<i32>> {
        self.stream.take()
    }

    pub fn emit(&self, value: i32) -> bool {
        self.sender.send(value)
    }

    pub fn end(&self) -> bool {
        self.sender.close()
    }
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}

/// Counts from 1 to `limit`, one a step, as a value that changes.
pub fn ramp(limit: i32) -> Signal<i32> {
    let (source, ramp) = signal(0);
    let mut next = 1;
    drop(stepped(move || {
        // A reader that has gone is the sign to stop.
        if next > limit || !source.set(next) {
            return Poll::Ready(Ok::<_, Failure>(()));
        }
        next += 1;
        Poll::Pending
    }));
    ramp
}

/// A value set from outside the library and read back as a signal, as a
/// library's own state would be.
pub struct Gauge {
    source: Source<i32>,
    signal: Option<Signal<i32>>,
}

impl Gauge {
    pub fn new(initial: i32) -> Self {
        let (source, signal) = signal(initial);
        Self {
            source,
            signal: Some(signal),
        }
    }

    /// The signal, the first time it is asked for.
    pub fn signal(&mut self) -> Option<Signal<i32>> {
        self.signal.take()
    }

    pub fn set(&self, value: i32) -> bool {
        self.source.set(value)
    }
}
