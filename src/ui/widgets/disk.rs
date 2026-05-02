use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::state::StateView;
use crate::ui::format;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, view: &StateView, theme: &Theme, focused: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(focused))
        .title(Span::styled(" Disks ", theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };

    if snap.disks.is_empty() {
        frame.render_widget(
            Paragraph::new("no disks discovered").style(theme.dim_style()),
            inner,
        );
        return;
    }

    let header_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from("MOUNT").style(header_style),
        Cell::from("FS").style(header_style),
        Cell::from("SIZE").style(header_style),
        Cell::from("USED").style(header_style),
        Cell::from("FREE").style(header_style),
        Cell::from("USE%").style(header_style),
    ]);

    let rows = snap.disks.iter().map(|d| {
        let used = d.total_bytes.saturating_sub(d.available_bytes);
        let pct = if d.total_bytes > 0 {
            used as f64 / d.total_bytes as f64 * 100.0
        } else {
            0.0
        };
        let pct_color = theme.gauge_for(pct);
        Row::new(vec![
            Cell::from(d.mount_point.clone()),
            Cell::from(d.fs_type.clone()).style(theme.dim_style()),
            Cell::from(format::bytes(d.total_bytes)),
            Cell::from(format::bytes(used)),
            Cell::from(format::bytes(d.available_bytes)),
            Cell::from(format!("{pct:>5.1}%")).style(Style::default().fg(pct_color)),
        ])
    });

    let widths = [
        Constraint::Percentage(34),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(7),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .style(Style::default().fg(Color::White))
        .column_spacing(1);
    frame.render_widget(table, inner);
}
