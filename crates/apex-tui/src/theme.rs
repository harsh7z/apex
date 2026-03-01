use ratatui::style::{Color, Modifier, Style};

pub const BG: Color = Color::Rgb(15, 17, 26);
pub const FG: Color = Color::Rgb(200, 210, 230);
pub const ACCENT: Color = Color::Rgb(80, 160, 255);
pub const ACCENT2: Color = Color::Rgb(0, 220, 220);
pub const DIM: Color = Color::Rgb(90, 100, 120);
pub const SUCCESS: Color = Color::Rgb(80, 220, 120);
pub const WARNING: Color = Color::Rgb(255, 200, 60);
pub const ERROR: Color = Color::Rgb(255, 80, 80);
pub const SURFACE: Color = Color::Rgb(25, 28, 40);
pub const HIGHLIGHT: Color = Color::Rgb(35, 40, 60);

pub fn title_style() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn selected_style() -> Style {
    Style::default().bg(HIGHLIGHT).fg(FG)
}

pub fn dim_style() -> Style {
    Style::default().fg(DIM)
}

pub fn accent_style() -> Style {
    Style::default().fg(ACCENT)
}

pub fn success_style() -> Style {
    Style::default().fg(SUCCESS)
}

pub fn warning_style() -> Style {
    Style::default().fg(WARNING)
}

pub fn error_style() -> Style {
    Style::default().fg(ERROR)
}

pub fn bar_style() -> Style {
    Style::default().fg(ACCENT2)
}
