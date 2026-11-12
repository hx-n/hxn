//! Plain data types, which are passed across the boundary by value. They
//! are defined in a separate file.

use crate::Counter;

/// A loudness level. Two discriminants are explicit and the third is
/// implicit.
#[hxn::data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Quiet = 3,
    /// The loudest level.
    Loud = 9,
    Off,
}

/// A point.
#[hxn::data]
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A struct with one field of each supported type.
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
    /// A nested struct.
    pub origin: Point,
    /// Optional.
    pub note: Option<String>,
    pub limit: Option<i32>,
    pub fallback: Option<Mode>,
    pub path: Vec<Point>,
    pub tags: Vec<String>,
    pub payload: Vec<u8>,
    pub grid: Vec<Vec<i32>>,
}

/// Static functions over data types, with no instance.
#[hxn::interface]
pub struct Shapes;

#[hxn::methods]
impl Shapes {
    /// A string showing the struct as the library received it. Takes the
    /// struct by reference.
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

    /// Takes a struct by value and returns it modified.
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

    /// The mode with discriminant `number`, if there is one.
    pub fn mode_of(number: i32) -> Option<Mode> {
        [Mode::Quiet, Mode::Loud, Mode::Off]
            .into_iter()
            .find(|mode| *mode as i32 == number)
    }

    /// The midpoint of two points, one taken by value and one by reference.
    pub fn midpoint(a: Point, b: &Point) -> Point {
        Point {
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
        }
    }

    /// The first point to the right of `x`, from a slice parameter.
    pub fn right_of(points: &[Point], x: f64) -> Option<Point> {
        points.iter().find(|point| point.x > x).cloned()
    }

    /// The sum of a list of numbers.
    pub fn total(values: Vec<i32>) -> i32 {
        values.iter().sum()
    }

    /// Doubles an optional number, if it is present.
    pub fn doubled(value: Option<i32>) -> Option<i32> {
        value.map(|value| value * 2)
    }

    /// The words of `text`.
    pub fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    /// Returns a struct, or an error.
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

    /// A counter starting at the struct's `count`. Takes data and returns an
    /// object.
    pub fn counter(settings: &Settings) -> Counter {
        Counter::new(settings.count)
    }
}
