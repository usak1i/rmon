pub mod container;
pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod help;
pub mod memory;
pub mod network;
pub mod process;
pub mod sensors;

use std::collections::VecDeque;

/// Convert a history series of f64 in `0..=ceiling` into the `u64` slice
/// that ratatui::Sparkline consumes. Values outside the range are clamped.
pub fn series_to_u64(series: Option<&VecDeque<f64>>, ceiling: f64) -> Vec<u64> {
    let Some(s) = series else { return Vec::new() };
    let scale = if ceiling <= 0.0 { 1.0 } else { ceiling };
    s.iter()
        .map(|v| {
            let clamped = v.clamp(0.0, scale);
            (clamped / scale * 100.0) as u64
        })
        .collect()
}
