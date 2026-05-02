use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::state::{ContainerSnapshot, ProcessSnapshot, StateView};
use crate::ui::format;
use crate::ui::state::{ProcessSort, UiState};
use crate::ui::theme::Theme;

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &StateView,
    theme: &Theme,
    ui: &mut UiState,
    focused: bool,
    firing: bool,
) {
    let title_text = build_title(view, ui);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style_for(focused, firing))
        .title(Span::styled(title_text, theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };

    let total_mem = snap.get("mem.total_bytes").unwrap_or(0.0);

    // Build the visible row list. Each entry is either a process row
    // (carries a snapshot index) or a header row (container or system bucket).
    let rows = if ui.grouped_mode {
        build_grouped_rows(
            snap.processes.as_slice(),
            &snap.containers,
            ui.search.as_deref(),
            ui.process_sort,
        )
    } else {
        let mut indices: Vec<RowKind> = (0..snap.processes.len())
            .filter(|i| matches_search(&snap.processes[*i], ui.search.as_deref()))
            .map(RowKind::Process)
            .collect();
        sort_process_rows(&mut indices, &snap.processes, ui.process_sort);
        indices
    };

    // Clamp selection.
    if let Some(sel) = ui.process_table.selected() {
        if rows.is_empty() {
            ui.process_table.select(None);
        } else if sel >= rows.len() {
            ui.process_table.select(Some(rows.len() - 1));
        }
    } else if !rows.is_empty() {
        ui.process_table.select(Some(0));
    }

    let header_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let sort_marker = |s: ProcessSort| if ui.process_sort == s { "▼" } else { " " };
    let header = Row::new(vec![
        Cell::from(format!("{}PID", sort_marker(ProcessSort::Pid))).style(header_style),
        Cell::from("USER").style(header_style),
        Cell::from(format!("{}CPU%", sort_marker(ProcessSort::Cpu))).style(header_style),
        Cell::from(format!("{}MEM%", sort_marker(ProcessSort::Memory))).style(header_style),
        Cell::from("S").style(header_style),
        Cell::from("TIME").style(header_style),
        Cell::from(format!("{}COMMAND", sort_marker(ProcessSort::Name))).style(header_style),
    ]);

    // Walk the row list once, producing parallel Vec<Row> and the
    // visible-PIDs list the key handlers consult.
    let mut visible_pids: Vec<Option<u32>> = Vec::with_capacity(rows.len());
    let mut table_rows: Vec<Row> = Vec::with_capacity(rows.len());
    for kind in &rows {
        match kind {
            RowKind::Process(idx) => {
                let p = &snap.processes[*idx];
                visible_pids.push(Some(p.pid));
                table_rows.push(make_process_row(p, total_mem, ui.grouped_mode, theme));
            }
            RowKind::ContainerHeader {
                container_idx,
                agg_cpu,
                agg_mem,
            } => {
                visible_pids.push(None);
                let c = &snap.containers[*container_idx];
                table_rows.push(make_container_header_row(
                    c, *agg_cpu, *agg_mem, total_mem, theme,
                ));
            }
            RowKind::SystemHeader { agg_cpu, agg_mem } => {
                visible_pids.push(None);
                table_rows.push(make_system_header_row(*agg_cpu, *agg_mem, total_mem, theme));
            }
        }
    }
    ui.last_visible_pids = visible_pids;

    let widths = [
        Constraint::Length(7),
        Constraint::Length(10),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(2),
        Constraint::Length(10),
        Constraint::Min(10),
    ];
    let table = Table::new(table_rows, widths)
        .header(header)
        .row_highlight_style(
            Style::default()
                .bg(theme.selected_bg)
                .fg(theme.selected_fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ")
        .column_spacing(1);

    frame.render_stateful_widget(table, inner, &mut ui.process_table);
}

fn build_title(view: &StateView, ui: &UiState) -> String {
    let count = view.current.map(|s| s.processes.len());
    let mode = if ui.grouped_mode { " grouped" } else { "" };
    match (&ui.search, count) {
        (Some(q), Some(n)) => format!(" Processes ({n}){mode}  /{q} "),
        (Some(q), None) => format!(" Processes{mode}  /{q} "),
        (None, Some(n)) => format!(" Processes ({n}){mode} "),
        (None, None) => format!(" Processes{mode} "),
    }
}

/// One displayed row: either a container/system header (no PID) or a
/// process row (snapshot index).
enum RowKind {
    Process(usize),
    ContainerHeader {
        container_idx: usize,
        agg_cpu: f32,
        agg_mem: u64,
    },
    SystemHeader {
        agg_cpu: f32,
        agg_mem: u64,
    },
}

fn build_grouped_rows(
    processes: &[ProcessSnapshot],
    containers: &[ContainerSnapshot],
    search: Option<&str>,
    sort: ProcessSort,
) -> Vec<RowKind> {
    let mut out = Vec::new();

    // For each container, find children whose container_id starts with the
    // short ID (cgroup gives 64-hex, docker stats gives 12-hex).
    for (cidx, c) in containers.iter().enumerate() {
        let mut child_indices: Vec<usize> = (0..processes.len())
            .filter(|i| {
                processes[*i]
                    .container_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(&c.id))
                    && matches_search(&processes[*i], search)
            })
            .collect();
        if child_indices.is_empty() {
            continue;
        }
        sort_process_indices(&mut child_indices, processes, sort);

        let agg_cpu: f32 = child_indices
            .iter()
            .map(|i| processes[*i].cpu_percent)
            .sum();
        let agg_mem: u64 = child_indices
            .iter()
            .map(|i| processes[*i].memory_bytes)
            .sum();
        out.push(RowKind::ContainerHeader {
            container_idx: cidx,
            agg_cpu,
            agg_mem,
        });
        for i in child_indices {
            out.push(RowKind::Process(i));
        }
    }

    // System bucket: processes with no container_id (or whose container_id
    // didn't match any known container).
    let known_short_ids: Vec<&str> = containers.iter().map(|c| c.id.as_str()).collect();
    let mut system_indices: Vec<usize> = (0..processes.len())
        .filter(|i| {
            let in_known_container = processes[*i]
                .container_id
                .as_deref()
                .is_some_and(|id| known_short_ids.iter().any(|s| id.starts_with(s)));
            !in_known_container && matches_search(&processes[*i], search)
        })
        .collect();
    if !system_indices.is_empty() {
        sort_process_indices(&mut system_indices, processes, sort);
        let agg_cpu: f32 = system_indices
            .iter()
            .map(|i| processes[*i].cpu_percent)
            .sum();
        let agg_mem: u64 = system_indices
            .iter()
            .map(|i| processes[*i].memory_bytes)
            .sum();
        out.push(RowKind::SystemHeader { agg_cpu, agg_mem });
        for i in system_indices {
            out.push(RowKind::Process(i));
        }
    }

    out
}

fn sort_process_indices(idx: &mut [usize], procs: &[ProcessSnapshot], sort: ProcessSort) {
    match sort {
        ProcessSort::Cpu => idx.sort_by(|a, b| {
            procs[*b]
                .cpu_percent
                .partial_cmp(&procs[*a].cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        ProcessSort::Memory => idx.sort_by_key(|i| std::cmp::Reverse(procs[*i].memory_bytes)),
        ProcessSort::Pid => idx.sort_by_key(|i| procs[*i].pid),
        ProcessSort::Name => idx.sort_by(|a, b| procs[*a].command.cmp(&procs[*b].command)),
    }
}

fn sort_process_rows(rows: &mut [RowKind], procs: &[ProcessSnapshot], sort: ProcessSort) {
    rows.sort_by(|a, b| {
        let (RowKind::Process(ai), RowKind::Process(bi)) = (a, b) else {
            return std::cmp::Ordering::Equal;
        };
        match sort {
            ProcessSort::Cpu => procs[*bi]
                .cpu_percent
                .partial_cmp(&procs[*ai].cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal),
            ProcessSort::Memory => procs[*bi].memory_bytes.cmp(&procs[*ai].memory_bytes),
            ProcessSort::Pid => procs[*ai].pid.cmp(&procs[*bi].pid),
            ProcessSort::Name => procs[*ai].command.cmp(&procs[*bi].command),
        }
    });
}

fn make_process_row<'a>(
    p: &'a ProcessSnapshot,
    total_mem: f64,
    indented: bool,
    theme: &Theme,
) -> Row<'a> {
    let mem_pct = if total_mem > 0.0 {
        (p.memory_bytes as f64 / total_mem * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let cpu_color = theme.gauge_for(p.cpu_percent.clamp(0.0, 100.0) as f64);
    let prefix = if indented { "  " } else { "" };
    Row::new(vec![
        Cell::from(p.pid.to_string()),
        Cell::from(p.user.clone()).style(theme.dim_style()),
        Cell::from(format!("{:>5.1}", p.cpu_percent.clamp(0.0, 999.9)))
            .style(Style::default().fg(cpu_color)),
        Cell::from(format!("{mem_pct:>4.1}")),
        Cell::from(p.status.to_string()),
        Cell::from(format::run_time(p.run_time_secs)).style(theme.dim_style()),
        Cell::from(format!("{prefix}{}", p.command)),
    ])
}

fn make_container_header_row<'a>(
    c: &'a ContainerSnapshot,
    agg_cpu: f32,
    agg_mem: u64,
    total_mem: f64,
    theme: &Theme,
) -> Row<'a> {
    let mem_pct = if total_mem > 0.0 {
        (agg_mem as f64 / total_mem * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let bold = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let short_id: String = c.id.chars().take(12).collect();
    Row::new(vec![
        Cell::from("").style(theme.dim_style()),
        Cell::from(short_id).style(theme.dim_style()),
        Cell::from(format!("{:>5.1}", agg_cpu)).style(bold),
        Cell::from(format!("{mem_pct:>4.1}")).style(bold),
        Cell::from(""),
        Cell::from(""),
        Cell::from(format!("▼ {}", c.name)).style(bold),
    ])
}

fn make_system_header_row<'a>(
    agg_cpu: f32,
    agg_mem: u64,
    total_mem: f64,
    theme: &Theme,
) -> Row<'a> {
    let mem_pct = if total_mem > 0.0 {
        (agg_mem as f64 / total_mem * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let bold = Style::default().fg(theme.dim).add_modifier(Modifier::BOLD);
    Row::new(vec![
        Cell::from(""),
        Cell::from(""),
        Cell::from(format!("{:>5.1}", agg_cpu)).style(bold),
        Cell::from(format!("{mem_pct:>4.1}")).style(bold),
        Cell::from(""),
        Cell::from(""),
        Cell::from("▼ system").style(bold),
    ])
}

fn matches_search(p: &ProcessSnapshot, query: Option<&str>) -> bool {
    let Some(q) = query else { return true };
    if q.is_empty() {
        return true;
    }
    let needle = q.to_ascii_lowercase();
    p.command.to_ascii_lowercase().contains(&needle)
        || p.user.to_ascii_lowercase().contains(&needle)
        || p.pid.to_string().contains(&needle)
}
