use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use serde::{Deserialize, Serialize};
use std::fs;

use crate::theme;

use super::Widget;

const SAVE_PATH: &str = "/tmp/apex-todos.json";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    fn next(self) -> Self {
        match self {
            Priority::Low => Priority::Medium,
            Priority::Medium => Priority::High,
            Priority::High => Priority::Low,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Priority::Low => "LOW",
            Priority::Medium => "MED",
            Priority::High => "HI!",
        }
    }

    fn style(self) -> ratatui::style::Style {
        match self {
            Priority::Low => theme::dim_style(),
            Priority::Medium => theme::warning_style(),
            Priority::High => theme::error_style(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TodoItem {
    text: String,
    done: bool,
    priority: Priority,
}

pub struct TodoListWidget {
    todos: Vec<TodoItem>,
    list_state: ListState,
    input_mode: bool,
    input_text: String,
}

impl TodoListWidget {
    pub fn new() -> Self {
        let todos = Self::load_todos();
        let mut list_state = ListState::default();
        if !todos.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            todos,
            list_state,
            input_mode: false,
            input_text: String::new(),
        }
    }

    fn load_todos() -> Vec<TodoItem> {
        fs::read_to_string(SAVE_PATH)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_todos(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.todos) {
            let _ = fs::write(SAVE_PATH, json);
        }
    }

    fn add_todo(&mut self, text: String) {
        self.todos.push(TodoItem {
            text,
            done: false,
            priority: Priority::Medium,
        });
        if self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
        self.save_todos();
    }

    fn remove_selected(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i < self.todos.len() {
                self.todos.remove(i);
                if self.todos.is_empty() {
                    self.list_state.select(None);
                } else if i >= self.todos.len() {
                    self.list_state.select(Some(self.todos.len() - 1));
                }
                self.save_todos();
            }
        }
    }

    fn toggle_selected(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i < self.todos.len() {
                self.todos[i].done = !self.todos[i].done;
                self.save_todos();
            }
        }
    }

    fn cycle_priority(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i < self.todos.len() {
                self.todos[i].priority = self.todos[i].priority.next();
                self.save_todos();
            }
        }
    }

    fn clear_done(&mut self) {
        self.todos.retain(|t| !t.done);
        if self.todos.is_empty() {
            self.list_state.select(None);
        } else if let Some(i) = self.list_state.selected() {
            if i >= self.todos.len() {
                self.list_state.select(Some(self.todos.len() - 1));
            }
        }
        self.save_todos();
    }
}

impl Widget for TodoListWidget {
    fn title(&self) -> &str {
        "Todo List"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        if self.input_mode {
            match key.code {
                KeyCode::Enter => {
                    if !self.input_text.is_empty() {
                        let text = self.input_text.clone();
                        self.add_todo(text);
                        self.input_text.clear();
                    }
                    self.input_mode = false;
                }
                KeyCode::Esc => {
                    self.input_text.clear();
                    self.input_mode = false;
                }
                KeyCode::Char(c) => {
                    self.input_text.push(c);
                }
                KeyCode::Backspace => {
                    self.input_text.pop();
                }
                _ => {}
            }
            return Vec::new();
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let i = self.list_state.selected().unwrap_or(0);
                if !self.todos.is_empty() && i + 1 < self.todos.len() {
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
                self.toggle_selected();
            }
            KeyCode::Char('a') => {
                self.input_mode = true;
                self.input_text.clear();
            }
            KeyCode::Char('d') => {
                self.remove_selected();
            }
            KeyCode::Char('p') => {
                self.cycle_priority();
            }
            KeyCode::Char('x') => {
                self.clear_done();
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, data } => {
                match command.as_str() {
                    "add" => {
                        if let Some(text) = data.as_str() {
                            self.add_todo(text.to_string());
                        }
                    }
                    "remove" => {
                        if let Some(idx) = data.as_u64() {
                            let idx = idx as usize;
                            if idx < self.todos.len() {
                                self.todos.remove(idx);
                                if self.todos.is_empty() {
                                    self.list_state.select(None);
                                } else if let Some(sel) = self.list_state.selected() {
                                    if sel >= self.todos.len() {
                                        self.list_state.select(Some(self.todos.len() - 1));
                                    }
                                }
                                self.save_todos();
                            }
                        }
                    }
                    "toggle" => {
                        if let Some(idx) = data.as_u64() {
                            let idx = idx as usize;
                            if idx < self.todos.len() {
                                self.todos[idx].done = !self.todos[idx].done;
                                self.save_todos();
                            }
                        }
                    }
                    "clear_done" => {
                        self.clear_done();
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
        Vec::new()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Stats
                Constraint::Min(5),   // Todo list
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Stats bar
        let total = self.todos.len();
        let done = self.todos.iter().filter(|t| t.done).count();
        let pending = total - done;
        let high = self.todos.iter().filter(|t| !t.done && t.priority == Priority::High).count();

        let stats = vec![Line::from(vec![
            Span::styled("  Total: ", theme::dim_style()),
            Span::styled(format!("{}", total), theme::accent_style()),
            Span::styled("  Done: ", theme::dim_style()),
            Span::styled(format!("{}", done), theme::success_style()),
            Span::styled("  Pending: ", theme::dim_style()),
            Span::styled(format!("{}", pending), theme::warning_style()),
            Span::styled("  High Priority: ", theme::dim_style()),
            Span::styled(format!("{}", high), theme::error_style()),
        ])];
        let stats_block = Block::default()
            .title(" Stats ")
            .borders(Borders::ALL)
            .border_style(theme::dim_style())
            .title_style(theme::title_style());
        frame.render_widget(Paragraph::new(stats).block(stats_block), chunks[0]);

        // Todo list
        let items: Vec<ListItem> = self
            .todos
            .iter()
            .map(|todo| {
                let checkbox = if todo.done { "[x]" } else { "[ ]" };
                let text_style = if todo.done {
                    theme::dim_style()
                } else {
                    ratatui::style::Style::default().fg(theme::FG)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", checkbox), if todo.done { theme::success_style() } else { theme::dim_style() }),
                    Span::styled(format!("[{}] ", todo.priority.label()), todo.priority.style()),
                    Span::styled(&todo.text, text_style),
                ]))
            })
            .collect();

        let list_title = if self.input_mode {
            format!(" Add Todo: {}█ ", self.input_text)
        } else {
            " Todos ".to_string()
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .title(list_title)
                    .borders(Borders::ALL)
                    .border_style(if self.input_mode { theme::accent_style() } else { theme::dim_style() })
                    .title_style(theme::title_style()),
            )
            .highlight_style(theme::selected_style());

        frame.render_stateful_widget(list, chunks[1], &mut self.list_state);

        // Status bar
        let status = if self.input_mode {
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
                Span::styled(" toggle  ", theme::dim_style()),
                Span::styled("a", theme::accent_style()),
                Span::styled(" add  ", theme::dim_style()),
                Span::styled("d", theme::accent_style()),
                Span::styled(" delete  ", theme::dim_style()),
                Span::styled("p", theme::accent_style()),
                Span::styled(" priority  ", theme::dim_style()),
                Span::styled("x", theme::accent_style()),
                Span::styled(" clear done", theme::dim_style()),
            ])
        };
        frame.render_widget(Paragraph::new(status), chunks[2]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "list" => {
                let items: Vec<serde_json::Value> = self
                    .todos
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        serde_json::json!({
                            "index": i,
                            "text": t.text,
                            "done": t.done,
                            "priority": format!("{:?}", t.priority),
                        })
                    })
                    .collect();
                serde_json::json!({ "todos": items })
            }
            "stats" => {
                let total = self.todos.len();
                let done = self.todos.iter().filter(|t| t.done).count();
                serde_json::json!({
                    "total": total,
                    "done": done,
                    "pending": total - done,
                    "high_priority": self.todos.iter().filter(|t| !t.done && t.priority == Priority::High).count(),
                })
            }
            _ => serde_json::json!({ "error": format!("Unknown query: {}", query) }),
        }
    }
}
