use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use ignore::WalkBuilder;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::theme;
use super::Widget;

pub struct ProjectOverviewWidget {
    root: PathBuf,
    project_type: String,
    project_name: String,
    file_count: usize,
    line_count: usize,
    extensions: Vec<(String, usize)>,
    dependencies: Vec<String>,
    recent_commits: Vec<String>,
    scroll: usize,
}

impl ProjectOverviewWidget {
    pub fn new(path: Option<String>) -> Self {
        let root = path
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let mut w = Self {
            root,
            project_type: String::new(),
            project_name: String::new(),
            file_count: 0,
            line_count: 0,
            extensions: Vec::new(),
            dependencies: Vec::new(),
            recent_commits: Vec::new(),
            scroll: 0,
        };
        w.analyze();
        w
    }

    fn analyze(&mut self) {
        self.detect_project_type();
        self.count_files();
        self.load_dependencies();
        self.load_recent_commits();
    }

    fn detect_project_type(&mut self) {
        let checks = [
            ("Cargo.toml", "Rust (Cargo)"),
            ("package.json", "JavaScript/TypeScript (npm)"),
            ("go.mod", "Go"),
            ("pyproject.toml", "Python (pyproject)"),
            ("requirements.txt", "Python (pip)"),
            ("Gemfile", "Ruby (Bundler)"),
            ("pom.xml", "Java (Maven)"),
            ("build.gradle", "Java/Kotlin (Gradle)"),
            ("CMakeLists.txt", "C/C++ (CMake)"),
            ("Makefile", "Make"),
        ];

        for (file, ptype) in checks {
            if self.root.join(file).exists() {
                self.project_type = ptype.to_string();
                break;
            }
        }

        if self.project_type.is_empty() {
            self.project_type = "Unknown".to_string();
        }

        // Try to get project name
        if self.root.join("Cargo.toml").exists() {
            if let Ok(content) = std::fs::read_to_string(self.root.join("Cargo.toml")) {
                for line in content.lines() {
                    if let Some(name) = line.strip_prefix("name = ") {
                        self.project_name = name.trim_matches('"').to_string();
                        break;
                    }
                }
            }
        } else if self.root.join("package.json").exists() {
            if let Ok(content) = std::fs::read_to_string(self.root.join("package.json")) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    self.project_name = json["name"].as_str().unwrap_or("").to_string();
                }
            }
        }

        if self.project_name.is_empty() {
            self.project_name = self
                .root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "project".to_string());
        }
    }

    fn count_files(&mut self) {
        let mut file_count = 0usize;
        let mut line_count = 0usize;
        let mut ext_map: HashMap<String, usize> = HashMap::new();

        let walker = WalkBuilder::new(&self.root)
            .hidden(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            file_count += 1;

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("(none)")
                .to_string();
            *ext_map.entry(ext).or_insert(0) += 1;

            // Count lines (only for text files under 1MB)
            if let Ok(meta) = path.metadata() {
                if meta.len() < 1_000_000 {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        line_count += content.lines().count();
                    }
                }
            }
        }

        self.file_count = file_count;
        self.line_count = line_count;

        let mut exts: Vec<(String, usize)> = ext_map.into_iter().collect();
        exts.sort_by(|a, b| b.1.cmp(&a.1));
        self.extensions = exts.into_iter().take(15).collect();
    }

    fn load_dependencies(&mut self) {
        self.dependencies.clear();

        if self.root.join("Cargo.toml").exists() {
            if let Ok(content) = std::fs::read_to_string(self.root.join("Cargo.toml")) {
                let mut in_deps = false;
                for line in content.lines() {
                    if line.starts_with("[dependencies]") || line.starts_with("[workspace.dependencies]") {
                        in_deps = true;
                        continue;
                    }
                    if line.starts_with('[') {
                        in_deps = false;
                        continue;
                    }
                    if in_deps {
                        if let Some(name) = line.split('=').next() {
                            let name = name.trim();
                            if !name.is_empty() {
                                self.dependencies.push(name.to_string());
                            }
                        }
                    }
                }
            }
        } else if self.root.join("package.json").exists() {
            if let Ok(content) = std::fs::read_to_string(self.root.join("package.json")) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(deps) = json["dependencies"].as_object() {
                        for key in deps.keys() {
                            self.dependencies.push(key.clone());
                        }
                    }
                    if let Some(deps) = json["devDependencies"].as_object() {
                        for key in deps.keys() {
                            self.dependencies.push(format!("{} (dev)", key));
                        }
                    }
                }
            }
        }
    }

    fn load_recent_commits(&mut self) {
        self.recent_commits.clear();
        if let Ok(repo) = git2::Repository::discover(&self.root) {
            if let Ok(mut revwalk) = repo.revwalk() {
                let _ = revwalk.push_head();
                for oid in revwalk.take(10).flatten() {
                    if let Ok(commit) = repo.find_commit(oid) {
                        let msg = commit
                            .message()
                            .unwrap_or("")
                            .lines()
                            .next()
                            .unwrap_or("")
                            .to_string();
                        let hash = oid.to_string()[..7].to_string();
                        self.recent_commits.push(format!("{} {}", hash, msg));
                    }
                }
            }
        }
    }
}

impl Widget for ProjectOverviewWidget {
    fn title(&self) -> &str {
        "Project Overview"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        match key.code {
            KeyCode::Char('r') => self.analyze(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, .. } => {
                match command.as_str() {
                    "refresh" => self.analyze(),
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
                Constraint::Length(8),  // Project info
                Constraint::Min(5),    // Details (extensions + deps side by side)
                Constraint::Length(1), // Status
            ])
            .split(area);

        // Project info
        let info_lines = vec![
            Line::from(vec![
                Span::styled("  Project: ", theme::dim_style()),
                Span::styled(&self.project_name, theme::accent_style()),
            ]),
            Line::from(vec![
                Span::styled("     Type: ", theme::dim_style()),
                Span::styled(&self.project_type, ratatui::style::Style::default().fg(theme::FG)),
            ]),
            Line::from(vec![
                Span::styled("    Files: ", theme::dim_style()),
                Span::styled(
                    format_number(self.file_count),
                    ratatui::style::Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(vec![
                Span::styled("    Lines: ", theme::dim_style()),
                Span::styled(
                    format_number(self.line_count),
                    ratatui::style::Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(vec![
                Span::styled("     Deps: ", theme::dim_style()),
                Span::styled(
                    self.dependencies.len().to_string(),
                    ratatui::style::Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(vec![
                Span::styled("     Path: ", theme::dim_style()),
                Span::styled(
                    self.root.display().to_string(),
                    theme::dim_style(),
                ),
            ]),
        ];
        let info = Paragraph::new(info_lines).block(
            Block::default()
                .title(" Project Overview ")
                .borders(Borders::ALL)
                .border_style(theme::dim_style())
                .title_style(theme::title_style()),
        );
        frame.render_widget(info, chunks[0]);

        // Details area: extensions | deps | recent commits
        let detail_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(35),
                Constraint::Percentage(35),
            ])
            .split(chunks[1]);

        // File types
        let ext_items: Vec<ListItem> = self
            .extensions
            .iter()
            .map(|(ext, count)| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  .{:<10}", ext), theme::accent_style()),
                    Span::styled(
                        format_number(*count),
                        ratatui::style::Style::default().fg(theme::FG),
                    ),
                ]))
            })
            .collect();
        let ext_list = List::new(ext_items).block(
            Block::default()
                .title(" File Types ")
                .borders(Borders::ALL)
                .border_style(theme::dim_style())
                .title_style(theme::title_style()),
        );
        frame.render_widget(ext_list, detail_chunks[0]);

        // Dependencies
        let dep_items: Vec<ListItem> = self
            .dependencies
            .iter()
            .take(20)
            .map(|d| ListItem::new(Span::styled(format!("  {}", d), ratatui::style::Style::default().fg(theme::FG))))
            .collect();
        let dep_list = List::new(dep_items).block(
            Block::default()
                .title(" Dependencies ")
                .borders(Borders::ALL)
                .border_style(theme::dim_style())
                .title_style(theme::title_style()),
        );
        frame.render_widget(dep_list, detail_chunks[1]);

        // Recent commits
        let commit_items: Vec<ListItem> = self
            .recent_commits
            .iter()
            .map(|c| {
                let parts: Vec<&str> = c.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {} ", parts[0]), theme::accent_style()),
                        Span::styled(parts[1], ratatui::style::Style::default().fg(theme::FG)),
                    ]))
                } else {
                    ListItem::new(Span::styled(format!("  {}", c), theme::dim_style()))
                }
            })
            .collect();
        let commit_list = List::new(commit_items).block(
            Block::default()
                .title(" Recent Activity ")
                .borders(Borders::ALL)
                .border_style(theme::dim_style())
                .title_style(theme::title_style()),
        );
        frame.render_widget(commit_list, detail_chunks[2]);

        // Status bar
        let status = Line::from(vec![
            Span::styled(" r", theme::accent_style()),
            Span::styled(" refresh", theme::dim_style()),
        ]);
        frame.render_widget(Paragraph::new(status), chunks[2]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "summary" | "overview" => {
                serde_json::json!({
                    "name": self.project_name,
                    "type": self.project_type,
                    "files": self.file_count,
                    "lines": self.line_count,
                    "dependencies": self.dependencies.len(),
                    "top_extensions": self.extensions.iter().take(5).map(|(e, c)| {
                        serde_json::json!({"ext": e, "count": c})
                    }).collect::<Vec<_>>(),
                })
            }
            "dependencies" | "deps" => {
                serde_json::json!({"dependencies": self.dependencies})
            }
            "activity" => {
                serde_json::json!({"recent_commits": self.recent_commits})
            }
            _ => serde_json::json!({"error": format!("Unknown query: {}", query)}),
        }
    }
}

fn format_number(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
