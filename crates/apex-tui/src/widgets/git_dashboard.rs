use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use git2::Repository;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs};
use ratatui::Frame;

use crate::theme;
use super::Widget;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tab {
    Status,
    Log,
    Branches,
}

struct FileEntry {
    path: String,
    status: String,
}

struct CommitEntry {
    hash: String,
    message: String,
    author: String,
    time: String,
}

pub struct GitDashboardWidget {
    repo_path: String,
    tab: Tab,
    status_files: Vec<FileEntry>,
    commits: Vec<CommitEntry>,
    branches: Vec<String>,
    current_branch: String,
    diff_text: String,
    list_state: ListState,
    error: Option<String>,
}

impl GitDashboardWidget {
    pub fn new(path: Option<String>) -> Self {
        let repo_path = path.unwrap_or_else(|| ".".to_string());
        let mut w = Self {
            repo_path,
            tab: Tab::Status,
            status_files: Vec::new(),
            commits: Vec::new(),
            branches: Vec::new(),
            current_branch: String::new(),
            diff_text: String::new(),
            list_state: ListState::default(),
            error: None,
        };
        w.refresh();
        w.list_state.select(Some(0));
        w
    }

    fn refresh(&mut self) {
        self.error = None;
        let repo = match Repository::discover(&self.repo_path) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(format!("Not a git repo: {}", e));
                return;
            }
        };

        // Branch
        self.current_branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
            .unwrap_or_else(|| "HEAD (detached)".into());

        // Status
        self.status_files.clear();
        if let Ok(statuses) = repo.statuses(None) {
            for entry in statuses.iter() {
                let path = entry.path().unwrap_or("?").to_string();
                let s = entry.status();
                let label = if s.is_index_new() {
                    "A (staged)"
                } else if s.is_index_modified() {
                    "M (staged)"
                } else if s.is_index_deleted() {
                    "D (staged)"
                } else if s.is_wt_modified() {
                    "M"
                } else if s.is_wt_new() {
                    "?"
                } else if s.is_wt_deleted() {
                    "D"
                } else {
                    "?"
                };
                self.status_files.push(FileEntry {
                    path,
                    status: label.to_string(),
                });
            }
        }

        // Log
        self.commits.clear();
        if let Ok(mut revwalk) = repo.revwalk() {
            let _ = revwalk.push_head();
            for oid in revwalk.take(100).flatten() {
                if let Ok(commit) = repo.find_commit(oid) {
                    let time = commit.time();
                    let secs = time.seconds();
                    let naive =
                        chrono_format(secs);
                    self.commits.push(CommitEntry {
                        hash: oid.to_string()[..7].to_string(),
                        message: commit
                            .message()
                            .unwrap_or("")
                            .lines()
                            .next()
                            .unwrap_or("")
                            .to_string(),
                        author: commit.author().name().unwrap_or("?").to_string(),
                        time: naive,
                    });
                }
            }
        }

        // Branches
        self.branches.clear();
        if let Ok(branches) = repo.branches(Some(git2::BranchType::Local)) {
            for branch in branches.flatten() {
                if let Some(name) = branch.0.name().ok().flatten() {
                    self.branches.push(name.to_string());
                }
            }
        }

        // Diff
        self.diff_text.clear();
        let diff_result = repo.diff_index_to_workdir(None, None);
        if let Ok(diff) = diff_result {
            let mut diff_buf = String::new();
            let _ = diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
                if diff_buf.len() < 5000 {
                    let origin = line.origin();
                    if origin == '+' || origin == '-' || origin == ' ' {
                        diff_buf.push(origin);
                    }
                    if let Ok(content) = std::str::from_utf8(line.content()) {
                        diff_buf.push_str(content);
                    }
                }
                true
            });
            self.diff_text = diff_buf;
        }
    }

    fn current_list_len(&self) -> usize {
        match self.tab {
            Tab::Status => self.status_files.len(),
            Tab::Log => self.commits.len(),
            Tab::Branches => self.branches.len(),
        }
    }
}

fn chrono_format(secs: i64) -> String {
    let days = secs / 86400;
    // Simple relative time from epoch seconds
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let diff = now - secs;
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else if diff < 604800 {
        format!("{}d ago", diff / 86400)
    } else {
        // Fall back to Y-M-D
        let d = days;
        let y = 1970 + d / 365;
        format!("{}-{:02}-{:02}", y, (d % 365) / 30 + 1, (d % 30) + 1)
    }
}

impl Widget for GitDashboardWidget {
    fn title(&self) -> &str {
        "Git Dashboard"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        match key.code {
            KeyCode::Tab | KeyCode::Right => {
                self.tab = match self.tab {
                    Tab::Status => Tab::Log,
                    Tab::Log => Tab::Branches,
                    Tab::Branches => Tab::Status,
                };
                self.list_state.select(Some(0));
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.tab = match self.tab {
                    Tab::Status => Tab::Branches,
                    Tab::Log => Tab::Status,
                    Tab::Branches => Tab::Log,
                };
                self.list_state.select(Some(0));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.list_state.selected().unwrap_or(0);
                if i > 0 {
                    self.list_state.select(Some(i - 1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.list_state.selected().unwrap_or(0);
                if i + 1 < self.current_list_len() {
                    self.list_state.select(Some(i + 1));
                }
            }
            KeyCode::Char('r') => {
                self.refresh();
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, data } => {
                match command.as_str() {
                    "refresh" => self.refresh(),
                    "switch_tab" => {
                        if let Some(tab) = data.as_str() {
                            self.tab = match tab {
                                "status" => Tab::Status,
                                "log" => Tab::Log,
                                "branches" => Tab::Branches,
                                _ => self.tab,
                            };
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
        Vec::new()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(ref err) = self.error {
            let p = Paragraph::new(err.as_str())
                .block(Block::default().title(" Git Dashboard ").borders(Borders::ALL).border_style(theme::error_style()));
            frame.render_widget(p, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Tabs
                Constraint::Min(5),   // Content
                Constraint::Length(1), // Status
            ])
            .split(area);

        // Tab bar
        let tab_titles = vec!["Status", "Log", "Branches"];
        let selected = match self.tab {
            Tab::Status => 0,
            Tab::Log => 1,
            Tab::Branches => 2,
        };
        let tabs = Tabs::new(tab_titles)
            .block(Block::default().title(format!(" {} — {} ", self.title(), self.current_branch)).borders(Borders::ALL).border_style(theme::dim_style()).title_style(theme::title_style()))
            .select(selected)
            .style(theme::dim_style())
            .highlight_style(theme::accent_style());
        frame.render_widget(tabs, chunks[0]);

        // Content
        match self.tab {
            Tab::Status => {
                let content_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(chunks[1]);

                let items: Vec<ListItem> = self
                    .status_files
                    .iter()
                    .map(|f| {
                        let style = if f.status.contains("staged") {
                            theme::success_style()
                        } else if f.status == "?" {
                            theme::dim_style()
                        } else {
                            theme::warning_style()
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("{:>10} ", f.status), style),
                            Span::styled(&f.path, ratatui::style::Style::default().fg(theme::FG)),
                        ]))
                    })
                    .collect();

                let list = List::new(items)
                    .block(Block::default().title(" Files ").borders(Borders::ALL).border_style(theme::dim_style()).title_style(theme::title_style()))
                    .highlight_style(theme::selected_style());
                frame.render_stateful_widget(list, content_chunks[0], &mut self.list_state);

                let diff = Paragraph::new(self.diff_text.as_str())
                    .block(Block::default().title(" Diff ").borders(Borders::ALL).border_style(theme::dim_style()).title_style(theme::title_style()));
                frame.render_widget(diff, content_chunks[1]);
            }
            Tab::Log => {
                let items: Vec<ListItem> = self
                    .commits
                    .iter()
                    .map(|c| {
                        ListItem::new(Line::from(vec![
                            Span::styled(&c.hash, theme::accent_style()),
                            Span::styled(" ", theme::dim_style()),
                            Span::styled(&c.message, ratatui::style::Style::default().fg(theme::FG)),
                            Span::styled(format!("  {} — {}", c.author, c.time), theme::dim_style()),
                        ]))
                    })
                    .collect();

                let list = List::new(items)
                    .block(Block::default().title(" Commit Log ").borders(Borders::ALL).border_style(theme::dim_style()).title_style(theme::title_style()))
                    .highlight_style(theme::selected_style());
                frame.render_stateful_widget(list, chunks[1], &mut self.list_state);
            }
            Tab::Branches => {
                let items: Vec<ListItem> = self
                    .branches
                    .iter()
                    .map(|b| {
                        let style = if *b == self.current_branch {
                            theme::success_style()
                        } else {
                            ratatui::style::Style::default().fg(theme::FG)
                        };
                        let prefix = if *b == self.current_branch { "● " } else { "  " };
                        ListItem::new(Span::styled(format!("{}{}", prefix, b), style))
                    })
                    .collect();

                let list = List::new(items)
                    .block(Block::default().title(" Branches ").borders(Borders::ALL).border_style(theme::dim_style()).title_style(theme::title_style()))
                    .highlight_style(theme::selected_style());
                frame.render_stateful_widget(list, chunks[1], &mut self.list_state);
            }
        }

        // Status bar
        let status = Line::from(vec![
            Span::styled(" Tab", theme::accent_style()),
            Span::styled(" switch  ", theme::dim_style()),
            Span::styled("j/k", theme::accent_style()),
            Span::styled(" nav  ", theme::dim_style()),
            Span::styled("r", theme::accent_style()),
            Span::styled(" refresh", theme::dim_style()),
        ]);
        frame.render_widget(Paragraph::new(status), chunks[2]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "status" => {
                let files: Vec<serde_json::Value> = self
                    .status_files
                    .iter()
                    .map(|f| serde_json::json!({"path": f.path, "status": f.status}))
                    .collect();
                serde_json::json!({"branch": self.current_branch, "files": files})
            }
            "log" => {
                let commits: Vec<serde_json::Value> = self
                    .commits
                    .iter()
                    .take(20)
                    .map(|c| serde_json::json!({"hash": c.hash, "message": c.message, "author": c.author}))
                    .collect();
                serde_json::json!({"commits": commits})
            }
            "branches" => {
                serde_json::json!({"current": self.current_branch, "branches": self.branches})
            }
            "diff" => {
                serde_json::json!({"diff": self.diff_text})
            }
            _ => serde_json::json!({"error": format!("Unknown query: {}", query)}),
        }
    }
}
