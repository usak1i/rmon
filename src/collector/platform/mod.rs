//! Platform-specific sensor reads.
//!
//! Each module exposes `read_sensors()` and `read_batteries()`, hiding all
//! `/sys` walking, IOKit calls, or subprocess spawning behind two entry
//! points. Callers must not assume which categories are present — Linux
//! returns `temp` / `fan`, macOS returns nothing for thermal/fan in
//! Phase 2.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{read_batteries, read_sensors};

#[cfg(target_os = "macos")]
mod ioreport;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use ioreport::IoReportSampler;
#[cfg(target_os = "macos")]
pub use macos::{read_batteries, read_sensors};

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn read_sensors() -> Vec<crate::state::SensorReading> {
    Vec::new()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn read_batteries() -> Vec<crate::state::BatteryReading> {
    Vec::new()
}
