use std::process::Command;

use crate::state::SensorReading;

/// macOS Phase 2 sensor coverage: battery only via `pmset -g batt`.
/// Apple Silicon thermals/fans require IOReport private framework — deferred
/// to Phase 2.5.
pub fn read_sensors() -> Vec<SensorReading> {
    read_battery()
}

fn read_battery() -> Vec<SensorReading> {
    let Ok(output) = Command::new("pmset").args(["-g", "batt"]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_pmset(&stdout)
        .map(|pct| {
            vec![SensorReading {
                category: "battery".to_string(),
                name: "internal".to_string(),
                value: pct,
                unit: "%",
            }]
        })
        .unwrap_or_default()
}

/// Extract the first percentage figure from `pmset -g batt` output, e.g.
/// `-InternalBattery-0 (id=...)\t82%; discharging; 5:23 remaining`.
fn parse_pmset(s: &str) -> Option<f64> {
    for line in s.lines() {
        if let Some((before, _)) = line.split_once('%') {
            let pct_str = before.split_whitespace().last()?;
            if let Ok(v) = pct_str.parse::<f64>() {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pmset_finds_percent() {
        let s = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=4194611)\t82%; discharging; 5:23 remaining present: true\n";
        assert_eq!(parse_pmset(s), Some(82.0));
    }

    #[test]
    fn parse_pmset_missing_returns_none() {
        assert_eq!(parse_pmset("nothing useful here"), None);
    }
}
