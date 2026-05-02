//! Platform-specific sensor reads.
//!
//! Each module exposes `read_sensors() -> Vec<SensorReading>`, hiding all
//! `/sys` walking, IOKit calls, or subprocess spawning behind one entry
//! point. Callers must not assume which categories are present — Linux
//! returns `temp` / `fan` / `battery`, macOS returns `battery` only for
//! Phase 2 (thermal/fan deferred to Phase 2.5).

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::read_sensors;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::read_sensors;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn read_sensors() -> Vec<crate::state::SensorReading> {
    Vec::new()
}
