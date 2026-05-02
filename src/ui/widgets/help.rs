use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::ui::theme::Theme;

pub fn render(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    let popup = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(" Help ", theme.title()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = [
        ("Tab", "cycle focus between panels"),
        ("↑ / ↓", "select row in Process panel"),
        ("c / m / p / n", "sort processes by CPU / Mem / PID / Name"),
        ("/", "filter processes (Esc to cancel)"),
        ("F9 / k", "ask to kill selected process; Enter to confirm"),
        ("?", "toggle this help"),
        ("q / Ctrl-C", "quit"),
    ];

    let body: Vec<Line> = lines
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(
                    format!("{k:>14} "),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::raw(*d),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(body), inner);
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
