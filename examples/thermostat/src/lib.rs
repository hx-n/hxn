//! A signal: a value the library keeps up to date and Haxe subscribes to.

use hxn_future::{Signal, Source, signal};

/// A thermostat and its target temperature.
#[hxn::interface]
pub struct Thermostat {
    degrees: f64,
    source: Source<f64>,
    // Taken by the first call to `target`, its only reader.
    signal: Option<Signal<f64>>,
}

#[hxn::methods]
impl Thermostat {
    pub fn new(degrees: f64) -> Self {
        let (source, signal) = signal(degrees);
        Self {
            degrees,
            source,
            signal: Some(signal),
        }
    }

    /// The target temperature as a signal. Haxe reads the latest value, and
    /// values computed from it are recomputed when it changes.
    pub fn target(&mut self) -> Result<Signal<f64>, &'static str> {
        self.signal.take().ok_or("the target is already followed")
    }

    /// Turns the dial by `degrees`, and returns the new setting.
    pub fn turn(&mut self, degrees: f64) -> f64 {
        self.degrees += degrees;
        self.source.set(self.degrees);
        self.degrees
    }
}
