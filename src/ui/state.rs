use ratatui::widgets::TableState;

/// Which panel currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub gpu_enabled: bool,
    /// PIDs of the rows currently visible in the process table, in display
    /// order. Refreshed each frame by the process widget so key handlers can
    /// translate `selected()` → PID without re-sorting.
    pub last_visible_pids: Vec<u32>,
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
            gpu_enabled,
            last_visible_pids: Vec::new(),
        }
    }

    pub fn selected_pid(&self) -> Option<u32> {
        self.process_table
            .selected()
            .and_then(|i| self.last_visible_pids.get(i).copied())
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new(false)
    }
}
