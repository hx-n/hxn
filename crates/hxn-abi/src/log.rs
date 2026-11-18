//! Bounded diagnostics pulled by an adapter, without retaining a host callback.

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::fmt::{self, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The maximum verbosity to emit. `Off` disables every record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum Level {
    Off = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Off,
            1 => Self::Error,
            2 => Self::Warn,
            3 => Self::Info,
            4 => Self::Debug,
            5 => Self::Trace,
            _ => return None,
        })
    }
}

impl fmt::Display for Level {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Self::Off => "OFF",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        })
    }
}

/// An owned diagnostic. Text is UTF-8, and time is milliseconds since Unix epoch.
#[derive(Debug)]
pub struct Record {
    pub level: Level,
    pub target: String,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub timestamp: f64,
    pub truncated: bool,
}

const CAPACITY: usize = 256;
const MESSAGE_BYTES: usize = 8192;
const TARGET_BYTES: usize = 256;
const FILE_BYTES: usize = 1024;

struct Text {
    value: String,
    limit: usize,
    truncated: bool,
}

impl Text {
    fn new(limit: usize) -> Self {
        Self {
            value: String::new(),
            limit,
            truncated: false,
        }
    }
}

impl Write for Text {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let mut end = value.len().min(self.limit - self.value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.value.push_str(&value[..end]);
        self.truncated |= end < value.len();
        if self.truncated {
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct Queue {
    records: VecDeque<Record>,
    dropped: u64,
}

struct Logger {
    level: AtomicU8,
    captured: AtomicBool,
    queue: Mutex<Queue>,
}

impl Logger {
    const fn new() -> Self {
        Self {
            level: AtomicU8::new(Level::Info as u8),
            captured: AtomicBool::new(false),
            queue: Mutex::new(Queue {
                records: VecDeque::new(),
                dropped: 0,
            }),
        }
    }

    fn enabled(&self, level: Level) -> bool {
        level != Level::Off && level as u8 <= self.level.load(Ordering::Relaxed)
    }

    fn write(
        &self,
        level: Level,
        target: &str,
        message: fmt::Arguments<'_>,
        file: &str,
        line: u32,
    ) {
        if !self.enabled(level) {
            return;
        }
        let mut text = Text::new(MESSAGE_BYTES);
        let _ = text.write_fmt(message);
        let mut scope = Text::new(TARGET_BYTES);
        let _ = scope.write_str(target);
        let mut source = Text::new(FILE_BYTES);
        let _ = source.write_str(file);
        let record = Record {
            level,
            target: scope.value,
            message: text.value,
            file: source.value,
            line,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64()
                * 1000.0,
            truncated: text.truncated || scope.truncated || source.truncated,
        };
        if self.captured.load(Ordering::Acquire) {
            let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            if queue.records.len() == CAPACITY {
                queue.dropped = queue.dropped.saturating_add(1);
            } else {
                queue.records.push_back(record);
            }
        } else {
            use std::io::Write;
            // A closed output stream must not turn a diagnostic into a library failure.
            let _ = writeln!(
                std::io::stderr().lock(),
                "[{}] {}: {}",
                record.level,
                record.target,
                record.message
            );
        }
    }

    fn take(&self) -> Queue {
        std::mem::take(&mut *self.queue.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

static LOGGER: Logger = Logger::new();

/// Changes the verbosity of this linked Rust logger. Haxe propagates its level here.
pub fn set_level(level: Level) {
    LOGGER.level.store(level as u8, Ordering::Relaxed);
}

/// Tests the filter without formatting a message.
pub fn enabled(level: Level) -> bool {
    LOGGER.enabled(level)
}

/// Emits a record. Prefer the level macros, which skip disabled arguments too.
pub fn write(level: Level, target: &str, message: fmt::Arguments<'_>, file: &str, line: u32) {
    LOGGER.write(level, target, message, file, line);
}

/// Enables pull delivery instead of the standalone stderr output. Used by adapters.
#[doc(hidden)]
pub fn capture(level: i32) {
    if let Ok(value) = u8::try_from(level)
        && let Some(level) = Level::from_u8(value)
    {
        set_level(level);
        LOGGER.captured.store(true, Ordering::Release);
    }
}

/// Takes one bounded batch. No runtime is called while the queue is locked.
/// Empty bytes mean no records/drops; otherwise: dropped f64, count i32, then
/// level i32, timestamp f64, target/message/file text, line i32, truncated bool.
#[doc(hidden)]
pub fn take_wire() -> Vec<u8> {
    let batch = LOGGER.take();
    if batch.records.is_empty() && batch.dropped == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    crate::wire::f64(&mut out, batch.dropped as f64);
    crate::wire::i32(&mut out, batch.records.len() as i32);
    for record in batch.records {
        crate::wire::i32(&mut out, record.level as i32);
        crate::wire::f64(&mut out, record.timestamp);
        // All fields have small fixed bounds, so lengths fit the wire format.
        let _ = crate::wire::text(&mut out, &record.target);
        let _ = crate::wire::text(&mut out, &record.message);
        let _ = crate::wire::text(&mut out, &record.file);
        crate::wire::i32(&mut out, record.line as i32);
        crate::wire::bool(&mut out, record.truncated);
    }
    out
}

/// Emits a diagnostic with source metadata, skipping disabled format arguments.
#[macro_export]
macro_rules! hxn_log {
    ($level:expr, target: $target:expr, $($args:tt)+) => {{
        let level = $level;
        if $crate::log::enabled(level) {
            $crate::log::write(level, $target, format_args!($($args)+), file!(), line!());
        }
    }};
    ($level:expr, $($args:tt)+) => { $crate::hxn_log!($level, target: module_path!(), $($args)+) };
}

#[macro_export]
macro_rules! hxn_error { ($($args:tt)+) => { $crate::hxn_log!($crate::log::Level::Error, $($args)+) }; }
#[macro_export]
macro_rules! hxn_warn { ($($args:tt)+) => { $crate::hxn_log!($crate::log::Level::Warn, $($args)+) }; }
#[macro_export]
macro_rules! hxn_info { ($($args:tt)+) => { $crate::hxn_log!($crate::log::Level::Info, $($args)+) }; }
#[macro_export]
macro_rules! hxn_debug { ($($args:tt)+) => { $crate::hxn_log!($crate::log::Level::Debug, $($args)+) }; }
#[macro_export]
macro_rules! hxn_trace { ($($args:tt)+) => { $crate::hxn_log!($crate::log::Level::Trace, $($args)+) }; }

pub use crate::{
    hxn_debug as debug, hxn_error as error, hxn_info as info, hxn_log as log, hxn_trace as trace,
    hxn_warn as warn,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_before_display_and_truncates_at_utf8_boundaries() {
        struct Never;
        impl fmt::Display for Never {
            fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
                panic!("disabled");
            }
        }
        let logger = Logger::new();
        logger.captured.store(true, Ordering::Release);
        logger.write(Level::Debug, "test", format_args!("{Never}"), "source", 1);
        logger.write(
            Level::Info,
            "test",
            format_args!("{}", "☃".repeat(4000)),
            "source",
            42,
        );
        let batch = logger.take();
        assert_eq!(batch.records.len(), 1);
        let record = &batch.records[0];
        assert!(record.truncated);
        assert!(record.message.len() <= MESSAGE_BYTES);
        assert_eq!(record.line, 42);
        assert!(record.message.chars().all(|c| c == '☃'));
        assert!(!logger.enabled(Level::Off));
    }

    #[test]
    fn concurrent_writers_keep_a_bounded_ordered_batch_and_account_for_drops() {
        let logger = std::sync::Arc::new(Logger::new());
        logger.captured.store(true, Ordering::Release);
        let workers: Vec<_> = (0..4)
            .map(|worker| {
                let logger = logger.clone();
                std::thread::spawn(move || {
                    for n in 0..100 {
                        logger.write(
                            Level::Info,
                            "test",
                            format_args!("{worker}:{n}"),
                            "source",
                            n,
                        );
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let batch = logger.take();
        assert_eq!(batch.records.len(), CAPACITY);
        assert_eq!(batch.dropped, 400 - CAPACITY as u64);
        let mut last = [None; 4];
        for record in batch.records {
            let (worker, n) = record.message.split_once(':').unwrap();
            let worker: usize = worker.parse().unwrap();
            let n: u32 = n.parse().unwrap();
            assert!(last[worker].is_none_or(|before| before < n));
            last[worker] = Some(n);
        }
        assert!(logger.take().records.is_empty());
    }
}
