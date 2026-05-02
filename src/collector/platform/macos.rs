use std::process::Command;

use crate::state::{BatteryReading, BatteryStatus, SensorReading};

/// macOS Phase 2 sensor coverage: nothing for thermal/fan yet
/// (Apple Silicon needs the private IOReport framework — see TODO Phase 2.5
/// carryovers).
pub fn read_sensors() -> Vec<SensorReading> {
    Vec::new()
}

pub fn read_batteries() -> Vec<BatteryReading> {
    let Ok(output) = Command::new("pmset").args(["-g", "batt"]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_pmset(&stdout).into_iter().collect()
}

/// Extract one battery reading from `pmset -g batt` output, e.g.:
///     -InternalBattery-0 (id=...)\t82%; discharging; 5:23 remaining present: true
/// Returns None when no recognisable battery line is present.
fn parse_pmset(s: &str) -> Option<BatteryReading> {
    for line in s.lines() {
        let trimmed = line.trim();
        // Battery line begins with `-` and contains a `%` — skips the
        // "Now drawing from 'AC Power'" header.
        if !trimmed.starts_with('-') || !trimmed.contains('%') {
            continue;
        }
        let percent = parse_percent(trimmed)?;
        let status = parse_status(trimmed);
        let time_remaining_minutes = parse_time(trimmed, status);
        // Identify the battery: take "InternalBattery-0" out of "-InternalBattery-0 (id=...)".
        let name = trimmed
            .strip_prefix('-')
            .and_then(|s| s.split_whitespace().next())
            .unwrap_or("battery")
            .to_string();
        return Some(BatteryReading {
            name,
            percent,
            status,
            time_remaining_minutes,
        });
    }
    None
}

fn parse_percent(line: &str) -> Option<f64> {
    let (before, _) = line.split_once('%')?;
    let last = before.split_whitespace().last()?;
    last.parse::<f64>().ok()
}

fn parse_status(line: &str) -> BatteryStatus {
    let lower = line.to_ascii_lowercase();
    // Order matters — `discharging` contains `charging` as a substring,
    // so the more specific match has to come first.
    if lower.contains("charged") || lower.contains("finishing charge") {
        BatteryStatus::Full
    } else if lower.contains("discharging") {
        BatteryStatus::Discharging
    } else if lower.contains("charging") {
        BatteryStatus::Charging
    } else {
        BatteryStatus::Unknown
    }
}

fn parse_time(line: &str, status: BatteryStatus) -> Option<u32> {
    if !matches!(status, BatteryStatus::Discharging | BatteryStatus::Charging) {
        return None;
    }
    // Find the first H:MM substring; pmset emits "5:23 remaining" or
    // "(no estimate)" when unknown.
    for tok in line.split_whitespace() {
        let Some((h, m)) = tok.split_once(':') else {
            continue;
        };
        let h: u32 = h.parse().ok()?;
        let m: u32 = m.parse().ok()?;
        let total = h * 60 + m;
        if total == 0 {
            return None;
        }
        return Some(total);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_discharging_with_time() {
        let s = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=4194611)\t82%; discharging; 5:23 remaining present: true\n";
        let b = parse_pmset(s).unwrap();
        assert_eq!(b.name, "InternalBattery-0");
        assert_eq!(b.percent, 82.0);
        assert_eq!(b.status, BatteryStatus::Discharging);
        assert_eq!(b.time_remaining_minutes, Some(5 * 60 + 23));
    }

    #[test]
    fn parses_charging() {
        let s = " -InternalBattery-0 (id=1)\t75%; charging; 1:32 remaining present: true";
        let b = parse_pmset(s).unwrap();
        assert_eq!(b.percent, 75.0);
        assert_eq!(b.status, BatteryStatus::Charging);
        assert_eq!(b.time_remaining_minutes, Some(92));
    }

    #[test]
    fn parses_charged_clamps_time_to_none() {
        let s = " -InternalBattery-0 (id=1)\t100%; charged; 0:00 remaining present: true";
        let b = parse_pmset(s).unwrap();
        assert_eq!(b.status, BatteryStatus::Full);
        assert_eq!(b.time_remaining_minutes, None);
    }

    #[test]
    fn parses_no_estimate() {
        let s = " -InternalBattery-0 (id=1)\t82%; discharging; (no estimate) present: true";
        let b = parse_pmset(s).unwrap();
        assert_eq!(b.status, BatteryStatus::Discharging);
        assert_eq!(b.time_remaining_minutes, None);
    }

    #[test]
    fn returns_none_when_no_battery_line() {
        assert!(parse_pmset("Now drawing from 'AC Power'").is_none());
    }
}
