use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::time::Instant;

use crate::theme;

use super::Widget;

#[derive(Debug, Clone, Copy, PartialEq)]
enum TimerMode {
    Stopwatch,
    Timer,
    Pomodoro,
}

impl TimerMode {
    fn label(self) -> &'static str {
        match self {
            TimerMode::Stopwatch => "Stopwatch",
            TimerMode::Timer => "Timer",
            TimerMode::Pomodoro => "Pomodoro",
        }
    }

    fn next(self) -> Self {
        match self {
            TimerMode::Stopwatch => TimerMode::Timer,
            TimerMode::Timer => TimerMode::Pomodoro,
            TimerMode::Pomodoro => TimerMode::Stopwatch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PomodoroPhase {
    Work,
    Break,
}

pub struct TimerWidget {
    mode: TimerMode,
    running: bool,
    // Elapsed time in seconds (accumulated)
    elapsed_secs: f64,
    // When the timer was last started
    start_instant: Option<Instant>,
    // Timer mode: target duration in seconds
    timer_duration: u64,
    // Pomodoro
    pomodoro_phase: PomodoroPhase,
    pomodoro_work_secs: u64,
    pomodoro_break_secs: u64,
    pomodoro_cycles: u32,
}

impl TimerWidget {
    pub fn new() -> Self {
        Self {
            mode: TimerMode::Stopwatch,
            running: false,
            elapsed_secs: 0.0,
            start_instant: None,
            timer_duration: 300, // 5 minutes default
            pomodoro_phase: PomodoroPhase::Work,
            pomodoro_work_secs: 25 * 60,
            pomodoro_break_secs: 5 * 60,
            pomodoro_cycles: 0,
        }
    }

    fn current_elapsed(&self) -> f64 {
        let base = self.elapsed_secs;
        if let Some(start) = self.start_instant {
            base + start.elapsed().as_secs_f64()
        } else {
            base
        }
    }

    fn toggle(&mut self) {
        if self.running {
            // Pause: accumulate elapsed
            if let Some(start) = self.start_instant.take() {
                self.elapsed_secs += start.elapsed().as_secs_f64();
            }
            self.running = false;
        } else {
            // Start
            self.start_instant = Some(Instant::now());
            self.running = true;
        }
    }

    fn reset(&mut self) {
        self.running = false;
        self.elapsed_secs = 0.0;
        self.start_instant = None;
        self.pomodoro_phase = PomodoroPhase::Work;
        self.pomodoro_cycles = 0;
    }

    fn switch_mode(&mut self) {
        self.reset();
        self.mode = self.mode.next();
    }

    fn format_time(secs: f64) -> (u32, u32, u32) {
        let total = secs.max(0.0) as u32;
        let h = total / 3600;
        let m = (total % 3600) / 60;
        let s = total % 60;
        (h, m, s)
    }

    fn big_digit(digit: u32) -> [&'static str; 5] {
        match digit {
            0 => [" ██ ", "█  █", "█  █", "█  █", " ██ "],
            1 => ["  █ ", " ██ ", "  █ ", "  █ ", " ███"],
            2 => [" ██ ", "█  █", "  █ ", " █  ", "████"],
            3 => ["███ ", "   █", " ██ ", "   █", "███ "],
            4 => ["█  █", "█  █", "████", "   █", "   █"],
            5 => ["████", "█   ", "███ ", "   █", "███ "],
            6 => [" ██ ", "█   ", "███ ", "█  █", " ██ "],
            7 => ["████", "   █", "  █ ", " █  ", " █  "],
            8 => [" ██ ", "█  █", " ██ ", "█  █", " ██ "],
            9 => [" ██ ", "█  █", " ███", "   █", " ██ "],
            _ => ["    ", "    ", "    ", "    ", "    "],
        }
    }

    fn colon_art() -> [&'static str; 5] {
        ["  ", "██", "  ", "██", "  "]
    }

    fn render_big_time(&self, secs: f64) -> Vec<Line<'static>> {
        let (h, m, s) = Self::format_time(secs);

        let digits: Vec<[&str; 5]> = if h > 0 {
            vec![
                Self::big_digit(h / 10),
                Self::big_digit(h % 10),
                Self::colon_art(),
                Self::big_digit(m / 10),
                Self::big_digit(m % 10),
                Self::colon_art(),
                Self::big_digit(s / 10),
                Self::big_digit(s % 10),
            ]
        } else {
            vec![
                Self::big_digit(m / 10),
                Self::big_digit(m % 10),
                Self::colon_art(),
                Self::big_digit(s / 10),
                Self::big_digit(s % 10),
            ]
        };

        let mut lines = Vec::new();
        for row in 0..5 {
            let mut spans = vec![Span::styled("  ", theme::dim_style())];
            for (i, digit) in digits.iter().enumerate() {
                let style = if self.running {
                    theme::accent_style()
                } else {
                    theme::dim_style()
                };
                spans.push(Span::styled(digit[row].to_string(), style));
                if i < digits.len() - 1 {
                    spans.push(Span::styled(" ", theme::dim_style()));
                }
            }
            lines.push(Line::from(spans));
        }
        lines
    }

    fn display_seconds(&self) -> f64 {
        let elapsed = self.current_elapsed();
        match self.mode {
            TimerMode::Stopwatch => elapsed,
            TimerMode::Timer => {
                let remaining = self.timer_duration as f64 - elapsed;
                remaining.max(0.0)
            }
            TimerMode::Pomodoro => {
                let target = match self.pomodoro_phase {
                    PomodoroPhase::Work => self.pomodoro_work_secs,
                    PomodoroPhase::Break => self.pomodoro_break_secs,
                };
                let remaining = target as f64 - elapsed;
                remaining.max(0.0)
            }
        }
    }

    fn check_timer_complete(&mut self) {
        let elapsed = self.current_elapsed();
        match self.mode {
            TimerMode::Timer => {
                if elapsed >= self.timer_duration as f64 {
                    self.running = false;
                    self.start_instant = None;
                    self.elapsed_secs = self.timer_duration as f64;
                }
            }
            TimerMode::Pomodoro => {
                let target = match self.pomodoro_phase {
                    PomodoroPhase::Work => self.pomodoro_work_secs,
                    PomodoroPhase::Break => self.pomodoro_break_secs,
                };
                if elapsed >= target as f64 {
                    // Switch phase
                    match self.pomodoro_phase {
                        PomodoroPhase::Work => {
                            self.pomodoro_phase = PomodoroPhase::Break;
                            self.pomodoro_cycles += 1;
                        }
                        PomodoroPhase::Break => {
                            self.pomodoro_phase = PomodoroPhase::Work;
                        }
                    }
                    self.elapsed_secs = 0.0;
                    self.start_instant = Some(Instant::now());
                }
            }
            _ => {}
        }
    }
}

impl Widget for TimerWidget {
    fn title(&self) -> &str {
        "Timer"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        match key.code {
            KeyCode::Char(' ') => {
                self.toggle();
            }
            KeyCode::Char('r') => {
                self.reset();
            }
            KeyCode::Char('m') => {
                self.switch_mode();
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if !self.running {
                    match self.mode {
                        TimerMode::Timer => {
                            self.timer_duration = (self.timer_duration + 60).min(7200);
                        }
                        TimerMode::Pomodoro => {
                            if self.pomodoro_phase == PomodoroPhase::Work {
                                self.pomodoro_work_secs = (self.pomodoro_work_secs + 60).min(3600);
                            } else {
                                self.pomodoro_break_secs = (self.pomodoro_break_secs + 60).min(1800);
                            }
                        }
                        _ => {}
                    }
                }
            }
            KeyCode::Char('-') => {
                if !self.running {
                    match self.mode {
                        TimerMode::Timer => {
                            self.timer_duration = self.timer_duration.saturating_sub(60).max(60);
                        }
                        TimerMode::Pomodoro => {
                            if self.pomodoro_phase == PomodoroPhase::Work {
                                self.pomodoro_work_secs = self.pomodoro_work_secs.saturating_sub(60).max(60);
                            } else {
                                self.pomodoro_break_secs = self.pomodoro_break_secs.saturating_sub(60).max(60);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, data } => {
                match command.as_str() {
                    "start" => {
                        if !self.running {
                            self.toggle();
                        }
                    }
                    "stop" => {
                        if self.running {
                            self.toggle();
                        }
                    }
                    "reset" => {
                        self.reset();
                    }
                    "set_mode" => {
                        if let Some(mode) = data.as_str() {
                            self.reset();
                            self.mode = match mode {
                                "stopwatch" => TimerMode::Stopwatch,
                                "timer" => TimerMode::Timer,
                                "pomodoro" => TimerMode::Pomodoro,
                                _ => self.mode,
                            };
                        }
                    }
                    "set_duration" => {
                        if let Some(secs) = data.as_u64() {
                            self.timer_duration = secs.max(60).min(7200);
                        }
                    }
                    _ => {
                        return vec![TuiToMcp::Ack {
                            success: false,
                            error: Some(format!("Unknown command: {}", command)),
                        }];
                    }
                }
                vec![TuiToMcp::Ack { success: true, error: None }]
            }
            McpToTui::Query { request_id, query } => {
                let data = self.query(&query);
                vec![TuiToMcp::QueryResponse { request_id, data }]
            }
            McpToTui::Shutdown => Vec::new(),
        }
    }

    fn tick(&mut self) -> Vec<TuiToMcp> {
        if self.running {
            self.check_timer_complete();
        }
        Vec::new()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Mode / Info
                Constraint::Length(7),  // Big time display
                Constraint::Min(3),    // Details
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Mode info
        let mode_text = match self.mode {
            TimerMode::Stopwatch => {
                vec![Line::from(vec![
                    Span::styled("  Mode: ", theme::dim_style()),
                    Span::styled("Stopwatch", theme::accent_style()),
                    Span::styled(if self.running { "  [RUNNING]" } else { "  [PAUSED]" },
                        if self.running { theme::success_style() } else { theme::warning_style() }),
                ])]
            }
            TimerMode::Timer => {
                let (_, m, s) = Self::format_time(self.timer_duration as f64);
                vec![Line::from(vec![
                    Span::styled("  Mode: ", theme::dim_style()),
                    Span::styled("Timer", theme::accent_style()),
                    Span::styled(format!("  Duration: {:02}:{:02}", m, s), theme::dim_style()),
                    Span::styled(if self.running { "  [RUNNING]" } else { "  [PAUSED]" },
                        if self.running { theme::success_style() } else { theme::warning_style() }),
                ])]
            }
            TimerMode::Pomodoro => {
                let phase_str = match self.pomodoro_phase {
                    PomodoroPhase::Work => "WORK",
                    PomodoroPhase::Break => "BREAK",
                };
                vec![Line::from(vec![
                    Span::styled("  Mode: ", theme::dim_style()),
                    Span::styled("Pomodoro", theme::accent_style()),
                    Span::styled(format!("  Phase: {}", phase_str),
                        match self.pomodoro_phase {
                            PomodoroPhase::Work => theme::error_style(),
                            PomodoroPhase::Break => theme::success_style(),
                        }),
                    Span::styled(format!("  Cycles: {}", self.pomodoro_cycles), theme::dim_style()),
                ])]
            }
        };
        let mode_block = Block::default()
            .title(format!(" {} ", self.mode.label()))
            .borders(Borders::ALL)
            .border_style(theme::dim_style())
            .title_style(theme::title_style());
        frame.render_widget(Paragraph::new(mode_text).block(mode_block), chunks[0]);

        // Big time display
        let display_secs = self.display_seconds();
        let big_lines = self.render_big_time(display_secs);
        let time_block = Block::default()
            .title(" Time ")
            .borders(Borders::ALL)
            .border_style(if self.running { theme::accent_style() } else { theme::dim_style() })
            .title_style(theme::title_style());
        frame.render_widget(Paragraph::new(big_lines).block(time_block), chunks[1]);

        // Details
        let (h, m, s) = Self::format_time(display_secs);
        let detail_lines = vec![
            Line::from(vec![
                Span::styled(format!("  {:02}:{:02}:{:02}", h, m, s), ratatui::style::Style::default().fg(theme::FG)),
            ]),
            Line::from(""),
            match self.mode {
                TimerMode::Pomodoro => Line::from(vec![
                    Span::styled(format!("  Work: {}m  Break: {}m",
                        self.pomodoro_work_secs / 60, self.pomodoro_break_secs / 60), theme::dim_style()),
                ]),
                TimerMode::Timer => Line::from(vec![
                    Span::styled(format!("  Target: {}m {}s",
                        self.timer_duration / 60, self.timer_duration % 60), theme::dim_style()),
                ]),
                _ => Line::from(""),
            },
        ];
        let detail_block = Block::default()
            .title(" Details ")
            .borders(Borders::ALL)
            .border_style(theme::dim_style())
            .title_style(theme::title_style());
        frame.render_widget(Paragraph::new(detail_lines).block(detail_block), chunks[2]);

        // Status bar
        let status = Line::from(vec![
            Span::styled(" Space", theme::accent_style()),
            Span::styled(" start/stop  ", theme::dim_style()),
            Span::styled("r", theme::accent_style()),
            Span::styled(" reset  ", theme::dim_style()),
            Span::styled("m", theme::accent_style()),
            Span::styled(" mode  ", theme::dim_style()),
            Span::styled("+/-", theme::accent_style()),
            Span::styled(" adjust", theme::dim_style()),
        ]);
        frame.render_widget(Paragraph::new(status), chunks[3]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "status" => {
                serde_json::json!({
                    "mode": self.mode.label(),
                    "running": self.running,
                    "display_seconds": self.display_seconds(),
                    "pomodoro_phase": match self.pomodoro_phase {
                        PomodoroPhase::Work => "work",
                        PomodoroPhase::Break => "break",
                    },
                    "pomodoro_cycles": self.pomodoro_cycles,
                })
            }
            "elapsed" => {
                let elapsed = self.current_elapsed();
                let (h, m, s) = Self::format_time(elapsed);
                serde_json::json!({
                    "elapsed_seconds": elapsed,
                    "formatted": format!("{:02}:{:02}:{:02}", h, m, s),
                })
            }
            _ => serde_json::json!({ "error": format!("Unknown query: {}", query) }),
        }
    }
}
