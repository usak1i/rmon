use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Sparkline};

use crate::state::StateView;
use crate::ui::format;
use crate::ui::theme::Theme;
use crate::ui::widgets::series_to_u64;

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &StateView,
    theme: &Theme,
    focused: bool,
    firing: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style_for(focused, firing))
        .title(Span::styled(" Memory ", theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };

    let total = snap.get("mem.total_bytes").unwrap_or(0.0);
    let used = snap.get("mem.used_bytes").unwrap_or(0.0);
    let avail = snap.get("mem.available_bytes").unwrap_or(0.0);
    let swap_total = snap.get("mem.swap_total_bytes").unwrap_or(0.0);
    let swap_used = snap.get("mem.swap_used_bytes").unwrap_or(0.0);

    let used_pct = if total > 0.0 {
        used / total * 100.0
    } else {
        0.0
    };
    let swap_pct = if swap_total > 0.0 {
        swap_used / swap_total * 100.0
    } else {
        0.0
    };

    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    // Sparkline of memory used % (computed from raw used/total at each tick).
    let spark_data = mem_pct_history(view, total);
    let spark = Sparkline::default()
        .data(&spark_data)
        .max(100)
        .style(Style::default().fg(theme.gauge_for(used_pct)));
    frame.render_widget(spark, chunks[0]);

    let ram_gauge = Gauge::default()
        .ratio((used_pct / 100.0).clamp(0.0, 1.0))
        .label(Span::styled(
            format!(
                "RAM  {} / {}  ({used_pct:.1}%)",
                format::bytes(used as u64),
                format::bytes(total as u64),
            ),
            Style::default().fg(Color::White),
        ))
        .gauge_style(Style::default().fg(theme.gauge_for(used_pct)));
    frame.render_widget(ram_gauge, chunks[1]);

    let swap_gauge = Gauge::default()
        .ratio((swap_pct / 100.0).clamp(0.0, 1.0))
        .label(Span::styled(
            format!(
                "Swap {} / {}  ({swap_pct:.1}%)",
                format::bytes(swap_used as u64),
                format::bytes(swap_total as u64),
            ),
            Style::default().fg(Color::White),
        ))
        .gauge_style(Style::default().fg(theme.gauge_for(swap_pct)));
    frame.render_widget(swap_gauge, chunks[2]);

    let info = Paragraph::new(Line::from(vec![
        Span::styled("avail ", theme.dim_style()),
        Span::raw(format::bytes(avail as u64)),
    ]));
    frame.render_widget(info, chunks[3]);
}

/// Convert mem.used_bytes history into a 0..100 percentage series for the
/// sparkline. Each historical sample is divided by the *current* total
/// (close enough — total rarely changes mid-session).
fn mem_pct_history(view: &StateView, total: f64) -> Vec<u64> {
    if total <= 0.0 {
        return Vec::new();
    }
    let series = view.history.series("mem.used_bytes");
    series_to_u64(series, total)
}
