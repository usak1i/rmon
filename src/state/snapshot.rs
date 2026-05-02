use std::collections::HashMap;

/// Identifies a single time-series metric. String-based for flexibility
/// (per-core CPU, per-disk IO, per-interface network throughput all need
/// dynamic indexing).
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct MetricKey(pub String);

impl MetricKey {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// Point-in-time process info captured each tick. Not pushed into history —
/// the process list is too dynamic for series storage; the table renders the
/// latest snapshot directly.
#[derive(Debug, Clone)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub user: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub command: String,
    pub status: char,
    pub run_time_secs: u64,
    /// Long (64-hex) container ID from `/proc/<pid>/cgroup`, or `None` for
    /// processes not in a container. Linux-only; other platforms always
    /// return `None`. Match against `ContainerSnapshot::id` (12-char short
    /// ID) via `starts_with`.
    pub container_id: Option<String>,
}

/// Point-in-time mount/volume entry. IO throughput per disk is deferred to
/// Phase 2 (sysinfo doesn't expose system-wide block-device IO).
#[derive(Debug, Clone)]
pub struct DiskSnapshot {
    pub mount_point: String,
    pub fs_type: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

/// Per-interface network reading. `rx_bps` / `tx_bps` are computed by the
/// network collector using `SystemSource::last_refresh_elapsed`.
#[derive(Debug, Clone)]
pub struct NetworkSnapshot {
    pub interface: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
}

/// Generic key-value sensor reading. `category` groups readings in the UI
/// (`temp`, `fan`); `name` is the human label; `unit` is appended for
/// display. Numeric value is also pushed into the `Snapshot::numeric` map
/// under `sensor.<category>.<name>` so it can drive sparklines later.
///
/// Battery is *not* a category here — it gets its own richer
/// `BatteryReading` since percentage alone is too coarse.
#[derive(Debug, Clone)]
pub struct SensorReading {
    pub category: String,
    pub name: String,
    pub value: f64,
    pub unit: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryStatus {
    Charging,
    Discharging,
    Full,
    Unknown,
}

impl BatteryStatus {
    pub fn label(self) -> &'static str {
        match self {
            BatteryStatus::Charging => "charging",
            BatteryStatus::Discharging => "discharging",
            BatteryStatus::Full => "full",
            BatteryStatus::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BatteryReading {
    pub name: String,
    pub percent: f64,
    pub status: BatteryStatus,
    /// Estimated minutes until empty (when discharging) or full (when
    /// charging). `None` when the system can't estimate (just plugged in,
    /// already full, etc.).
    pub time_remaining_minutes: Option<u32>,
}

/// Per-container snapshot from `docker stats`. Names map to docker's
/// `stats --format` keys; we copy them into typed numbers so the widget
/// doesn't have to re-parse strings each frame. `id` is the 12-char
/// short ID from docker, which is a prefix of the long ID we'd find in
/// `/proc/<pid>/cgroup` (so process→container matching is `starts_with`).
#[derive(Debug, Clone)]
pub struct ContainerSnapshot {
    pub id: String,
    pub name: String,
    pub cpu_percent: f64,
    pub mem_bytes: u64,
    pub mem_percent: f64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
}

/// One sampling round's worth of data, accumulated by the registry.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// Numeric metrics keyed by name. Convention: `<group>.<sub>` e.g.
    /// `cpu.total`, `cpu.core.0`, `mem.used_bytes`.
    pub numeric: HashMap<MetricKey, f64>,
    pub processes: Vec<ProcessSnapshot>,
    pub disks: Vec<DiskSnapshot>,
    pub networks: Vec<NetworkSnapshot>,
    pub sensors: Vec<SensorReading>,
    pub batteries: Vec<BatteryReading>,
    pub containers: Vec<ContainerSnapshot>,
}

impl Snapshot {
    pub fn new() -> Self {
        Self {
            numeric: HashMap::new(),
            processes: Vec::new(),
            disks: Vec::new(),
            networks: Vec::new(),
            sensors: Vec::new(),
            batteries: Vec::new(),
            containers: Vec::new(),
        }
    }

    pub fn set(&mut self, key: impl Into<String>, value: f64) {
        self.numeric.insert(MetricKey::new(key), value);
    }

    pub fn get(&self, key: &str) -> Option<f64> {
        self.numeric.get(&MetricKey::new(key)).copied()
    }
}
