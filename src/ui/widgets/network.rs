use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table};

use crate::state::StateView;
use crate::ui::format;
use crate::ui::theme::Theme;

/// Cap the sparkline at this many bytes/sec for visual scaling. Beyond this
/// the bar saturates — fine for a terminal sparkline since the table below
/// shows the exact rate.
const SPARK_CEILING_BPS: f64 = 100.0 * 1024.0 * 1024.0; // 100 MB/s

pub fn render(frame: &mut Frame<'_>, area: Rect, view: &StateView, theme: &Theme, focused: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(focused))
        .title(Span::styled(" Network ", theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };

    // Optional 1-line conn footer (Linux only).
    let has_conns = snap.get("net.conn.tcp_established").is_some();
    let chunks = if has_conns {
        Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner)
    } else {
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(inner)
    };

    let total_rx = snap.get("net.total.rx_bps").unwrap_or(0.0);
    let total_tx = snap.get("net.total.tx_bps").unwrap_or(0.0);

    // Combined RX+TX sparkline. We sum the two series point-wise from history.
    let spark_data = combined_history(view);
    let spark = Sparkline::default()
        .data(&spark_data)
        .max(100)
        .style(Style::default().fg(theme.accent));
    frame.render_widget(spark, chunks[0]);

    if snap.networks.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("↓ ", theme.dim_style()),
                Span::raw(format!("{}/s", format::bytes(total_rx as u64))),
                Span::raw("   "),
                Span::styled("↑ ", theme.dim_style()),
                Span::raw(format!("{}/s", format::bytes(total_tx as u64))),
                Span::styled("    no interfaces", theme.dim_style()),
            ])),
            chunks[1],
        );
        return;
    }

    let header_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from("IFACE").style(header_style),
        Cell::from("↓ RX/s").style(header_style),
        Cell::from("↑ TX/s").style(header_style),
        Cell::from("Σ↓").style(header_style),
        Cell::from("Σ↑").style(header_style),
    ]);

    let rows = snap.networks.iter().map(|n| {
        Row::new(vec![
            Cell::from(n.interface.clone()),
            Cell::from(format!("{}/s", format::bytes(n.rx_bps as u64))),
            Cell::from(format!("{}/s", format::bytes(n.tx_bps as u64))),
            Cell::from(format::bytes(n.total_rx_bytes)).style(theme.dim_style()),
            Cell::from(format::bytes(n.total_tx_bytes)).style(theme.dim_style()),
        ])
    });

    let widths = [
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(8),
    ];
    let table = Table::new(rows, widths).header(header).column_spacing(1);
    frame.render_widget(table, chunks[1]);

    if has_conns {
        let est = snap.get("net.conn.tcp_established").unwrap_or(0.0) as u32;
        let lis = snap.get("net.conn.tcp_listen").unwrap_or(0.0) as u32;
        let tw = snap.get("net.conn.tcp_time_wait").unwrap_or(0.0) as u32;
        let udp = snap.get("net.conn.udp").unwrap_or(0.0) as u32;
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("TCP ", theme.dim_style()),
            Span::raw(format!("{est} estab")),
            Span::styled(" · ", theme.dim_style()),
            Span::raw(format!("{lis} listen")),
            Span::styled(" · ", theme.dim_style()),
            Span::raw(format!("{tw} wait")),
            Span::styled("    UDP ", theme.dim_style()),
            Span::raw(format!("{udp}")),
        ]));
        frame.render_widget(footer, chunks[2]);
    }
}

fn combined_history(view: &StateView) -> Vec<u64> {
    let rx = view.history.series("net.total.rx_bps");
    let tx = view.history.series("net.total.tx_bps");
    match (rx, tx) {
        (Some(a), Some(b)) => {
            let n = a.len().min(b.len());
            a.iter()
                .rev()
                .take(n)
                .zip(b.iter().rev().take(n))
                .map(|(x, y)| {
                    let v = (x + y).clamp(0.0, SPARK_CEILING_BPS);
                    (v / SPARK_CEILING_BPS * 100.0) as u64
                })
                .rev()
                .collect()
        }
        _ => Vec::new(),
    }
}
