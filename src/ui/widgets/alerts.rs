use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::alert::{AlertEventKind, AlertSeverity};
use crate::state::StateView;
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, view: &StateView, theme: &Theme) {
    let popup = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup);

    let title = match view.current.map(|s| s.firing_alerts.len()).unwrap_or(0) {
        0 => " Alerts (0 firing) ".to_string(),
        n => format!(" Alerts ({n} firing) "),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(title, theme.title()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::vertical([
        Constraint::Length(1), // "Firing now" header
        Constraint::Min(3),    // firing list (or empty msg)
        Constraint::Length(1), // "Recent" header
        Constraint::Min(3),    // recent list
        Constraint::Length(1), // footer hint
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "Firing now",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )])),
        chunks[0],
    );

    let firing_lines: Vec<Line> = match view.current {
        Some(snap) if !snap.firing_alerts.is_empty() => snap
            .firing_alerts
            .iter()
            .map(|a| {
                Line::from(vec![
                    Span::styled(
                        format!("  {:<10} ", severity_label(a.severity)),
                        severity_style(theme, a.severity),
                    ),
                    Span::raw(format!("{:<24} ", a.rule_name)),
                    Span::styled(
                        format!("{:>10}={:.2} ", a.metric, a.value),
                        theme.dim_style(),
                    ),
                    Span::styled(format!("(thr {:.2})", a.threshold), theme.dim_style()),
                ])
            })
            .collect(),
        _ => vec![Line::from(Span::styled(
            "  no alerts firing",
            theme.dim_style(),
        ))],
    };
    frame.render_widget(Paragraph::new(firing_lines), chunks[1]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "Recent transitions",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )])),
        chunks[2],
    );

    let now = Instant::now();
    let recent_lines: Vec<Line> = if view.alert_events.is_empty() {
        vec![Line::from(Span::styled("  nothing yet", theme.dim_style()))]
    } else {
        view.alert_events
            .iter()
            .rev()
            .take(15)
            .map(|ev| {
                let kind = match ev.kind {
                    AlertEventKind::Fired => "FIRED",
                    AlertEventKind::Recovered => "ok",
                };
                let kind_style = match ev.kind {
                    AlertEventKind::Fired => severity_style(theme, ev.severity),
                    AlertEventKind::Recovered => Style::default().fg(theme.gauge_low),
                };
                let value_text = if ev.value.is_nan() {
                    format!("{:>10}=—  ", ev.metric)
                } else {
                    format!("{:>10}={:.2} ", ev.metric, ev.value)
                };
                Line::from(vec![
                    Span::styled(format!("  {:<5} ", kind), kind_style),
                    Span::raw(format!("{:<24} ", ev.rule_name)),
                    Span::styled(value_text, theme.dim_style()),
                    Span::styled(format!("({})", elapsed(now, ev.at)), theme.dim_style()),
                ])
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(recent_lines), chunks[3]);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "a or Esc to close",
            theme.dim_style(),
        ))),
        chunks[4],
    );
}

fn severity_label(s: AlertSeverity) -> &'static str {
    match s {
        AlertSeverity::Critical => "CRITICAL",
        AlertSeverity::Warn => "WARN",
        AlertSeverity::Info => "info",
    }
}

fn severity_style(theme: &Theme, s: AlertSeverity) -> Style {
    let color = match s {
        AlertSeverity::Critical => theme.gauge_high,
        AlertSeverity::Warn => theme.gauge_mid,
        AlertSeverity::Info => theme.accent,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn elapsed(now: Instant, at: Instant) -> String {
    let secs = now.saturating_duration_since(at).as_secs();
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m{}s ago", secs / 60, secs % 60)
    } else {
        format!("{}h{}m ago", secs / 3600, (secs % 3600) / 60)
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let v = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(v[1])[1]
}
