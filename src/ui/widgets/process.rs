use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::state::{ProcessSnapshot, StateView};
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
) {
    let title_text = match (&ui.search, view.current.map(|s| s.processes.len())) {
        (Some(q), Some(n)) => format!(" Processes ({n})  /{q} "),
        (Some(q), None) => format!(" Processes  /{q} "),
        (None, Some(n)) => format!(" Processes ({n}) "),
        (None, None) => " Processes ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(focused))
        .title(Span::styled(title_text, theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };

    let total_mem = snap.get("mem.total_bytes").unwrap_or(0.0);

    let mut indices: Vec<usize> = (0..snap.processes.len())
        .filter(|i| matches_search(&snap.processes[*i], ui.search.as_deref()))
        .collect();

    sort_indices(&mut indices, &snap.processes, ui.process_sort);

    // Clamp selection.
    let len = indices.len();
    if let Some(sel) = ui.process_table.selected() {
        if len == 0 {
            ui.process_table.select(None);
        } else if sel >= len {
            ui.process_table.select(Some(len - 1));
        }
    } else if len > 0 {
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

    let rows = indices.iter().map(|&i| {
        let p = &snap.processes[i];
        let mem_pct = if total_mem > 0.0 {
            (p.memory_bytes as f64 / total_mem * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let cpu_color = theme.gauge_for(p.cpu_percent.clamp(0.0, 100.0) as f64);
        Row::new(vec![
            Cell::from(p.pid.to_string()),
            Cell::from(p.user.clone()).style(theme.dim_style()),
            Cell::from(format!("{:>5.1}", p.cpu_percent.clamp(0.0, 999.9)))
                .style(Style::default().fg(cpu_color)),
            Cell::from(format!("{mem_pct:>4.1}")),
            Cell::from(p.status.to_string()),
            Cell::from(format::run_time(p.run_time_secs)).style(theme.dim_style()),
            Cell::from(p.command.clone()),
        ])
    });

    let widths = [
        Constraint::Length(7),
        Constraint::Length(10),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(2),
        Constraint::Length(10),
        Constraint::Min(10),
    ];
    let table = Table::new(rows, widths)
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

    // Stash sort order for kill confirmation lookup.
    ui.last_visible_pids = indices.iter().map(|&i| snap.processes[i].pid).collect();
}

fn sort_indices(idx: &mut [usize], procs: &[ProcessSnapshot], sort: ProcessSort) {
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
