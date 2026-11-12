//! A small fixture library with one of each feature a binding has to
//! support: a type with a destructor, a fallible method, string and byte
//! parameters, a string return value, a callback, and asynchronous results.

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

// Interfaces defined in submodules are re-exported at the crate root, where
// the adapters look for them.
mod blob;
mod events;
mod numbers;
mod shapes;
pub use blob::Blob;
pub use events::{CountError, Event, Events, Refusal, Trouble};
// Functions defined in submodules are re-exported at the root as well.
pub use numbers::{Numbers, halve};
pub use shapes::{Mode, Point, Settings, Shapes};

static LIVE: AtomicI32 = AtomicI32::new(0);

/// The number of live counters. A leak or double free in a binding shows up
/// in this count.
pub fn live() -> i32 {
    LIVE.load(Ordering::SeqCst)
}

/// Returns the library's name and version. A function with no `class`
/// argument becomes a static method of the class named after the library,
/// which here is the `Counter` class.
#[hxn::function]
pub fn version() -> String {
    String::from("counter 1")
}

/// Clamps `value` to the range `low` to `high`. Fails if `low` is greater
/// than `high`.
#[hxn::function(class = "Tools")]
pub fn clamp(value: i32, low: i32, high: i32) -> Result<i32, &'static str> {
    if low > high {
        return Err("low is above high");
    }
    Ok(value.clamp(low, high))
}

/// Returns the sum of two counters' values.
#[hxn::function(class = "Tools")]
pub fn total(first: &Counter, second: &Counter) -> i32 {
    first.value + second.value
}

/// Copies as much of `from` as fits into `into`, and returns how many bytes
/// it copied.
#[hxn::function(class = "Tools")]
pub fn copy(from: &[u8], into: &mut [u8]) -> i32 {
    let count = from.len().min(into.len());
    into[..count].copy_from_slice(&from[..count]);
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// Creates a counter from a free function instead of a `Counter` method.
#[hxn::function(class = "Tools")]
pub fn starting_at(value: i32) -> Counter {
    Counter::new(value)
}

/// An integer counter, with one method for each feature a binding supports.
#[hxn::interface]
pub struct Counter {
    value: i32,
    // Atomics the host reads and writes directly, without a call.
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

// The methods the generator exposes. The hand-written adapters under `c`
// and `hl` also cover the rest of the file.
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

    /// Returns `count` bytes, each set to the value's low byte. Fails for a
    /// negative count.
    pub fn fill(&self, count: i32) -> Result<Vec<u8>, &'static str> {
        let count = usize::try_from(count).map_err(|_| "a negative count")?;
        Ok(vec![self.value.to_le_bytes()[0]; count])
    }

    /// Adds `amount` to the value and returns the new value. Fails on
    /// overflow. A locked counter leaves the value unchanged.
    pub fn add(&mut self, amount: i32) -> Result<i32, Overflow> {
        if self.locked.load() {
            return Ok(self.value);
        }
        self.value = self.value.checked_add(amount).ok_or(Overflow)?;
        self.adds.fetch_add(1, Ordering::SeqCst);
        Ok(self.value)
    }

    /// The number of adds made, as an atomic the host reads directly.
    pub fn adds(&self) -> Arc<AtomicI32> {
        self.adds.clone()
    }

    /// A flag that makes the counter ignore adds while it is set.
    pub fn locked(&self) -> Arc<AtomicFlag> {
        self.locked.clone()
    }

    /// Does `units` of work, incrementing `progress` for each unit, and stops
    /// early once `cancel` is set. Returns the number of units done.
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

    /// Adds `other`'s value, consuming it, and returns the new value.
    pub fn consume(&mut self, other: Counter) -> i32 {
        self.value += other.value;
        self.value
    }

    /// `parts` new counters, starting at this one's value and counting up.
    pub fn split(&self, parts: i32) -> Vec<Counter> {
        (0..parts).map(|at| Counter::new(self.value + at)).collect()
    }

    /// Writes the value's low byte into every byte of `data`.
    pub fn stamp(&self, data: &mut [u8]) {
        data.fill(self.value as u8);
    }

    pub fn is_positive(&self) -> bool {
        self.value > 0
    }

    pub fn scaled(&self, factor: f64) -> f64 {
        f64::from(self.value) * factor
    }

    /// Sets the value, negated if `negate` is set.
    pub fn set(&mut self, value: i32, negate: bool) {
        self.value = if negate { -value } else { value };
    }

    /// Adds another counter's value to this one.
    pub fn absorb(&mut self, other: &Counter) -> Result<i32, Overflow> {
        self.add(other.value)
    }

    /// Swaps values with another counter.
    pub fn swap(&mut self, other: &mut Counter) {
        std::mem::swap(&mut self.value, &mut other.value);
    }

    /// A new counter starting at this one's value.
    pub fn fork(&self) -> Counter {
        Counter::new(self.value)
    }

    /// Returns a counter holding the larger of the two values, or `None` if
    /// `other` is `None`.
    pub fn larger(&self, other: Option<&Counter>) -> Option<Counter> {
        Some(Counter::new(self.value.max(other?.value)))
    }

    /// Consumes the counter and returns its value.
    pub fn into_value(self) -> i32 {
        self.value
    }

    /// Consumes the counter in an async function that returns its value
    /// later.
    pub async fn into_value_later(self, milliseconds: i32) -> i32 {
        let _ = square_later(1, millis(milliseconds)).await;
        self.value
    }

    /// The number of live counters.
    pub fn live() -> i32 {
        live()
    }

    /// Panics, to test how a binding handles a panic.
    pub fn fail() {
        panic!("counter failed on request");
    }

    /// Sleeps, to stand in for a blocking call.
    #[hxn(blocking)]
    pub fn nap(milliseconds: i32) {
        sleep(Duration::from_millis(milliseconds.max(0) as u64));
    }
}

// Methods that take a host callback. Each returns the callback's error if
// the callback fails.
#[hxn::methods]
impl Counter {
    /// Calls `f` with each number from 1 to `count` and half that number,
    /// and stops at the first failure.
    pub fn each<E>(count: i32, mut f: impl FnMut(i32, f64) -> Result<(), E>) -> Result<(), E> {
        for n in 1..=count {
            f(n, f64::from(n) / 2.0)?;
        }
        Ok(())
    }

    /// Returns the result of calling `f` with the value.
    pub fn test<E>(&self, f: impl Fn(i32) -> Result<bool, E>) -> Result<bool, E> {
        f(self.value)
    }

    /// Returns the result of calling `f` with half the value and whether
    /// the value is odd.
    pub fn scaled_by<E>(&self, f: impl Fn(f64, bool) -> Result<f64, E>) -> Result<f64, E> {
        f(f64::from(self.value) / 2.0, self.value % 2 != 0)
    }

    /// Calls `f` with a name and the value and returns its result. The
    /// callback takes a borrowed string and returns an owned one.
    pub fn described<E>(&self, f: impl Fn(&str, i32) -> Result<String, E>) -> Result<String, E> {
        let made = f("naïve ☃ 𝄞", self.value)?;
        Ok(format!("[{made}]"))
    }

    /// Returns the number of words in `text` for which `f` returns true.
    pub fn words<E>(text: &str, mut f: impl FnMut(&str) -> Result<bool, E>) -> Result<i32, E> {
        let mut kept = 0;
        for word in text.split_whitespace() {
            if f(word)? {
                kept += 1;
            }
        }
        Ok(kept)
    }

    /// Returns the number of characters in the string `f` returns.
    pub fn titled<E>(f: impl FnOnce() -> Result<String, E>) -> Result<i32, E> {
        Ok(f()?.chars().count() as i32)
    }

    /// Calls `f` with the value's bytes in chunks of `size`, and returns the
    /// number of bytes passed.
    pub fn chunks<E>(
        &self,
        size: i32,
        mut f: impl FnMut(&[u8], i32) -> Result<(), E>,
    ) -> Result<i32, E> {
        let bytes = self.value.to_le_bytes();
        let size = usize::try_from(size).unwrap_or(1).max(1);
        let mut shown = 0;
        for (index, chunk) in bytes.chunks(size).enumerate() {
            f(chunk, index as i32)?;
            shown += chunk.len() as i32;
        }
        f(&[], -1)?;
        Ok(shown)
    }

    /// Creates `count` counters with values 1, 2 and so on, and passes
    /// ownership of each to `f`. Returns the sum of `f`'s return values.
    pub fn spawn_each<E>(
        count: i32,
        mut f: impl FnMut(Counter, &str) -> Result<i32, E>,
    ) -> Result<i32, E> {
        let mut total = 0;
        for n in 1..=count {
            total += f(Counter::new(n), "made")?;
        }
        Ok(total)
    }

    /// Returns what `f` returns.
    pub fn ask<E>(f: impl FnOnce() -> Result<i32, E>) -> Result<i32, E> {
        f()
    }

    /// Returns a counter starting at the value `f` returns.
    pub fn from_fn<E>(f: impl FnOnce() -> Result<i32, E>) -> Result<Counter, E> {
        Ok(Counter::new(f()?))
    }

    /// Returns what `f` returns, or -1 if it fails. Here the library
    /// discards the callback's error.
    pub fn attempt<E>(f: impl FnOnce() -> Result<i32, E>) -> i32 {
        f().unwrap_or(-1)
    }

    /// Replaces the value with `f(value)`, `times` times. Stops at the first
    /// failure and keeps the value from the last successful call.
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

/// Work on a thread the library starts, monitored and stopped through two
/// atomics.
#[hxn::interface]
pub struct Job {
    done: Arc<AtomicI32>,
    stop: Arc<AtomicFlag>,
}

#[hxn::methods]
impl Job {
    /// Starts counting to `units`, one per millisecond.
    pub fn new(units: i32) -> Self {
        let done = Arc::new(AtomicI32::new(0));
        let stop = Arc::new(AtomicFlag::new(false));
        let (counted, stopped) = (done.clone(), stop.clone());
        // Nobody awaits this future. Callers watch the atomics instead.
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

/// A running total of counters: an interface whose methods accept and
/// return another interface.
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

fn millis(milliseconds: i32) -> Duration {
    Duration::from_millis(u64::try_from(milliseconds).unwrap_or(0))
}

/// Asynchronous results as the generator binds them: a class of static
/// functions with no instance.
#[hxn::interface]
pub struct Later;

#[hxn::methods]
impl Later {
    /// The square of `value`, after a delay.
    pub fn square(value: i32, milliseconds: i32) -> Future<i32> {
        square_later(value, millis(milliseconds))
    }

    pub fn greet(name: &str, milliseconds: i32) -> Future<String> {
        greet_later(name.to_owned(), millis(milliseconds))
    }

    /// The sum of 1 to `limit`, adding one term per step.
    pub fn sum(limit: i32) -> Future<i32> {
        sum_later(limit)
    }

    /// One async task that awaits `square` and `sum` and adds the results.
    pub fn combined(value: i32, limit: i32) -> Future<i32> {
        combined_later(value, limit)
    }

    pub fn is_even(value: i32) -> Future<bool> {
        spawn_blocking(move || Ok::<_, Overflow>(value % 2 == 0))
    }

    pub fn half(value: i32) -> Future<f64> {
        spawn_blocking(move || Ok::<_, Overflow>(f64::from(value) / 2.0))
    }

    /// A counter created by the task and transferred to the caller when the
    /// result is taken.
    pub fn counter(start: i32) -> Future<Counter> {
        spawn_blocking(move || Ok::<_, Overflow>(Counter::new(start)))
    }

    /// The same for a shared value.
    pub fn pot(start: i32) -> Future<Arc<Pot>> {
        spawn_blocking(move || Ok::<_, Overflow>(Arc::new(Pot::new(start))))
    }

    /// A future that is never settled.
    pub fn abandoned() -> Future<i32> {
        abandoned()
    }

    /// Returns an error immediately for a negative limit, and otherwise
    /// behaves as `sum`.
    pub fn checked(limit: i32) -> Result<Future<i32>, &'static str> {
        if limit < 0 {
            return Err("a negative limit");
        }
        Ok(sum_later(limit))
    }

    /// Yields 1 to `limit` one item at a time, then ends or, with `fail`,
    /// fails.
    pub fn count_up(limit: i32, fail: bool) -> Stream<i32> {
        count_up(limit, fail)
    }

    /// A stream of `count` counters with values 1, 2 and so on.
    pub fn counters(count: i32) -> Stream<Counter> {
        let (sender, stream) = channel();
        let mut next = 1;
        drop(stepped(move || {
            if next > count || !sender.send(Counter::new(next)) {
                return Poll::Ready(Ok::<_, Failure>(()));
            }
            next += 1;
            Poll::Pending
        }));
        stream
    }

    /// `square` as an async function: the function body is the task.
    pub async fn squared(value: i32, milliseconds: i32) -> Result<i32, Failure> {
        square_later(value, millis(milliseconds)).await
    }

    /// String and byte parameters of an async function are owned copies,
    /// because it runs after the call that passed them has returned.
    /// A counter at `value` once some time has passed, or none for 0.
    pub async fn maybe_counter(value: i32) -> Option<Counter> {
        let _ = square_later(1, millis(1)).await;
        (value != 0).then(|| Counter::new(value))
    }

    /// The sum of two counters' values, taking both, once some time has
    /// passed.
    pub async fn total_later(first: Counter, second: Option<Counter>) -> i32 {
        let _ = square_later(1, millis(1)).await;
        first.value + second.map_or(0, |second| second.value)
    }

    pub async fn shout(name: &str, data: &[u8]) -> String {
        let greeting = greet_later(name.to_owned(), millis(1))
            .await
            .unwrap_or_default();
        format!("{greeting}! {}", data.len())
    }

    /// Returns no value, only completion.
    pub async fn settle(milliseconds: i32) {
        let _ = square_later(1, millis(milliseconds)).await;
    }

    /// Returns no value, or an error.
    pub async fn agree(really: bool) -> Result<(), &'static str> {
        if really { Ok(()) } else { Err("refused later") }
    }

    /// A counter created by an async function.
    pub async fn counted(start: i32) -> Counter {
        let _ = square_later(1, millis(1)).await;
        Counter::new(start)
    }

    /// A timestamp derived from `value`: a future that resolves to a 64-bit
    /// integer.
    pub async fn stamp(value: i32) -> i64 {
        let _ = square_later(1, millis(1)).await;
        i64::from(value) << 32
    }

    /// `count` bytes, each set to `fill`, created by the async function.
    pub async fn frame(count: i32, fill: i32) -> Result<Vec<u8>, &'static str> {
        let count = usize::try_from(count).map_err(|_| "a negative size")?;
        Ok(vec![fill as u8; count])
    }

    /// A stream of frames with lengths from 1 to `count` bytes.
    pub fn frames(count: i32) -> Stream<Vec<u8>> {
        let (sender, stream) = channel();
        let mut size = 1;
        drop(stepped(move || {
            if size > count || !sender.send(vec![size as u8; size as usize]) {
                return Poll::Ready(Ok::<_, Failure>(()));
            }
            size += 1;
            Poll::Pending
        }));
        stream
    }

    /// A signal of a count that does not fit in 32 bits, updated as it
    /// increases.
    pub fn odometer(limit: i32) -> Signal<i64> {
        let base = 1_i64 << 40;
        let (source, odometer) = signal(base);
        let mut next = 1;
        drop(stepped(move || {
            if next > limit || !source.set(base + i64::from(next)) {
                return Poll::Ready(Ok::<_, Failure>(()));
            }
            next += 1;
            Poll::Pending
        }));
        odometer
    }

    /// A stream of `count` unit items. Each carries no data and only reports
    /// that something happened.
    pub fn ticks(count: i32) -> Stream<()> {
        let (sender, stream) = channel();
        let mut sent = 0;
        drop(stepped(move || {
            if sent >= count || !sender.send(()) {
                return Poll::Ready(Ok::<_, Failure>(()));
            }
            sent += 1;
            Poll::Pending
        }));
        stream
    }

    /// A signal whose value increases from 0 to `limit`.
    pub fn ramp(limit: i32) -> Signal<i32> {
        ramp(limit)
    }

    /// The same, with the value as a string.
    pub fn label(limit: i32) -> Signal<String> {
        let (source, label) = signal(String::from("0"));
        let mut next = 1;
        drop(stepped(move || {
            if next > limit || !source.set(next.to_string()) {
                return Poll::Ready(Ok::<_, Failure>(()));
            }
            next += 1;
            Poll::Pending
        }));
        label
    }
}

static POTS: AtomicI32 = AtomicI32::new(0);

/// A total with several contributors. It is shared: the library and any
/// number of Haxe objects hold references to the same value, which lives
/// until the last reference is dropped.
#[hxn::interface(shared)]
pub struct Pot {
    total: AtomicI32,
}

#[hxn::methods]
impl Pot {
    pub fn new(start: i32) -> Self {
        POTS.fetch_add(1, Ordering::SeqCst);
        Self {
            total: AtomicI32::new(start),
        }
    }

    /// Adds to the total and returns it.
    pub fn add(&self, amount: i32) -> i32 {
        self.total.fetch_add(amount, Ordering::SeqCst) + amount
    }

    pub fn total(&self) -> i32 {
        self.total.load(Ordering::SeqCst)
    }

    /// Adds after a delay. An async method of a shared value holds a
    /// reference to it while it runs, whatever happens to the object it was
    /// called on.
    pub async fn add_later(&self, amount: i32, milliseconds: i32) -> i32 {
        let _ = square_later(1, millis(milliseconds)).await;
        self.add(amount)
    }

    /// Adds another pot's total, and a third pot's total if one is given.
    pub async fn pour(&self, from: &Pot, also: Option<Arc<Pot>>) -> Arc<Pot> {
        let _ = square_later(1, millis(1)).await;
        self.add(from.total());
        if let Some(also) = also {
            self.add(also.total());
        }
        Arc::new(Pot::new(self.total()))
    }

    /// Passes `f` a reference to `pot` and a new pot holding half its total,
    /// and adds `f`'s return value to the total.
    pub fn offer<E>(
        pot: Arc<Pot>,
        f: impl Fn(Arc<Pot>, Arc<Pot>) -> Result<i32, E>,
    ) -> Result<i32, E> {
        let half = Arc::new(Pot::new(pot.total() / 2));
        Ok(pot.add(f(Arc::clone(&pot), half)?))
    }

    /// A separate pot starting at this one's total, returned as a shared
    /// reference.
    pub fn split(&self) -> Arc<Pot> {
        Arc::new(Pot::new(self.total()))
    }

    /// Two new pots, holding this one's total and twice it.
    pub fn pair(&self) -> Vec<Arc<Pot>> {
        vec![
            Arc::new(Pot::new(self.total())),
            Arc::new(Pot::new(self.total() * 2)),
        ]
    }

    /// The number of live pots, regardless of how many references each has.
    pub fn live() -> i32 {
        POTS.load(Ordering::SeqCst)
    }
}

impl Drop for Pot {
    fn drop(&mut self) {
        POTS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A player that pays into a pot and holds a shared reference to it.
#[hxn::interface]
pub struct Player {
    pot: Arc<Pot>,
}

#[hxn::methods]
impl Player {
    pub fn new(pot: Arc<Pot>) -> Self {
        Self { pot }
    }

    /// Adds to the pot and returns its total.
    pub fn pay(&self, amount: i32) -> i32 {
        self.pot.add(amount)
    }

    /// The pot this player pays into. Each call returns a reference to the
    /// same pot.
    pub fn pot(&self) -> Arc<Pot> {
        Arc::clone(&self.pot)
    }

    /// Whether this player pays into `pot`.
    pub fn pays_into(&self, pot: &Pot) -> bool {
        std::ptr::eq(Arc::as_ptr(&self.pot), pot)
    }

    /// Switches to another pot, or keeps the current one if given `None`.
    /// Returns the previous pot.
    pub fn move_to(&mut self, pot: Option<Arc<Pot>>) -> Option<Arc<Pot>> {
        pot.map(|pot| std::mem::replace(&mut self.pot, pot))
    }

    /// A new pot holding the larger total of this player's pot and `other`,
    /// or `None` if `other` is `None`.
    pub fn fuller(&self, other: Option<&Pot>) -> Option<Pot> {
        other.map(|other| Pot::new(self.pot.total().max(other.total())))
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

/// The sum of 1 to `limit`, adding one term per step.
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

/// The square of `value` plus the sum of 1 to `limit`, each computed by one
/// of the functions above.
pub fn combined_later(value: i32, limit: i32) -> Future<i32> {
    spawn(async move {
        let square = square_later(value, Duration::from_millis(5)).await?;
        let sum = sum_later(limit).await?;
        Ok::<_, Failure>(square.checked_add(sum).ok_or(Overflow)?)
    })
}

/// A future that is never settled.
pub fn abandoned() -> Future<i32> {
    pending().1
}

/// A future settled from outside the library, the way a host callback would
/// settle one.
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

    /// Returns the future on the first call, and `None` afterwards.
    pub fn future(&mut self) -> Option<Future<i32>> {
        self.future.take()
    }

    /// Returns a future of twice the value instead of the plain future.
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

/// Yields 1 to `limit`, one per step, and then ends, or fails instead if
/// `fail` is set.
pub fn count_up(limit: i32, fail: bool) -> Stream<i32> {
    let (sender, stream) = channel();
    let mut next = 1;
    // Nobody awaits this future. Callers read the stream instead.
    drop(stepped(move || {
        if next > limit {
            if fail {
                sender.fail(Overflow);
            }
            return Poll::Ready(Ok::<_, Failure>(()));
        }
        // Stop once the reader has been dropped.
        if !sender.send(next) {
            return Poll::Ready(Ok(()));
        }
        next += 1;
        Poll::Pending
    }));
    stream
}

/// A stream fed from outside the library, the way host events would feed
/// one.
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

    /// Returns the stream on the first call, and `None` afterwards.
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

/// A signal that counts from 1 to `limit`, one per step.
pub fn ramp(limit: i32) -> Signal<i32> {
    let (source, ramp) = signal(0);
    let mut next = 1;
    drop(stepped(move || {
        // Stop once the reader has been dropped.
        if next > limit || !source.set(next) {
            return Poll::Ready(Ok::<_, Failure>(()));
        }
        next += 1;
        Poll::Pending
    }));
    ramp
}

/// A value set from outside the library and read back as a signal, the way
/// a library would expose its own state.
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

    /// Returns the signal on the first call, and `None` afterwards.
    pub fn signal(&mut self) -> Option<Signal<i32>> {
        self.signal.take()
    }

    pub fn set(&self, value: i32) -> bool {
        self.source.set(value)
    }
}
