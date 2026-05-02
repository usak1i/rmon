use std::io::{self, Stdout};
use std::panic;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::alert::{AlertEvaluator, AlertEventKind, AlertRule, AlertSeverity};
use crate::collector::{
    ContainerCollector, CpuCollector, DiskCollector, MemoryCollector, NetworkCollector,
    ProcessCollector, Registry, SensorsCollector, SystemSource,
};
use crate::config::Config;
use crate::state::{SharedState, Snapshot};
use crate::ui;
use crate::ui::state::{Panel, ProcessSort, UiState};
use crate::ui::theme::Theme;

type Tui = Terminal<CrosstermBackend<Stdout>>;

pub struct App {
    state: SharedState,
    config: Config,
    ui: UiState,
    theme: Theme,
    gpu_enabled: bool,
    alert_rules: Vec<AlertRule>,
}

impl App {
    pub fn new(
        state: SharedState,
        config: Config,
        gpu_enabled: bool,
        alert_rules: Vec<AlertRule>,
    ) -> Self {
        Self {
            state,
            config,
            ui: UiState::new(gpu_enabled),
            theme: Theme::dark(),
            gpu_enabled,
            alert_rules,
        }
    }

    pub fn run(&mut self) -> Result<()> {
        let shutdown = Arc::new(AtomicBool::new(false));
        install_panic_hook();

        let mut terminal = enter_terminal().context("entering terminal")?;
        let sampler_handle = spawn_sampler(
            self.state.clone(),
            self.config.clone(),
            self.gpu_enabled,
            self.alert_rules.clone(),
            shutdown.clone(),
        );

        let result = self.event_loop(&mut terminal, &shutdown);

        shutdown.store(true, Ordering::SeqCst);
        if let Err(e) = sampler_handle.join() {
            tracing::warn!(?e, "sampler thread panicked");
        }
        leave_terminal(&mut terminal)?;
        result
    }

    fn event_loop(&mut self, terminal: &mut Tui, shutdown: &AtomicBool) -> Result<()> {
        let tick = self.config.ui_tick();
        let mut next_tick = Instant::now() + tick;

        loop {
            terminal.draw(|frame| ui::render(frame, &self.state, &mut self.ui, &self.theme))?;

            let timeout = next_tick.saturating_duration_since(Instant::now());
            if event::poll(timeout)?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
                && self.handle_key(&key) == KeyOutcome::Quit
            {
                shutdown.store(true, Ordering::SeqCst);
                return Ok(());
            }

            if Instant::now() >= next_tick {
                next_tick += tick;
            }

            if shutdown.load(Ordering::SeqCst) {
                return Ok(());
            }
        }
    }

    fn handle_key(&mut self, key: &KeyEvent) -> KeyOutcome {
        // Search input mode swallows most keys.
        if self.ui.editing_search {
            match key.code {
                KeyCode::Esc => {
                    self.ui.editing_search = false;
                    self.ui.search = None;
                }
                KeyCode::Enter => {
                    self.ui.editing_search = false;
                    if self.ui.search.as_deref().is_some_and(str::is_empty) {
                        self.ui.search = None;
                    }
                }
                KeyCode::Backspace => {
                    if let Some(s) = self.ui.search.as_mut() {
                        s.pop();
                    }
                }
                KeyCode::Char(c) => {
                    self.ui.search.get_or_insert_with(String::new).push(c);
                }
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        // Kill confirmation prompt.
        if let Some(pid) = self.ui.kill_pending {
            match key.code {
                KeyCode::Enter => {
                    if let Err(e) = send_sigterm(pid) {
                        tracing::warn!(pid, %e, "kill failed");
                    } else {
                        tracing::info!(pid, "sent SIGTERM");
                    }
                    self.ui.kill_pending = None;
                }
                KeyCode::Esc => self.ui.kill_pending = None,
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), KeyModifiers::NONE) => return KeyOutcome::Quit,
            (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                return KeyOutcome::Quit;
            }
            (KeyCode::Char('?'), _) => {
                self.ui.show_help = !self.ui.show_help;
                self.ui.show_alerts = false;
            }
            (KeyCode::Char('a'), KeyModifiers::NONE) => {
                self.ui.show_alerts = !self.ui.show_alerts;
                self.ui.show_help = false;
            }
            (KeyCode::Esc, _) => {
                self.ui.show_help = false;
                self.ui.show_alerts = false;
            }
            (KeyCode::Tab, _) => self.ui.focus = self.ui.focus.next(self.gpu_enabled),
            (KeyCode::Up, _) => move_selection(&mut self.ui, -1),
            (KeyCode::Down, _) => move_selection(&mut self.ui, 1),
            (KeyCode::PageUp, _) => move_selection(&mut self.ui, -10),
            (KeyCode::PageDown, _) => move_selection(&mut self.ui, 10),
            (KeyCode::Char('c'), KeyModifiers::NONE) => self.ui.process_sort = ProcessSort::Cpu,
            (KeyCode::Char('m'), KeyModifiers::NONE) => self.ui.process_sort = ProcessSort::Memory,
            (KeyCode::Char('p'), KeyModifiers::NONE) => self.ui.process_sort = ProcessSort::Pid,
            (KeyCode::Char('n'), KeyModifiers::NONE) => self.ui.process_sort = ProcessSort::Name,
            (KeyCode::Char('/'), _) => {
                self.ui.editing_search = true;
                self.ui.search.get_or_insert_with(String::new);
            }
            (KeyCode::Char('g'), KeyModifiers::NONE) => {
                self.ui.grouped_mode = !self.ui.grouped_mode;
            }
            (KeyCode::F(9), _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                if let Some(pid) = self.ui.selected_pid() {
                    self.ui.kill_pending = Some(pid);
                }
            }
            _ => {}
        }
        KeyOutcome::Continue
    }
}

#[derive(Debug, PartialEq, Eq)]
enum KeyOutcome {
    Continue,
    Quit,
}

fn move_selection(ui: &mut UiState, delta: i32) {
    if ui.focus != Panel::Process {
        return;
    }
    let len = ui.last_visible_pids.len();
    if len == 0 {
        return;
    }
    let cur = ui.process_table.selected().unwrap_or(0) as i32;
    let next = (cur + delta).clamp(0, len as i32 - 1) as usize;
    ui.process_table.select(Some(next));
}

fn log_event(ev: &crate::alert::AlertEvent) {
    match (ev.kind, ev.severity) {
        (AlertEventKind::Fired, AlertSeverity::Critical) => tracing::error!(
            rule = %ev.rule_name,
            metric = %ev.metric,
            value = ev.value,
            threshold = ev.threshold,
            "alert FIRED (critical)"
        ),
        (AlertEventKind::Fired, AlertSeverity::Warn) => tracing::warn!(
            rule = %ev.rule_name,
            metric = %ev.metric,
            value = ev.value,
            threshold = ev.threshold,
            "alert FIRED"
        ),
        (AlertEventKind::Fired, AlertSeverity::Info) => tracing::info!(
            rule = %ev.rule_name,
            metric = %ev.metric,
            value = ev.value,
            threshold = ev.threshold,
            "alert FIRED (info)"
        ),
        (AlertEventKind::Recovered, _) => tracing::info!(
            rule = %ev.rule_name,
            metric = %ev.metric,
            value = ev.value,
            "alert recovered"
        ),
    }
}

fn ring_bell() {
    use std::io::Write;
    // BEL char to stderr; the alt-screen TUI doesn't intercept stderr,
    // and most terminal emulators flash or beep on \x07.
    let _ = std::io::stderr().write_all(b"\x07");
    let _ = std::io::stderr().flush();
}

fn send_sigterm(pid: u32) -> Result<()> {
    // SAFETY: libc::kill is safe to call with any pid; it just returns -1 on
    // failure (e.g. ESRCH if the process is gone). pid is bounded by i32 since
    // it came from sysinfo::Pid::as_u32 on a Unix process id.
    let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error().into())
    }
}

fn enter_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(Into::into)
}

fn leave_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Restore terminal on panic so the user's shell isn't left in raw mode.
fn install_panic_hook() {
    let original = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original(info);
    }));
}

fn spawn_sampler(
    state: SharedState,
    config: Config,
    gpu_enabled: bool,
    alert_rules: Vec<AlertRule>,
    shutdown: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("sampler".into())
        .spawn(move || sampler_loop(state, config, gpu_enabled, alert_rules, shutdown))
        .expect("failed to spawn sampler thread")
}

fn sampler_loop(
    state: SharedState,
    config: Config,
    gpu_enabled: bool,
    alert_rules: Vec<AlertRule>,
    shutdown: Arc<AtomicBool>,
) {
    let mut system = SystemSource::new();
    let mut registry = Registry::new();
    registry.register(Box::new(CpuCollector::new()));
    registry.register(Box::new(MemoryCollector::new()));
    registry.register(Box::new(DiskCollector::new()));
    registry.register(Box::new(NetworkCollector::new()));
    registry.register(Box::new(SensorsCollector::new()));
    registry.register(Box::new(ContainerCollector::new()));
    registry.register(Box::new(ProcessCollector::new()));

    #[cfg(target_os = "macos")]
    if gpu_enabled {
        match crate::collector::GpuCollector::try_new() {
            Ok(c) => registry.register(Box::new(c)),
            Err(e) => tracing::warn!(error = %e, "GPU collector disabled"),
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = gpu_enabled;

    let mut evaluator = AlertEvaluator::new(alert_rules);

    let interval = config.sample_interval();
    while !shutdown.load(Ordering::SeqCst) {
        let started = Instant::now();
        let mut snapshot = Snapshot::new();
        registry.sample_all(&mut system, &mut snapshot);

        // Run alerts after collectors have populated this tick's snapshot;
        // stamp firing set into the snapshot so the UI can colour borders.
        let out = evaluator.evaluate(&snapshot, Instant::now());
        for ev in &out.events {
            log_event(ev);
        }
        if !out.events.is_empty() {
            ring_bell();
        }
        snapshot.firing_alerts = out.firing_now;
        state.commit(snapshot, out.events);

        let elapsed = started.elapsed();
        let sleep_for = interval.saturating_sub(elapsed);
        sleep_until_or_shutdown(sleep_for, &shutdown);
    }
    tracing::debug!("sampler thread exiting");
}

/// Sleep in small chunks so shutdown is observed promptly.
fn sleep_until_or_shutdown(total: Duration, shutdown: &AtomicBool) {
    let chunk = Duration::from_millis(50);
    let mut remaining = total;
    while remaining > Duration::ZERO {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        let step = remaining.min(chunk);
        thread::sleep(step);
        remaining = remaining.saturating_sub(step);
    }
}
