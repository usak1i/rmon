use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::state::StateView;
use crate::ui::format;
use crate::ui::theme::Theme;

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &StateView,
    theme: &Theme,
    focused: bool,
    firing: bool,
) {
    let title = match view.current.map(|s| s.containers.len()) {
        Some(n) if n > 0 => format!(" Containers ({n}) "),
        _ => " Containers ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style_for(focused, firing))
        .title(Span::styled(title, theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };

    let available = snap.get("container.available").unwrap_or(0.0) > 0.5;
    if !available {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "docker daemon not reachable",
                theme.dim_style(),
            )])),
            inner,
        );
        return;
    }

    if snap.containers.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "no running containers",
                theme.dim_style(),
            )])),
            inner,
        );
        return;
    }

    let header_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from("NAME").style(header_style),
        Cell::from("CPU%").style(header_style),
        Cell::from("MEM").style(header_style),
        Cell::from("MEM%").style(header_style),
        Cell::from("↓ NET").style(header_style),
        Cell::from("↑ NET").style(header_style),
    ]);

    let rows = snap.containers.iter().map(|c| {
        let cpu_color = theme.gauge_for(c.cpu_percent.clamp(0.0, 100.0));
        let mem_color = theme.gauge_for(c.mem_percent.clamp(0.0, 100.0));
        Row::new(vec![
            Cell::from(c.name.clone()),
            Cell::from(format!("{:>5.1}", c.cpu_percent)).style(Style::default().fg(cpu_color)),
            Cell::from(format::bytes(c.mem_bytes)),
            Cell::from(format!("{:>4.1}", c.mem_percent)).style(Style::default().fg(mem_color)),
            Cell::from(format::bytes(c.net_rx_bytes)).style(theme.dim_style()),
            Cell::from(format::bytes(c.net_tx_bytes)).style(theme.dim_style()),
        ])
    });

    let widths = [
        Constraint::Min(10),
        Constraint::Length(5),
        Constraint::Length(7),
        Constraint::Length(5),
        Constraint::Length(7),
        Constraint::Length(7),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .style(Style::default().fg(Color::White))
        .column_spacing(1);
    frame.render_widget(table, inner);
}
