//! Tagged enums, maps, typed errors, and data carried by futures, streams,
//! signals and callbacks.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::task::Poll;

use hxn_future::{Failure, Signal, Stream, channel, signal, stepped};

use crate::{Counter, Mode, Point, millis, square_later};

/// An event: a tagged enum whose variants carry different fields.
#[hxn::data]
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Started,
    /// Progress so far.
    Progress {
        done: u32,
        total: u32,
    },
    Message(String),
    Moved(Point, Mode),
    /// A list of nested events.
    Batch(Vec<Event>),
    Wrapped(Box<Event>),
    Labelled {
        labels: HashMap<String, i32>,
        order: BTreeMap<i32, String>,
    },
    Maybe(Option<Box<Event>>),
}

/// Why a count was rejected: a typed error with one variant per reason.
#[hxn::data]
#[derive(Clone, Debug, PartialEq)]
pub enum CountError {
    Empty,
    TooMany { limit: u32, given: u32 },
    Unknown(String),
}

impl fmt::Display for CountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("nothing to count"),
            Self::TooMany { limit, given } => write!(f, "{given} is more than {limit}"),
            Self::Unknown(name) => write!(f, "unknown: {name}"),
        }
    }
}

/// A typed error that is a struct.
#[hxn::data]
#[derive(Clone, Debug, PartialEq)]
pub struct Refusal {
    pub code: i32,
    pub reason: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "refused ({}): {}", self.code, self.reason)
    }
}

/// A typed error that is a C-like enum.
#[hxn::data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trouble {
    Minor = 1,
    Major = 5,
}

impl fmt::Display for Trouble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Minor => "minor trouble",
            Self::Major => "major trouble",
        })
    }
}

/// Static functions over events, with no instance.
#[hxn::interface]
pub struct Events;

#[hxn::methods]
impl Events {
    /// The nesting depth of the events in `event`.
    pub fn depth(event: &Event) -> i32 {
        match event {
            Event::Batch(events) => 1 + events.iter().map(Self::depth).max().unwrap_or(0),
            Event::Wrapped(inner) | Event::Maybe(Some(inner)) => 1 + Self::depth(inner),
            _ => 0,
        }
    }

    /// A string showing the event as the library received it.
    pub fn show(event: &Event) -> String {
        format!("{event:?}")
    }

    /// One event of each variant.
    pub fn history() -> Vec<Event> {
        vec![
            Event::Started,
            Event::Progress { done: 3, total: 8 },
            Event::Message(String::from("naïve ☃")),
            Event::Moved(Point { x: 1.5, y: -2.0 }, Mode::Loud),
            Event::Batch(vec![Event::Started, Event::Message(String::from("in"))]),
            Event::Wrapped(Box::new(Event::Progress { done: 1, total: 1 })),
            Event::Labelled {
                labels: HashMap::from([(String::from("a"), 1)]),
                order: BTreeMap::from([(2, String::from("two")), (1, String::from("one"))]),
            },
            Event::Maybe(None),
            Event::Maybe(Some(Box::new(Event::Started))),
        ]
    }

    /// The event wrapped `times` times.
    pub fn wrapped(event: Event, times: i32) -> Event {
        (0..times).fold(event, |inner, _| Event::Wrapped(Box::new(inner)))
    }

    /// The names keyed by their counts. Takes a hash map and returns a
    /// sorted map.
    pub fn inverted(counts: &HashMap<String, i32>) -> BTreeMap<i32, String> {
        counts
            .iter()
            .map(|(name, count)| (*count, name.clone()))
            .collect()
    }

    /// The count, or a typed error giving the reason it was rejected.
    pub fn checked(count: u32) -> Result<u32, CountError> {
        match count {
            0 => Err(CountError::Empty),
            1..=3 => Ok(count),
            given => Err(CountError::TooMany { limit: 3, given }),
        }
    }

    /// The mode called `name`, or an error carrying the name.
    pub fn mode_named(name: &str) -> Result<Mode, CountError> {
        match name {
            "quiet" => Ok(Mode::Quiet),
            "loud" => Ok(Mode::Loud),
            other => Err(CountError::Unknown(other.to_owned())),
        }
    }

    /// Returns no value, or a struct error describing the refusal.
    pub fn refuse(code: i32) -> Result<(), Refusal> {
        if code == 0 {
            return Ok(());
        }
        Err(Refusal {
            code,
            reason: String::from("not today ☃"),
        })
    }

    /// The level, or the `Trouble` that level corresponds to.
    pub fn trouble(level: i32) -> Result<i32, Trouble> {
        match level {
            1 => Err(Trouble::Minor),
            5 => Err(Trouble::Major),
            other => Ok(other),
        }
    }

    /// A future that resolves to a struct, or fails with the typed error of
    /// the task that would have produced it.
    pub async fn checked_later(count: u32) -> Result<Point, CountError> {
        let _ = square_later(1, millis(1)).await;
        let count = Self::checked(count)?;
        Ok(Point {
            x: f64::from(count),
            y: -f64::from(count),
        })
    }

    /// A future that resolves to an enum.
    pub async fn mode_later(mode: Mode) -> Mode {
        let _ = square_later(1, millis(1)).await;
        crate::Shapes::louder(mode)
    }

    /// A future that resolves to a narrow integer.
    pub async fn small_later(value: i32) -> u8 {
        value as u8
    }

    /// A future that resolves to a list of structs.
    pub async fn path_later(count: i32) -> Vec<Point> {
        (0..count)
            .map(|step| Point {
                x: f64::from(step),
                y: 0.0,
            })
            .collect()
    }

    /// A future that resolves to an optional number.
    pub async fn maybe_later(value: i32) -> Option<i32> {
        (value >= 0).then_some(value)
    }

    /// A stream whose typed error follows its preceding values.
    pub fn checked_feed(fail: bool) -> Stream<Result<i32, CountError>> {
        let (sender, stream) = channel();
        sender.send(Ok(1));
        sender.send(Ok(2));
        if fail {
            sender.send(Err(CountError::Unknown(String::from("stream ☃"))));
        }
        sender.send(Ok(99));
        stream
    }

    pub fn checked_points() -> Stream<Result<Point, Refusal>> {
        let (sender, stream) = channel();
        sender.send(Ok(Point { x: 3.0, y: 4.0 }));
        sender.send(Err(Refusal {
            code: 8,
            reason: String::from("stream refused"),
        }));
        stream
    }

    pub fn checked_ticks() -> Stream<Result<(), Trouble>> {
        let (sender, stream) = channel();
        sender.send(Ok(()));
        sender.send(Err(Trouble::Major));
        stream
    }

    pub fn checked_bytes() -> Stream<Result<Vec<u8>, CountError>> {
        let (sender, stream) = channel();
        sender.send(Ok(b"hello".to_vec()));
        sender.send(Err(CountError::Empty));
        stream
    }

    pub fn checked_objects() -> Stream<Result<Counter, CountError>> {
        let (sender, stream) = channel();
        sender.send(Ok(Counter::new(7)));
        sender.send(Err(CountError::Empty));
        sender.send(Ok(Counter::new(9)));
        stream
    }

    pub fn checked_optional() -> Stream<Result<Option<Counter>, CountError>> {
        let (sender, stream) = channel();
        sender.send(Ok(None));
        sender.send(Ok(Some(Counter::new(8))));
        sender.send(Err(CountError::Empty));
        stream
    }

    pub fn checked_batches() -> Stream<Result<Vec<Counter>, CountError>> {
        let (sender, stream) = channel();
        sender.send(Ok(vec![Counter::new(5), Counter::new(6)]));
        sender.send(Ok(vec![]));
        sender.send(Err(CountError::Empty));
        sender.send(Ok(vec![Counter::new(99)]));
        stream
    }

    /// A stream of the first `count` events of `history`.
    pub fn feed(count: i32) -> Stream<Event> {
        let (sender, stream) = channel();
        let mut events = Self::history()
            .into_iter()
            .take(usize::try_from(count).unwrap_or(0));
        drop(stepped(move || {
            let sent = events.next().is_some_and(|event| sender.send(event));
            if sent {
                Poll::Pending
            } else {
                Poll::Ready(Ok::<_, Failure>(()))
            }
        }));
        stream
    }

    /// A signal of a mode that changes from quiet to off.
    pub fn mode_now() -> Signal<Mode> {
        let (source, mode) = signal(Mode::Quiet);
        let mut next = [Mode::Loud, Mode::Off].into_iter();
        drop(stepped(move || match next.next() {
            Some(mode) if source.set(mode) => Poll::Pending,
            _ => Poll::Ready(Ok::<_, Failure>(())),
        }));
        mode
    }

    /// A signal of a point whose `x` increases until it reaches `limit`.
    pub fn place_now(limit: i32) -> Signal<Point> {
        let (source, place) = signal(Point { x: 0.0, y: 1.0 });
        let mut next = 1;
        drop(stepped(move || {
            let place = Point {
                x: f64::from(next),
                y: 1.0,
            };
            if next > limit || !source.set(place) {
                return Poll::Ready(Ok::<_, Failure>(()));
            }
            next += 1;
            Poll::Pending
        }));
        place
    }

    /// The result of `f` for each point. The callback takes a borrowed
    /// struct and returns an enum.
    pub fn each_point<E>(
        points: &[Point],
        mut f: impl FnMut(&Point) -> Result<Mode, E>,
    ) -> Result<Vec<Mode>, E> {
        points.iter().map(&mut f).collect()
    }

    /// The event `f` returns for `event`. The callback takes an owned event
    /// and returns one.
    pub fn reshaped<E>(
        event: Event,
        f: impl FnOnce(Event) -> Result<Event, E>,
    ) -> Result<Event, E> {
        f(event)
    }

    /// The point `f` picks, if any. The callback takes a list borrowed as a
    /// slice, and an enum.
    pub fn picked<E>(
        points: Vec<Point>,
        f: impl Fn(&[Point], Mode) -> Result<Option<Point>, E>,
    ) -> Result<Option<Point>, E> {
        f(&points, Mode::Loud)
    }

    /// The bytes `f` returns for 1, followed by the bytes it returns for 2.
    pub fn stamped<E>(f: impl Fn(i32) -> Result<Vec<u8>, E>) -> Result<Vec<u8>, E> {
        let mut out = f(1)?;
        out.extend(f(2)?);
        Ok(out)
    }

    /// The result of `f` for quiet. The callback takes an enum and returns
    /// one, with no other values.
    pub fn graded<E>(f: impl Fn(Mode) -> Result<Mode, E>) -> Result<Mode, E> {
        f(Mode::Quiet)
    }
}
