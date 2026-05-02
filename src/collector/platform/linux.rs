use std::fs;
use std::path::Path;

use crate::state::{BatteryReading, BatteryStatus, SensorReading};

pub fn read_sensors() -> Vec<SensorReading> {
    read_hwmon()
}

pub fn read_batteries() -> Vec<BatteryReading> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("BAT") {
            continue;
        }
        let Some(percent) = read_trim(&path.join("capacity")).and_then(|s| s.parse::<f64>().ok())
        else {
            continue;
        };
        let status = parse_status(read_trim(&path.join("status")).as_deref());
        let time_remaining_minutes = read_time_remaining(&path, status);
        out.push(BatteryReading {
            name,
            percent,
            status,
            time_remaining_minutes,
        });
    }
    out
}

/// Walk `/sys/class/hwmon/hwmon*` for `temp*_input` (mC) and `fan*_input` (RPM).
fn read_hwmon() -> Vec<SensorReading> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return out;
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        let chip_name = read_trim(&dir.join("name")).unwrap_or_else(|| "hwmon".to_string());

        for kind in [SensorKind::Temp, SensorKind::Fan] {
            let mut idx = 1;
            loop {
                let input = dir.join(format!("{}{idx}_input", kind.prefix()));
                if !input.exists() {
                    break;
                }
                let label = read_trim(&dir.join(format!("{}{idx}_label", kind.prefix())));
                let raw = match read_trim(&input).and_then(|s| s.parse::<f64>().ok()) {
                    Some(v) => v,
                    None => {
                        idx += 1;
                        continue;
                    }
                };

                let (value, unit) = match kind {
                    SensorKind::Temp => (raw / 1000.0, "°C"),
                    SensorKind::Fan => (raw, "rpm"),
                };
                let name = match label {
                    Some(l) => format!("{chip_name}:{l}"),
                    None => format!("{chip_name}:{}{idx}", kind.prefix()),
                };

                out.push(SensorReading {
                    category: kind.category().to_string(),
                    name,
                    value,
                    unit,
                });
                idx += 1;
            }
        }
    }
    out
}

fn parse_status(raw: Option<&str>) -> BatteryStatus {
    match raw.map(str::trim) {
        Some("Charging") => BatteryStatus::Charging,
        Some("Discharging") => BatteryStatus::Discharging,
        Some("Full") => BatteryStatus::Full,
        Some("Not charging") => BatteryStatus::Full,
        _ => BatteryStatus::Unknown,
    }
}

/// `time_to_empty_now` / `time_to_full_now` are in seconds when present.
/// Many drivers omit them; this returns None in that case.
fn read_time_remaining(path: &Path, status: BatteryStatus) -> Option<u32> {
    let file = match status {
        BatteryStatus::Discharging => "time_to_empty_now",
        BatteryStatus::Charging => "time_to_full_now",
        _ => return None,
    };
    let secs: u64 = read_trim(&path.join(file))?.parse().ok()?;
    if secs == 0 {
        return None;
    }
    Some((secs / 60) as u32)
}

#[derive(Copy, Clone)]
enum SensorKind {
    Temp,
    Fan,
}

impl SensorKind {
    fn prefix(self) -> &'static str {
        match self {
            SensorKind::Temp => "temp",
            SensorKind::Fan => "fan",
        }
    }
    fn category(self) -> &'static str {
        match self {
            SensorKind::Temp => "temp",
            SensorKind::Fan => "fan",
        }
    }
}

fn read_trim(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_known_values() {
        assert_eq!(parse_status(Some("Charging")), BatteryStatus::Charging);
        assert_eq!(
            parse_status(Some("Discharging")),
            BatteryStatus::Discharging
        );
        assert_eq!(parse_status(Some("Full")), BatteryStatus::Full);
        assert_eq!(parse_status(Some("Not charging")), BatteryStatus::Full);
        assert_eq!(parse_status(Some("garbage")), BatteryStatus::Unknown);
        assert_eq!(parse_status(None), BatteryStatus::Unknown);
    }
}
