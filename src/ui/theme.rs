use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub dim: Color,
    pub gauge_low: Color,
    pub gauge_mid: Color,
    pub gauge_high: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub border: Color,
    pub focus_border: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            accent: Color::Cyan,
            dim: Color::DarkGray,
            gauge_low: Color::Green,
            gauge_mid: Color::Yellow,
            gauge_high: Color::Red,
            selected_bg: Color::Indexed(238),
            selected_fg: Color::White,
            border: Color::DarkGray,
            focus_border: Color::Cyan,
        }
    }

    /// Pick a colour for a 0..100 scale (CPU%, mem%, disk%).
    pub fn gauge_for(&self, percent: f64) -> Color {
        match percent {
            p if p >= 85.0 => self.gauge_high,
            p if p >= 60.0 => self.gauge_mid,
            _ => self.gauge_low,
        }
    }

    pub fn title(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn dim_style(&self) -> Style {
        Style::default().fg(self.dim)
    }

    pub fn border_style(&self, focused: bool) -> Style {
        Style::default().fg(if focused {
            self.focus_border
        } else {
            self.border
        })
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
