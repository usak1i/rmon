use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Sparkline};

use crate::state::StateView;
use crate::ui::theme::Theme;
use crate::ui::widgets::series_to_u64;

pub fn render(frame: &mut Frame<'_>, area: Rect, view: &StateView, theme: &Theme, focused: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(focused))
        .title(Span::styled(" GPU ", theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };

    // No GPU samples yet — likely powermetrics still warming up or sudo failed.
    let usage = snap.get("gpu.usage");
    let freq = snap.get("gpu.freq_mhz");
    let power_mw = snap.get("gpu.power_mw");
    if usage.is_none() && freq.is_none() && power_mw.is_none() {
        frame.render_widget(
            Paragraph::new("waiting for powermetrics…").style(theme.dim_style()),
            inner,
        );
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(2), // sparkline
        Constraint::Length(1), // usage gauge
        Constraint::Length(1), // freq line
        Constraint::Length(1), // power line
        Constraint::Min(0),
    ])
    .split(inner);

    let usage_pct = usage.unwrap_or(0.0).clamp(0.0, 100.0);
    let spark_data = series_to_u64(view.history.series("gpu.usage"), 100.0);
    let spark = Sparkline::default()
        .data(spark_data)
        .max(100)
        .style(Style::default().fg(theme.gauge_for(usage_pct)));
    frame.render_widget(spark, chunks[0]);

    let usage_gauge = Gauge::default()
        .ratio(usage_pct / 100.0)
        .label(Span::styled(
            format!("usage {usage_pct:>5.1}%"),
            Style::default().fg(Color::White),
        ))
        .gauge_style(Style::default().fg(theme.gauge_for(usage_pct)));
    frame.render_widget(usage_gauge, chunks[1]);

    let freq_line = Paragraph::new(Line::from(vec![
        Span::styled("freq  ", theme.dim_style()),
        Span::raw(match freq {
            Some(f) => format!("{f:>4.0} MHz"),
            None => "—".to_string(),
        }),
    ]));
    frame.render_widget(freq_line, chunks[2]);

    let power_line = Paragraph::new(Line::from(vec![
        Span::styled("power ", theme.dim_style()),
        Span::raw(match power_mw {
            Some(p) if p >= 1000.0 => format!("{:.2} W", p / 1000.0),
            Some(p) => format!("{p:>4.0} mW"),
            None => "—".to_string(),
        }),
    ]));
    frame.render_widget(power_line, chunks[3]);
}
