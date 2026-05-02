pub mod format;
pub mod state;
pub mod theme;
pub mod widgets;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::SharedState;
use crate::ui::state::{Panel, UiState};
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, state: &SharedState, ui: &mut UiState, theme: &Theme) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),  // header
        Constraint::Length(11), // CPU + Mem
        Constraint::Length(8),  // Network + Sensors
        Constraint::Length(6),  // Disk
        Constraint::Min(8),     // Process
        Constraint::Length(1),  // footer
    ])
    .split(area);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "resource-monitor",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(concat!("v", env!("CARGO_PKG_VERSION")), theme.dim_style()),
    ]));
    frame.render_widget(header, chunks[0]);

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

    state.with_view(|view| {
        widgets::cpu::render(frame, top[0], &view, theme, ui.focus == Panel::Cpu);
        widgets::memory::render(frame, top[1], &view, theme, ui.focus == Panel::Memory);
        if ui.gpu_enabled {
            widgets::gpu::render(frame, top[2], &view, theme, ui.focus == Panel::Gpu);
        }
        widgets::network::render(frame, mid[0], &view, theme, ui.focus == Panel::Network);
        widgets::sensors::render(frame, mid[1], &view, theme, ui.focus == Panel::Sensors);
        widgets::disk::render(frame, disk_row[0], &view, theme, ui.focus == Panel::Disk);
        widgets::container::render(
            frame,
            disk_row[1],
            &view,
            theme,
            ui.focus == Panel::Container,
        );
        widgets::process::render(
            frame,
            chunks[4],
            &view,
            theme,
            ui,
            ui.focus == Panel::Process,
        );
    });

    let footer_text = if ui.editing_search {
        format!(
            "/ {}_  (Enter accept, Esc cancel)",
            ui.search.as_deref().unwrap_or("")
        )
    } else if let Some(pid) = ui.kill_pending {
        format!("kill PID {pid}? (Enter=SIGTERM, Esc=cancel)")
    } else {
        "q quit  Tab focus  ↑↓ select  c/m/p/n sort  / search  F9 kill  ? help".to_string()
    };
    let footer = Paragraph::new(footer_text).style(theme.dim_style());
    frame.render_widget(footer, chunks[5]);

    if ui.show_help {
        widgets::help::render(frame, area, theme);
    }
}
