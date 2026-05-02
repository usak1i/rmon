use ratatui::widgets::TableState;

/// Which panel currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Cpu,
    Memory,
    Gpu,
    Network,
    Sensors,
    Disk,
    Container,
    Process,
}

/// Map a `Snapshot::numeric` key prefix to the panel that surfaces it.
/// Used by alert highlighting so a firing rule against `cpu.total` tints
/// the CPU panel border. Returns `None` for keys that aren't tied to a
/// specific panel (e.g. derived aggregates).
pub fn panel_for_metric(key: &str) -> Option<Panel> {
    if key.starts_with("cpu.") {
        Some(Panel::Cpu)
    } else if key.starts_with("mem.") {
        Some(Panel::Memory)
    } else if key.starts_with("gpu.") {
        Some(Panel::Gpu)
    } else if key.starts_with("net.") {
        Some(Panel::Network)
    } else if key.starts_with("sensor.") || key.starts_with("battery.") {
        Some(Panel::Sensors)
    } else if key.starts_with("disk.") {
        Some(Panel::Disk)
    } else if key.starts_with("container.") {
        Some(Panel::Container)
    } else if key.starts_with("process.") {
        Some(Panel::Process)
    } else {
        None
    }
}

impl Panel {
    /// Cycle to the next panel; skip `Gpu` when GPU monitoring is disabled.
    pub fn next(self, gpu_enabled: bool) -> Self {
        let raw = match self {
            Panel::Cpu => Panel::Memory,
            Panel::Memory => Panel::Gpu,
            Panel::Gpu => Panel::Network,
            Panel::Network => Panel::Sensors,
            Panel::Sensors => Panel::Disk,
            Panel::Disk => Panel::Container,
            Panel::Container => Panel::Process,
            Panel::Process => Panel::Cpu,
        };
        if !gpu_enabled && raw == Panel::Gpu {
            Panel::Network
        } else {
            raw
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSort {
    Cpu,
    Memory,
    Pid,
    Name,
}

#[derive(Debug)]
pub struct UiState {
    pub focus: Panel,
    pub process_table: TableState,
    pub process_sort: ProcessSort,
    pub search: Option<String>,
    pub editing_search: bool,
    pub kill_pending: Option<u32>,
    pub show_help: bool,
    pub show_alerts: bool,
    pub gpu_enabled: bool,
    /// When true, the Process panel groups rows by container (header row +
    /// indented children, with a final `system` bucket for unattributed PIDs).
    pub grouped_mode: bool,
    /// PIDs of the rows currently visible in the process table, in display
    /// order. `None` for non-process rows (container/system headers in
    /// grouped mode). Refreshed each frame by the process widget so key
    /// handlers can translate `selected()` → PID without re-sorting.
    pub last_visible_pids: Vec<Option<u32>>,
}

impl UiState {
    pub fn new(gpu_enabled: bool) -> Self {
        let mut table = TableState::default();
        table.select(Some(0));
        Self {
            focus: Panel::Process,
            process_table: table,
            process_sort: ProcessSort::Cpu,
            search: None,
            editing_search: false,
            kill_pending: None,
            show_help: false,
            show_alerts: false,
            gpu_enabled,
            grouped_mode: false,
            last_visible_pids: Vec::new(),
        }
    }

    pub fn selected_pid(&self) -> Option<u32> {
        self.process_table
            .selected()
            .and_then(|i| self.last_visible_pids.get(i).copied().flatten())
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new(false)
    }
}
