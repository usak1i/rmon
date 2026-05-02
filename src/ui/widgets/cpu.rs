use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Sparkline};

use crate::state::StateView;
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
        .title(Span::styled(" CPU ", theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        let p = Paragraph::new("…").style(theme.dim_style());
        frame.render_widget(p, inner);
        return;
    };

    // Determine core count from snapshot keys.
    let core_count = (0..)
        .take_while(|i| snap.get(&format!("cpu.core.{i}")).is_some())
        .count();

    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);

    // Sparkline of cpu.total
    let total_data = series_to_u64(view.history.series("cpu.total"), 100.0);
    let total_now = snap.get("cpu.total").unwrap_or(0.0);
    let spark = Sparkline::default()
        .data(&total_data)
        .max(100)
        .style(Style::default().fg(theme.gauge_for(total_now)));
    frame.render_widget(spark, chunks[0]);

    // Per-core gauges, packed in two columns if there are many.
    let cores_area = chunks[1];
    let cols = if core_count > 8 { 2 } else { 1 };
    let col_constraints: Vec<Constraint> = (0..cols)
        .map(|_| Constraint::Ratio(1, cols as u32))
        .collect();
    let columns = Layout::horizontal(col_constraints).split(cores_area);

    let per_col = core_count.div_ceil(cols);
    for (col_idx, col_rect) in columns.iter().enumerate() {
        let start = col_idx * per_col;
        let end = (start + per_col).min(core_count);
        if start >= end {
            continue;
        }
        let row_count = end - start;
        let row_constraints: Vec<Constraint> =
            (0..row_count).map(|_| Constraint::Length(1)).collect();
        let rows = Layout::vertical(row_constraints).split(*col_rect);
        for (row_idx, row_rect) in rows.iter().enumerate() {
            let core = start + row_idx;
            let pct = snap
                .get(&format!("cpu.core.{core}"))
                .unwrap_or(0.0)
                .clamp(0.0, 100.0);
            let gauge = Gauge::default()
                .ratio(pct / 100.0)
                .label(Span::styled(
                    format!("{core:>2} {pct:>5.1}%"),
                    Style::default().fg(Color::White),
                ))
                .gauge_style(Style::default().fg(theme.gauge_for(pct)));
            frame.render_widget(gauge, *row_rect);
        }
    }

    // Load average footer.
    let load1 = snap.get("cpu.load.1").unwrap_or(0.0);
    let load5 = snap.get("cpu.load.5").unwrap_or(0.0);
    let load15 = snap.get("cpu.load.15").unwrap_or(0.0);
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("load ", theme.dim_style()),
        Span::raw(format!("{load1:.2}")),
        Span::styled(" / ", theme.dim_style()),
        Span::raw(format!("{load5:.2}")),
        Span::styled(" / ", theme.dim_style()),
        Span::raw(format!("{load15:.2}")),
    ]));
    frame.render_widget(footer, chunks[2]);
}
