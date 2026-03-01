use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tokio::sync::mpsc;

use apex_common::TuiToMcp;
use crate::event::AppEvent;
use crate::theme;
use crate::widgets::Widget;

pub struct App {
    pub widget: Box<dyn Widget>,
    pub should_quit: bool,
    pub response_tx: mpsc::UnboundedSender<TuiToMcp>,
}

impl App {
    pub fn new(widget: Box<dyn Widget>, response_tx: mpsc::UnboundedSender<TuiToMcp>) -> Self {
        Self {
            widget,
            should_quit: false,
            response_tx,
        }
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => {
                // Global quit
                if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
                    // Only quit if widget doesn't consume 'q' in a special mode
                    // For now, let widgets handle first, then check
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.should_quit = true;
                    return;
                }

                let responses = self.widget.handle_key(key);
                for resp in responses {
                    let _ = self.response_tx.send(resp);
                }

                // Quit on 'q' at top level
                if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
                    self.should_quit = true;
                }
            }
            AppEvent::Tick => {
                let responses = self.widget.tick();
                for resp in responses {
                    let _ = self.response_tx.send(resp);
                }
            }
            AppEvent::Ipc(msg) => {
                if matches!(msg, apex_common::McpToTui::Shutdown) {
                    self.should_quit = true;
                    return;
                }
                let responses = self.widget.handle_ipc(msg);
                for resp in responses {
                    let _ = self.response_tx.send(resp);
                }
            }
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Title bar
                Constraint::Min(3),   // Widget
                Constraint::Length(1), // Pane switch help
            ])
            .split(area);

        // Title bar
        let title = Line::from(vec![
            Span::styled(" ◆ APEX ", theme::title_style()),
            Span::styled("│ ", theme::dim_style()),
            Span::styled(self.widget.title(), ratatui::style::Style::default().fg(theme::ACCENT2)),
        ]);
        frame.render_widget(Paragraph::new(title), chunks[0]);

        // Widget
        self.widget.render(frame, chunks[1]);

        // Pane switch help bar
        let help = Line::from(vec![
            Span::styled(" Ctrl-a ←/→/↑/↓", theme::accent_style()),
            Span::styled(" switch pane  ", theme::dim_style()),
            Span::styled("q", theme::accent_style()),
            Span::styled(" close widget", theme::dim_style()),
        ]);
        frame.render_widget(
            Paragraph::new(help).style(
                ratatui::style::Style::default()
                    .bg(ratatui::style::Color::Rgb(20, 22, 35)),
            ),
            chunks[2],
        );
    }

    /// Called after terminal.draw() to render overlays like kitty images
    pub fn post_render(&mut self) {
        self.widget.post_render();
    }
}
