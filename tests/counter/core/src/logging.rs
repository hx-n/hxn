//! Diagnostics crossing the generated binding, including errors and worker work.

use crate::CountError;
use hxn_future::{Future, spawn};

#[hxn::function(class = "Diagnostics")]
pub fn emit_logs(count: i32) -> i32 {
    hxn::log::debug!(target: "fixture", "debug record");
    hxn::log::trace!(target: "fixture", "trace record");
    for n in 0..count.clamp(0, 300) {
        hxn::log::info!(target: "fixture", "native naïve ☃ {n}");
    }
    count
}

#[hxn::function(class = "Diagnostics")]
pub fn log_failure() -> Result<(), CountError> {
    hxn::log::error!(target: "fixture", "before typed error");
    Err(CountError::Unknown("logging".into()))
}

#[hxn::function(class = "Diagnostics")]
pub fn log_work() -> Future<i32> {
    spawn(async {
        hxn::log::warn!(target: "fixture", "worker record");
        Ok::<_, &'static str>(42)
    })
}

#[hxn::function(class = "Diagnostics")]
pub fn log_large() {
    hxn::log::info!(target: "fixture", "{}", "☃".repeat(4000));
}
