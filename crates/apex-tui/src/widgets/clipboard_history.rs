use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::process::Command;
use std::time::SystemTime;

use crate::theme;

use super::Widget;

const MAX_ENTRIES: usize = 50;

#[derive(Debug, Clone)]
struct ClipboardEntry {
    content: String,
    timestamp: String,
    preview: String,
}

impl ClipboardEntry {
    fn new(content: String) -> Self {
        let timestamp = Self::now_string();
        let preview = Self::make_preview(&content);
        Self {
            content,
            timestamp,
            preview,
        }
    }

    fn now_string() -> String {
        let duration = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = duration.as_secs();
        // Simple HH:MM:SS from epoch seconds (local-ish, good enough for display)
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    }

    fn make_preview(content: &str) -> String {
        let line = content.lines().next().unwrap_or("");
        if line.len() > 60 {
            format!("{}...", &line[..57])
        } else if content.lines().count() > 1 {
            format!("{} (+{} lines)", line, content.lines().count() - 1)
        } else {
            line.to_string()
        }
    }
}

pub struct ClipboardHistoryWidget {
    entries: Vec<ClipboardEntry>,
    list_state: ListState,
    last_clipboard: String,
    filter: String,
    filter_mode: bool,
}

impl ClipboardHistoryWidget {
    pub fn new() -> Self {
        let mut w = Self {
            entries: Vec::new(),
            list_state: ListState::default(),
            last_clipboard: String::new(),
            filter: String::new(),
            filter_mode: false,
        };
        // Capture initial clipboard content
        if let Some(content) = w.read_clipboard() {
            w.last_clipboard = content.clone();
            w.entries.push(ClipboardEntry::new(content));
            w.list_state.select(Some(0));
        }
        w
    }

    fn read_clipboard(&self) -> Option<String> {
        Command::new("pbpaste")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let s = String::from_utf8_lossy(&o.stdout).to_string();
                    if s.is_empty() { None } else { Some(s) }
                } else {
                    None
                }
            })
    }

    fn write_clipboard(content: &str) {
        if let Ok(mut child) = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(ref mut stdin) = child.stdin {
                use std::io::Write;
                let _ = stdin.write_all(content.as_bytes());
            }
            let _ = child.wait();
        }
    }

    fn poll_clipboard(&mut self) {
        if let Some(content) = self.read_clipboard() {
            if content != self.last_clipboard {
                self.last_clipboard = content.clone();
                self.entries.insert(0, ClipboardEntry::new(content));
                if self.entries.len() > MAX_ENTRIES {
                    self.entries.pop();
                }
                self.list_state.select(Some(0));
            }
        }
    }

    fn filtered_indices(&self) -> Vec<usize> {
        if self.filter.is_empty() {
            return (0..self.entries.len()).collect();
        }
        let filter_lower = self.filter.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.content.to_lowercase().contains(&filter_lower))
            .map(|(i, _)| i)
            .collect()
    }

    fn copy_selected(&mut self) {
        let indices = self.filtered_indices();
        if let Some(sel) = self.list_state.selected() {
            if sel < indices.len() {
                let idx = indices[sel];
                let content = self.entries[idx].content.clone();
                Self::write_clipboard(&content);
                self.last_clipboard = content;
            }
        }
    }

    fn delete_selected(&mut self) {
        let indices = self.filtered_indices();
        if let Some(sel) = self.list_state.selected() {
            if sel < indices.len() {
                let idx = indices[sel];
                self.entries.remove(idx);
                let new_indices = self.filtered_indices();
                if new_indices.is_empty() {
                    self.list_state.select(None);
                } else if sel >= new_indices.len() {
                    self.list_state.select(Some(new_indices.len() - 1));
                }
            }
        }
    }
}

impl Widget for ClipboardHistoryWidget {
    fn title(&self) -> &str {
        "Clipboard History"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        if self.filter_mode {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.filter_mode = false;
                    let indices = self.filtered_indices();
                    if !indices.is_empty() {
                        self.list_state.select(Some(0));
                    } else {
                        self.list_state.select(None);
                    }
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                _ => {}
            }
            return Vec::new();
        }

        let filtered_len = self.filtered_indices().len();

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let i = self.list_state.selected().unwrap_or(0);
                if filtered_len > 0 && i + 1 < filtered_len {
                    self.list_state.select(Some(i + 1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let i = self.list_state.selected().unwrap_or(0);
                if i > 0 {
                    self.list_state.select(Some(i - 1));
                }
            }
            KeyCode::Enter => {
                self.copy_selected();
            }
            KeyCode::Char('d') => {
                self.delete_selected();
            }
            KeyCode::Char('/') => {
                self.filter_mode = true;
                self.filter.clear();
            }
            KeyCode::Char('c') => {
                self.filter.clear();
                if !self.entries.is_empty() {
                    self.list_state.select(Some(0));
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, data: _ } => {
                match command.as_str() {
                    "clear" => {
                        self.entries.clear();
                        self.list_state.select(None);
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
        self.poll_clipboard();
        Vec::new()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Current clipboard preview
                Constraint::Min(5),   // History list
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Current clipboard
        let current_preview = if self.last_clipboard.is_empty() {
            "(empty)".to_string()
        } else {
            ClipboardEntry::make_preview(&self.last_clipboard)
        };
        let current_text = vec![Line::from(vec![
            Span::styled("  ", theme::dim_style()),
            Span::styled(current_preview, ratatui::style::Style::default().fg(theme::FG)),
        ])];
        let current_block = Block::default()
            .title(" Current Clipboard ")
            .borders(Borders::ALL)
            .border_style(theme::accent_style())
            .title_style(theme::title_style());
        frame.render_widget(Paragraph::new(current_text).block(current_block), chunks[0]);

        // History list
        let indices = self.filtered_indices();
        let items: Vec<ListItem> = indices
            .iter()
            .map(|&i| {
                let entry = &self.entries[i];
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", entry.timestamp), theme::dim_style()),
                    Span::styled(&entry.preview, ratatui::style::Style::default().fg(theme::FG)),
                ]))
            })
            .collect();

        let list_title = if self.filter_mode {
            format!(" History ({}) [/{}█] ", indices.len(), self.filter)
        } else if !self.filter.is_empty() {
            format!(" History ({}) [filter: {}] ", indices.len(), self.filter)
        } else {
            format!(" History ({}) ", self.entries.len())
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .title(list_title)
                    .borders(Borders::ALL)
                    .border_style(if self.filter_mode { theme::accent_style() } else { theme::dim_style() })
                    .title_style(theme::title_style()),
            )
            .highlight_style(theme::selected_style());

        frame.render_stateful_widget(list, chunks[1], &mut self.list_state);

        // Status bar
        let status = if self.filter_mode {
            Line::from(vec![
                Span::styled(" Enter", theme::accent_style()),
                Span::styled(" confirm  ", theme::dim_style()),
                Span::styled("Esc", theme::accent_style()),
                Span::styled(" cancel", theme::dim_style()),
            ])
        } else {
            Line::from(vec![
                Span::styled(" j/k", theme::accent_style()),
                Span::styled(" nav  ", theme::dim_style()),
                Span::styled("Enter", theme::accent_style()),
                Span::styled(" copy  ", theme::dim_style()),
                Span::styled("d", theme::accent_style()),
                Span::styled(" delete  ", theme::dim_style()),
                Span::styled("/", theme::accent_style()),
                Span::styled(" filter  ", theme::dim_style()),
                Span::styled("c", theme::accent_style()),
                Span::styled(" clear filter", theme::dim_style()),
            ])
        };
        frame.render_widget(Paragraph::new(status), chunks[2]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "list" => {
                let items: Vec<serde_json::Value> = self
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        serde_json::json!({
                            "index": i,
                            "timestamp": e.timestamp,
                            "preview": e.preview,
                            "length": e.content.len(),
                        })
                    })
                    .collect();
                serde_json::json!({ "entries": items, "total": self.entries.len() })
            }
            "current" => {
                serde_json::json!({
                    "content": self.last_clipboard,
                    "length": self.last_clipboard.len(),
                })
            }
            _ => serde_json::json!({ "error": format!("Unknown query: {}", query) }),
        }
    }
}
