//! Enums whose variants hold values, maps, errors that are data, and data
//! where a result over time or a lent function carries it.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::task::Poll;

use hxn_future::{Failure, Signal, Stream, channel, signal, stepped};

use crate::{Mode, Point, millis, square_later};

/// What happened, each kind with what it holds.
#[hxn::data]
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Started,
    /// How far along.
    Progress {
        done: u32,
        total: u32,
    },
    Message(String),
    Moved(Point, Mode),
    /// Events inside an event.
    Batch(Vec<Event>),
    Wrapped(Box<Event>),
    Labelled {
        labels: HashMap<String, i32>,
        order: BTreeMap<i32, String>,
    },
    Maybe(Option<Box<Event>>),
}

/// Why a count was refused: an error with a case for each reason.
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

/// An error that is a record.
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

/// An error that is a name.
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

/// Functions over events, with no value behind them.
#[hxn::interface]
pub struct Events;

#[hxn::methods]
impl Events {
    /// How deep the events in `event` nest.
    pub fn depth(event: &Event) -> i32 {
        match event {
            Event::Batch(events) => 1 + events.iter().map(Self::depth).max().unwrap_or(0),
            Event::Wrapped(inner) | Event::Maybe(Some(inner)) => 1 + Self::depth(inner),
            _ => 0,
        }
    }

    /// The event as the library read it.
    pub fn show(event: &Event) -> String {
        format!("{event:?}")
    }

    /// One of each case.
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

    /// The event, inside `times` others.
    pub fn wrapped(event: Event, times: i32) -> Event {
        (0..times).fold(event, |inner, _| Event::Wrapped(Box::new(inner)))
    }

    /// The names by their counts: a map in, and a sorted one out.
    pub fn inverted(counts: &HashMap<String, i32>) -> BTreeMap<i32, String> {
        counts
            .iter()
            .map(|(name, count)| (*count, name.clone()))
            .collect()
    }

    /// The count, or which of the reasons it was refused for.
    pub fn checked(count: u32) -> Result<u32, CountError> {
        match count {
            0 => Err(CountError::Empty),
            1..=3 => Ok(count),
            given => Err(CountError::TooMany { limit: 3, given }),
        }
    }

    /// The mode `name` names, or the name back in the error.
    pub fn mode_named(name: &str) -> Result<Mode, CountError> {
        match name {
            "quiet" => Ok(Mode::Quiet),
            "loud" => Ok(Mode::Loud),
            other => Err(CountError::Unknown(other.to_owned())),
        }
    }

    /// Nothing, or a record of why not.
    pub fn refuse(code: i32) -> Result<(), Refusal> {
        if code == 0 {
            return Ok(());
        }
        Err(Refusal {
            code,
            reason: String::from("not today ☃"),
        })
    }

    /// The level, or the trouble it names.
    pub fn trouble(level: i32) -> Result<i32, Trouble> {
        match level {
            1 => Err(Trouble::Minor),
            5 => Err(Trouble::Major),
            other => Ok(other),
        }
    }

    /// A record later, or the error of the work that would have made it.
    pub async fn checked_later(count: u32) -> Result<Point, CountError> {
        let _ = square_later(1, millis(1)).await;
        let count = Self::checked(count)?;
        Ok(Point {
            x: f64::from(count),
            y: -f64::from(count),
        })
    }

    /// An enum a future carries.
    pub async fn mode_later(mode: Mode) -> Mode {
        let _ = square_later(1, millis(1)).await;
        crate::Shapes::louder(mode)
    }

    /// A narrow number a future carries.
    pub async fn small_later(value: i32) -> u8 {
        value as u8
    }

    /// A list of records a future carries.
    pub async fn path_later(count: i32) -> Vec<Point> {
        (0..count)
            .map(|step| Point {
                x: f64::from(step),
                y: 0.0,
            })
            .collect()
    }

    /// A number that may be missing, later.
    pub async fn maybe_later(value: i32) -> Option<i32> {
        (value >= 0).then_some(value)
    }

    /// The first `count` of `history`, one after another.
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

    /// A mode that goes from quiet to off.
    pub fn mode_now() -> Signal<Mode> {
        let (source, mode) = signal(Mode::Quiet);
        let mut next = [Mode::Loud, Mode::Off].into_iter();
        drop(stepped(move || match next.next() {
            Some(mode) if source.set(mode) => Poll::Pending,
            _ => Poll::Ready(Ok::<_, Failure>(())),
        }));
        mode
    }

    /// A place that moves right until it is at `limit`.
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

    /// What `f` makes of each point: a record lent to the function, and an
    /// enum given back.
    pub fn each_point<E>(
        points: &[Point],
        mut f: impl FnMut(&Point) -> Result<Mode, E>,
    ) -> Result<Vec<Mode>, E> {
        points.iter().map(&mut f).collect()
    }

    /// The event `f` makes of `event`: one handed over, and one given back.
    pub fn reshaped<E>(
        event: Event,
        f: impl FnOnce(Event) -> Result<Event, E>,
    ) -> Result<Event, E> {
        f(event)
    }

    /// The point `f` picks, if it picks one: a list lent as a slice, beside
    /// an enum.
    pub fn picked<E>(
        points: Vec<Point>,
        f: impl Fn(&[Point], Mode) -> Result<Option<Point>, E>,
    ) -> Result<Option<Point>, E> {
        f(&points, Mode::Loud)
    }

    /// The bytes `f` gives for 1, then for 2.
    pub fn stamped<E>(f: impl Fn(i32) -> Result<Vec<u8>, E>) -> Result<Vec<u8>, E> {
        let mut out = f(1)?;
        out.extend(f(2)?);
        Ok(out)
    }

    /// What `f` makes of quiet: an enum each way and nothing else.
    pub fn graded<E>(f: impl Fn(Mode) -> Result<Mode, E>) -> Result<Mode, E> {
        f(Mode::Quiet)
    }
}
