//! A value the library keeps current, which Haxe follows: a signal.

use hxn_future::{Signal, Source, signal};

/// A thermostat, and the temperature it is set to.
#[hxn::interface]
pub struct Thermostat {
    degrees: f64,
    source: Source<f64>,
    // Handed to the one reader that asks for it.
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

    /// The temperature it is set to, as a value that changes. Haxe reads
    /// the latest, and whatever was computed from it is computed again.
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
