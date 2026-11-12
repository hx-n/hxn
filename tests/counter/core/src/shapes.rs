//! Plain data: values that cross whole, by copy, kept in a file of their
//! own.

use crate::Counter;

/// How loud. Two of its numbers are written and one follows on.
#[hxn::data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Quiet = 3,
    /// As loud as it goes.
    Loud = 9,
    Off,
}

/// A place.
#[hxn::data]
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// One of each thing a record holds.
#[hxn::data]
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub title: String,
    pub count: i32,
    pub wide: i64,
    pub ratio: f32,
    pub small: u8,
    pub flags: u32,
    pub enabled: bool,
    pub mode: Mode,
    /// Another record, inside this one.
    pub origin: Point,
    /// May be left out.
    pub note: Option<String>,
    pub limit: Option<i32>,
    pub fallback: Option<Mode>,
    pub path: Vec<Point>,
    pub tags: Vec<String>,
    pub payload: Vec<u8>,
    pub grid: Vec<Vec<i32>>,
}

/// Functions over data, with no value behind them.
#[hxn::interface]
pub struct Shapes;

#[hxn::methods]
impl Shapes {
    /// The record as the library read it, taken by reference.
    pub fn describe(settings: &Settings) -> String {
        format!(
            "{} x{} wide={} ratio={} small={} flags={} {} {:?} at ({}, {}) note={:?} limit={:?} fallback={:?} path={} tags={} payload={:?} grid={:?}",
            settings.title,
            settings.count,
            settings.wide,
            settings.ratio,
            settings.small,
            settings.flags,
            if settings.enabled { "on" } else { "off" },
            settings.mode,
            settings.origin.x,
            settings.origin.y,
            settings.note,
            settings.limit,
            settings.fallback,
            settings.path.len(),
            settings.tags.join("+"),
            settings.payload,
            settings.grid,
        )
    }

    /// A record taken by value and given back changed.
    pub fn bumped(mut settings: Settings) -> Settings {
        settings.count += 1;
        settings.wide += 1 << 40;
        settings.title.push('!');
        settings.mode = Mode::Loud;
        settings.note = settings.note.map(|note| note.to_uppercase());
        settings.limit = None;
        settings.path.reverse();
        settings.tags.push(String::from("bumped"));
        settings
    }

    /// The mode one step louder.
    pub fn louder(mode: Mode) -> Mode {
        match mode {
            Mode::Off => Mode::Quiet,
            Mode::Quiet | Mode::Loud => Mode::Loud,
        }
    }

    /// An enum taken by reference.
    pub fn named(mode: &Mode) -> String {
        format!("{mode:?}")
    }

    /// The mode a number names, if it names one.
    pub fn mode_of(number: i32) -> Option<Mode> {
        [Mode::Quiet, Mode::Loud, Mode::Off]
            .into_iter()
            .find(|mode| *mode as i32 == number)
    }

    /// Halfway between two points, one taken each way.
    pub fn midpoint(a: Point, b: &Point) -> Point {
        Point {
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
        }
    }

    /// The first point right of `x`, taken from a slice.
    pub fn right_of(points: &[Point], x: f64) -> Option<Point> {
        points.iter().find(|point| point.x > x).cloned()
    }

    /// A list of numbers, added up.
    pub fn total(values: Vec<i32>) -> i32 {
        values.iter().sum()
    }

    /// A number that may be missing, doubled if it is there.
    pub fn doubled(value: Option<i32>) -> Option<i32> {
        value.map(|value| value * 2)
    }

    /// The words of `text`.
    pub fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    /// A record, or a failure.
    pub fn preset(mode: Mode) -> Result<Settings, &'static str> {
        if mode == Mode::Off {
            return Err("nothing is preset for off");
        }
        Ok(Settings {
            title: String::from("preset"),
            count: 1,
            wide: -1,
            ratio: 0.5,
            small: 255,
            flags: u32::MAX,
            enabled: true,
            mode,
            origin: Point { x: 1.0, y: -1.0 },
            note: None,
            limit: Some(7),
            fallback: Some(Mode::Quiet),
            path: Vec::new(),
            tags: vec![String::from("a"), String::from("naïve ☃")],
            payload: vec![0, 255],
            grid: vec![vec![1, 2], vec![]],
        })
    }

    /// A counter starting at what a record says: data in, an object out.
    pub fn counter(settings: &Settings) -> Counter {
        Counter::new(settings.count)
    }
}
