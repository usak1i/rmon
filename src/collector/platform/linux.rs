use std::fs;
use std::path::Path;

use crate::state::SensorReading;

pub fn read_sensors() -> Vec<SensorReading> {
    let mut out = Vec::new();
    out.extend(read_hwmon());
    out.extend(read_battery());
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

/// Read `/sys/class/power_supply/BAT*/{capacity,status}`.
fn read_battery() -> Vec<SensorReading> {
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
        let Some(capacity) = read_trim(&path.join("capacity")).and_then(|s| s.parse::<f64>().ok())
        else {
            continue;
        };
        out.push(SensorReading {
            category: "battery".to_string(),
            name: name.clone(),
            value: capacity,
            unit: "%",
        });
    }
    out
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
