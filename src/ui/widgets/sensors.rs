use std::collections::BTreeMap;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::{BatteryReading, BatteryStatus, SensorReading, StateView};
use crate::ui::theme::Theme;

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
        .title(Span::styled(" Sensors ", theme.title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(snap) = view.current else {
        frame.render_widget(Paragraph::new("…").style(theme.dim_style()), inner);
        return;
    };

    if snap.sensors.is_empty() && snap.batteries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "no sensors discovered",
                theme.dim_style(),
            )])),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line<'_>> = Vec::new();

    // Battery section first — most action-relevant on a laptop.
    if !snap.batteries.is_empty() {
        lines.push(section_header("Battery", theme));
        for b in &snap.batteries {
            lines.push(render_battery(b, theme));
        }
    }

    // Then temp / fan grouped by category.
    let mut groups: BTreeMap<&str, Vec<&SensorReading>> = BTreeMap::new();
    for r in &snap.sensors {
        groups.entry(r.category.as_str()).or_default().push(r);
    }
    for (cat, readings) in &groups {
        lines.push(section_header(category_display(cat), theme));
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

fn section_header(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![Span::styled(
        text.to_string(),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )])
}

fn render_battery<'a>(b: &'a BatteryReading, theme: &Theme) -> Line<'a> {
    let pct_color = battery_colour(theme, b.percent);
    let status_label = b.status.label();
    let time = match b.time_remaining_minutes {
        Some(m) => format!(" {}:{:02}", m / 60, m % 60),
        None => String::new(),
    };
    Line::from(vec![
        Span::styled(
            format!("  {:<14} ", truncate(&b.name, 14)),
            theme.dim_style(),
        ),
        Span::styled(
            format!("{:>3.0}%", b.percent),
            Style::default().fg(pct_color),
        ),
        Span::raw("  "),
        Span::styled(status_label.to_string(), status_colour(theme, b.status)),
        Span::raw(time),
    ])
}

fn category_display(cat: &str) -> &str {
    match cat {
        "temp" => "Temperature",
        "fan" => "Fan",
        "power" => "Power",
        other => other,
    }
}

fn format_value(r: &SensorReading) -> String {
    match r.unit {
        "°C" => format!("{:.1}{}", r.value, r.unit),
        "rpm" => format!("{:>5} {}", r.value as i64, r.unit),
        "W" => format!("{:>5.2} {}", r.value, r.unit),
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
        "power" => match value {
            v if v >= 20.0 => theme.gauge_high,
            v if v >= 8.0 => theme.gauge_mid,
            _ => theme.gauge_low,
        },
        _ => Color::White,
    }
}

fn battery_colour(theme: &Theme, percent: f64) -> Color {
    match percent {
        p if p <= 15.0 => theme.gauge_high,
        p if p <= 30.0 => theme.gauge_mid,
        _ => theme.gauge_low,
    }
}

fn status_colour(theme: &Theme, s: BatteryStatus) -> Style {
    match s {
        BatteryStatus::Charging => Style::default().fg(theme.gauge_low),
        BatteryStatus::Discharging => Style::default().fg(theme.gauge_mid),
        BatteryStatus::Full => Style::default().fg(theme.dim),
        BatteryStatus::Unknown => Style::default().fg(theme.dim),
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
