use std::collections::BTreeMap;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::{SensorReading, StateView};
use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, view: &StateView, theme: &Theme, focused: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(focused))
        .title(Span::styled(" Sensors ", theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };

    if snap.sensors.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "no sensors discovered",
                theme.dim_style(),
            )])),
            inner,
        );
        return;
    }

    // Group readings by category so categories render contiguously.
    let mut groups: BTreeMap<&str, Vec<&SensorReading>> = BTreeMap::new();
    for r in &snap.sensors {
        groups.entry(r.category.as_str()).or_default().push(r);
    }

    let mut lines: Vec<Line<'_>> = Vec::new();
    for (cat, readings) in &groups {
        lines.push(Line::from(vec![Span::styled(
            category_display(cat).to_string(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )]));
        for r in readings {
            let value_color = colour_for(theme, &r.category, r.value);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<22} ", truncate(&r.name, 22)),
                    theme.dim_style(),
                ),
                Span::styled(format_value(r), Style::default().fg(value_color)),
            ]));
        }
    }

    let para = Paragraph::new(lines);
    let chunks = Layout::vertical([Constraint::Min(0)]).split(inner);
    frame.render_widget(para, chunks[0]);
}

fn category_display(cat: &str) -> &str {
    match cat {
        "temp" => "Temperature",
        "fan" => "Fan",
        "battery" => "Battery",
        other => other,
    }
}

fn format_value(r: &SensorReading) -> String {
    match r.unit {
        "°C" => format!("{:.1}{}", r.value, r.unit),
        "rpm" => format!("{:>5} {}", r.value as i64, r.unit),
        "%" => format!("{:>3.0}{}", r.value, r.unit),
        _ => format!("{:.2} {}", r.value, r.unit),
    }
}

fn colour_for(theme: &Theme, category: &str, value: f64) -> Color {
    match category {
        "temp" => match value {
            v if v >= 90.0 => theme.gauge_high,
            v if v >= 75.0 => theme.gauge_mid,
            _ => theme.gauge_low,
        },
        "battery" => match value {
            v if v <= 15.0 => theme.gauge_high,
            v if v <= 30.0 => theme.gauge_mid,
            _ => theme.gauge_low,
        },
        _ => Color::White,
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
