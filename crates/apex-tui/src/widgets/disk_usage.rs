use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Gauge, List, ListItem, ListState, Paragraph, Row, Table};
use ratatui::Frame;
use std::process::Command;

use crate::theme;

use super::Widget;

#[derive(Debug, Clone)]
struct MountInfo {
    filesystem: String,
    size: String,
    used: String,
    available: String,
    use_percent: u16,
    mount_point: String,
}

#[derive(Debug, Clone)]
struct DirSize {
    path: String,
    size: String,
}

pub struct DiskUsageWidget {
    mounts: Vec<MountInfo>,
    dir_sizes: Vec<DirSize>,
    list_state: ListState,
    scan_path: String,
}

impl DiskUsageWidget {
    pub fn new() -> Self {
        let mut w = Self {
            mounts: Vec::new(),
            dir_sizes: Vec::new(),
            list_state: ListState::default(),
            scan_path: ".".to_string(),
        };
        w.refresh();
        if !w.mounts.is_empty() {
            w.list_state.select(Some(0));
        }
        w
    }

    fn refresh(&mut self) {
        self.parse_df();
        self.scan_dirs();
    }

    fn parse_df(&mut self) {
        self.mounts.clear();
        let output = match Command::new("df").arg("-h").output() {
            Ok(o) => o,
            Err(_) => return,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                continue;
            }
            // On macOS df -h: Filesystem Size Used Avail Capacity iused ifree %iused Mounted
            // On Linux df -h: Filesystem Size Used Avail Use% Mounted
            // We'll handle both by looking for the percent column
            let mut use_pct = 0u16;
            let mut mount_idx = parts.len() - 1;

            for (i, part) in parts.iter().enumerate() {
                if part.ends_with('%') {
                    if let Ok(p) = part.trim_end_matches('%').parse::<u16>() {
                        use_pct = p;
                        // Mount point is the last field
                        mount_idx = parts.len() - 1;
                        break;
                    }
                }
            }

            // Skip pseudo-filesystems and tiny ones
            let mount_point = parts[mount_idx].to_string();
            let fs = parts[0].to_string();
            if fs.starts_with("devfs")
                || fs.starts_with("map ")
                || mount_point.starts_with("/System/Volumes/xarts")
                || mount_point.starts_with("/System/Volumes/iSCPreboot")
                || mount_point.starts_with("/System/Volumes/Hardware")
            {
                continue;
            }

            let (size, used, avail) = if parts.len() >= 4 {
                (parts[1].to_string(), parts[2].to_string(), parts[3].to_string())
            } else {
                continue;
            };

            self.mounts.push(MountInfo {
                filesystem: fs,
                size,
                used,
                available: avail,
                use_percent: use_pct,
                mount_point,
            });
        }
    }

    fn scan_dirs(&mut self) {
        self.dir_sizes.clear();
        let output = match Command::new("du")
            .args(["-sh", "--"])
            .args(
                // Common top-level dirs to scan
                ["Documents", "Downloads", "Desktop", "Pictures", "Music", "Videos", ".cache", ".local"]
                    .iter()
                    .map(|d| {
                        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                        format!("{}/{}", home, d)
                    })
                    .collect::<Vec<_>>(),
            )
            .output()
        {
            Ok(o) => o,
            Err(_) => return,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Also parse stderr for lines that still have data
        let all_output = format!("{}{}", stdout, stderr);
        for line in all_output.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                // Skip error lines
                if parts[0].contains("cannot") || parts[0].contains("No such") {
                    continue;
                }
                let size = parts[0].to_string();
                let path = parts[1..].join(" ");
                // Only include if we got a valid size
                if size.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    self.dir_sizes.push(DirSize { path, size });
                }
            }
        }
        // Sort by size descending (rough alphabetical on unit is ok for display)
        self.dir_sizes.sort_by(|a, b| {
            let av = Self::parse_size_bytes(&a.size);
            let bv = Self::parse_size_bytes(&b.size);
            bv.cmp(&av)
        });
    }

    fn parse_size_bytes(s: &str) -> u64 {
        let s = s.trim();
        let (num_str, unit) = if s.ends_with('G') || s.ends_with('g') {
            (&s[..s.len() - 1], 1_073_741_824u64)
        } else if s.ends_with('M') || s.ends_with('m') {
            (&s[..s.len() - 1], 1_048_576u64)
        } else if s.ends_with('K') || s.ends_with('k') {
            (&s[..s.len() - 1], 1024u64)
        } else if s.ends_with('T') || s.ends_with('t') {
            (&s[..s.len() - 1], 1_099_511_627_776u64)
        } else if s.ends_with('B') || s.ends_with('b') {
            (&s[..s.len() - 1], 1u64)
        } else {
            (s, 1u64)
        };
        num_str.parse::<f64>().unwrap_or(0.0) as u64 * unit
    }
}

impl Widget for DiskUsageWidget {
    fn title(&self) -> &str {
        "Disk Usage"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let i = self.list_state.selected().unwrap_or(0);
                if !self.mounts.is_empty() && i + 1 < self.mounts.len() {
                    self.list_state.select(Some(i + 1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let i = self.list_state.selected().unwrap_or(0);
                if i > 0 {
                    self.list_state.select(Some(i - 1));
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
                    "refresh" => {
                        self.refresh();
                    }
                    "scan_path" => {
                        if let Some(path) = data.as_str() {
                            self.scan_path = path.to_string();
                            self.refresh();
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
        // Don't refresh every tick - disk I/O is expensive
        Vec::new()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),    // Mount points with gauges
                Constraint::Length(10), // Directory sizes
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Mount points - each gets a gauge
        let mount_constraints: Vec<Constraint> = self
            .mounts
            .iter()
            .map(|_| Constraint::Length(3))
            .collect();

        let mount_block = Block::default()
            .title(format!(" Filesystems ({}) ", self.mounts.len()))
            .borders(Borders::ALL)
            .border_style(theme::dim_style())
            .title_style(theme::title_style());

        let inner = mount_block.inner(chunks[0]);
        frame.render_widget(mount_block, chunks[0]);

        if !self.mounts.is_empty() {
            let gauge_areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints(mount_constraints)
                .split(inner);

            for (i, mount) in self.mounts.iter().enumerate() {
                if i >= gauge_areas.len() {
                    break;
                }
                let gauge_style = if mount.use_percent > 90 {
                    theme::error_style()
                } else if mount.use_percent > 75 {
                    theme::warning_style()
                } else {
                    theme::bar_style()
                };

                let selected = self.list_state.selected() == Some(i);
                let border_style = if selected { theme::accent_style() } else { theme::dim_style() };

                let label = format!(
                    "{} - {} / {} ({}%)",
                    mount.mount_point, mount.used, mount.size, mount.use_percent
                );
                let gauge = Gauge::default()
                    .block(
                        Block::default()
                            .title(format!(" {} ", mount.filesystem))
                            .borders(Borders::ALL)
                            .border_style(border_style)
                            .title_style(if selected { theme::accent_style() } else { theme::dim_style() }),
                    )
                    .gauge_style(gauge_style)
                    .percent(mount.use_percent.min(100))
                    .label(label);
                frame.render_widget(gauge, gauge_areas[i]);
            }
        }

        // Directory sizes
        let dir_items: Vec<ListItem> = self
            .dir_sizes
            .iter()
            .map(|d| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {:>6}", d.size), theme::accent_style()),
                    Span::styled(format!("  {}", d.path), ratatui::style::Style::default().fg(theme::FG)),
                ]))
            })
            .collect();

        let dir_list = List::new(dir_items).block(
            Block::default()
                .title(" Directory Sizes ")
                .borders(Borders::ALL)
                .border_style(theme::dim_style())
                .title_style(theme::title_style()),
        );
        frame.render_widget(dir_list, chunks[1]);

        // Status bar
        let status = Line::from(vec![
            Span::styled(" j/k", theme::accent_style()),
            Span::styled(" nav  ", theme::dim_style()),
            Span::styled("r", theme::accent_style()),
            Span::styled(" refresh", theme::dim_style()),
        ]);
        frame.render_widget(Paragraph::new(status), chunks[2]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "usage" | "mounts" => {
                let mounts: Vec<serde_json::Value> = self
                    .mounts
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "filesystem": m.filesystem,
                            "size": m.size,
                            "used": m.used,
                            "available": m.available,
                            "use_percent": m.use_percent,
                            "mount_point": m.mount_point,
                        })
                    })
                    .collect();
                let dirs: Vec<serde_json::Value> = self
                    .dir_sizes
                    .iter()
                    .map(|d| {
                        serde_json::json!({
                            "path": d.path,
                            "size": d.size,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "mounts": mounts,
                    "directories": dirs,
                })
            }
            _ => serde_json::json!({ "error": format!("Unknown query: {}", query) }),
        }
    }
}
