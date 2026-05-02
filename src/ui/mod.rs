pub mod format;
pub mod state;
pub mod theme;
pub mod widgets;

use std::collections::HashSet;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::SharedState;
use crate::ui::state::{Panel, UiState, panel_for_metric};
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, state: &SharedState, ui: &mut UiState, theme: &Theme) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),  // header
        Constraint::Length(11), // CPU + Mem
        Constraint::Length(8),  // Network + Sensors
        Constraint::Length(6),  // Disk + Container
        Constraint::Min(8),     // Process
        Constraint::Length(1),  // footer
    ])
    .split(area);

    // Top row: CPU + Mem (+ GPU when enabled). GPU gets the smallest slice
    // since it shows fewer numbers; CPU keeps the largest because per-core
    // gauges need horizontal space.
    let top = if ui.gpu_enabled {
        Layout::horizontal([
            Constraint::Percentage(45),
            Constraint::Percentage(35),
            Constraint::Percentage(20),
        ])
        .split(chunks[1])
    } else {
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1])
    };
    let mid = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[2]);
    let disk_row = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[3]);

    // Single state read for the whole frame: header, panels, and the alert
    // overlay all see the same snapshot. Saves redundant lock acquisition
    // and ensures the alert count in the header matches what panels render.
    state.with_view(|view| {
        let firing_panels: HashSet<Panel> = view
            .current
            .map(|s| {
                s.firing_alerts
                    .iter()
                    .filter_map(|a| panel_for_metric(&a.metric))
                    .collect()
            })
            .unwrap_or_default();
        let firing_count = view.current.map(|s| s.firing_alerts.len()).unwrap_or(0);
        let f = |p: Panel| firing_panels.contains(&p);

        // Header — flag the alert count when there are firing alerts.
        let mut header_spans = vec![
            Span::styled(
                "resource-monitor",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(concat!("v", env!("CARGO_PKG_VERSION")), theme.dim_style()),
        ];
        if firing_count > 0 {
            header_spans.push(Span::raw("  "));
            header_spans.push(Span::styled(
                format!("⚠ {firing_count} alert(s) firing"),
                Style::default()
                    .fg(theme.gauge_high)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(header_spans)), chunks[0]);

        widgets::cpu::render(
            frame,
            top[0],
            &view,
            theme,
            ui.focus == Panel::Cpu,
            f(Panel::Cpu),
        );
        widgets::memory::render(
            frame,
            top[1],
            &view,
            theme,
            ui.focus == Panel::Memory,
            f(Panel::Memory),
        );
        if ui.gpu_enabled {
            widgets::gpu::render(
                frame,
                top[2],
                &view,
                theme,
                ui.focus == Panel::Gpu,
                f(Panel::Gpu),
            );
        }
        widgets::network::render(
            frame,
            mid[0],
            &view,
            theme,
            ui.focus == Panel::Network,
            f(Panel::Network),
        );
        widgets::sensors::render(
            frame,
            mid[1],
            &view,
            theme,
            ui.focus == Panel::Sensors,
            f(Panel::Sensors),
        );
        widgets::disk::render(
            frame,
            disk_row[0],
            &view,
            theme,
            ui.focus == Panel::Disk,
            f(Panel::Disk),
        );
        widgets::container::render(
            frame,
            disk_row[1],
            &view,
            theme,
            ui.focus == Panel::Container,
            f(Panel::Container),
        );
        widgets::process::render(
            frame,
            chunks[4],
            &view,
            theme,
            ui,
            ui.focus == Panel::Process,
            f(Panel::Process),
        );

        if ui.show_help {
            widgets::help::render(frame, area, theme);
        } else if ui.show_alerts {
            widgets::alerts::render(frame, area, &view, theme);
        }
    });

    let footer_text = if ui.editing_search {
        format!(
            "/ {}_  (Enter accept, Esc cancel)",
            ui.search.as_deref().unwrap_or("")
        )
    } else if let Some(pid) = ui.kill_pending {
        format!("kill PID {pid}? (Enter=SIGTERM, Esc=cancel)")
    } else {
        "q quit  Tab focus  ↑↓ select  c/m/p/n sort  g group  / search  F9 kill  a alerts  ? help"
            .to_string()
    };
    let footer = Paragraph::new(footer_text).style(theme.dim_style());
    frame.render_widget(footer, chunks[5]);
}
