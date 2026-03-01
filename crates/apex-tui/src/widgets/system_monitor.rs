use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table, TableState};
use ratatui::Frame;
use sysinfo::System;

use crate::theme;

use super::Widget;

#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cpu: f32,
    memory: u64,
}

pub struct SystemMonitorWidget {
    sys: System,
    processes: Vec<ProcessInfo>,
    table_state: TableState,
    sort_by: SortColumn,
    sort_asc: bool,
    filter: String,
    filter_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SortColumn {
    Pid,
    Name,
    Cpu,
    Memory,
}

impl SystemMonitorWidget {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let mut w = Self {
            sys,
            processes: Vec::new(),
            table_state: TableState::default(),
            sort_by: SortColumn::Cpu,
            sort_asc: false,
            filter: String::new(),
            filter_mode: false,
        };
        w.refresh_processes();
        w.table_state.select(Some(0));
        w
    }

    fn refresh_processes(&mut self) {
        self.sys.refresh_all();
        self.processes = self
            .sys
            .processes()
            .iter()
            .map(|(pid, p)| ProcessInfo {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu: p.cpu_usage(),
                memory: p.memory(),
            })
            .collect();

        if !self.filter.is_empty() {
            let filter = self.filter.to_lowercase();
            self.processes.retain(|p| p.name.to_lowercase().contains(&filter));
        }

        self.processes.sort_by(|a, b| {
            let ord = match self.sort_by {
                SortColumn::Pid => a.pid.cmp(&b.pid),
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::Cpu => a.cpu.partial_cmp(&b.cpu).unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Memory => a.memory.cmp(&b.memory),
            };
            if self.sort_asc { ord } else { ord.reverse() }
        });
    }

    fn total_cpu(&self) -> f64 {
        let cpus = self.sys.cpus();
        if cpus.is_empty() {
            return 0.0;
        }
        cpus.iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / cpus.len() as f64
    }

    fn format_bytes(bytes: u64) -> String {
        if bytes >= 1_073_741_824 {
            format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
        } else if bytes >= 1_048_576 {
            format!("{:.1} MB", bytes as f64 / 1_048_576.0)
        } else {
            format!("{:.0} KB", bytes as f64 / 1024.0)
        }
    }
}

impl Widget for SystemMonitorWidget {
    fn title(&self) -> &str {
        "System Monitor"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        if self.filter_mode {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.filter_mode = false,
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.refresh_processes();
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.refresh_processes();
                }
                _ => {}
            }
            return Vec::new();
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.table_state.selected().unwrap_or(0);
                if i > 0 {
                    self.table_state.select(Some(i - 1));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.table_state.selected().unwrap_or(0);
                if i + 1 < self.processes.len() {
                    self.table_state.select(Some(i + 1));
                }
            }
            KeyCode::Char('/') => {
                self.filter_mode = true;
            }
            KeyCode::Char('s') => {
                self.sort_by = match self.sort_by {
                    SortColumn::Cpu => SortColumn::Memory,
                    SortColumn::Memory => SortColumn::Name,
                    SortColumn::Name => SortColumn::Pid,
                    SortColumn::Pid => SortColumn::Cpu,
                };
                self.refresh_processes();
            }
            KeyCode::Char('S') => {
                self.sort_asc = !self.sort_asc;
                self.refresh_processes();
            }
            KeyCode::Char('c') => {
                self.filter.clear();
                self.refresh_processes();
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, data } => {
                match command.as_str() {
                    "filter" => {
                        if let Some(f) = data.as_str() {
                            self.filter = f.to_string();
                            self.refresh_processes();
                        }
                    }
                    "sort" => {
                        if let Some(col) = data.as_str() {
                            self.sort_by = match col {
                                "pid" => SortColumn::Pid,
                                "name" => SortColumn::Name,
                                "cpu" => SortColumn::Cpu,
                                "memory" | "mem" => SortColumn::Memory,
                                _ => self.sort_by,
                            };
                            self.refresh_processes();
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
        self.refresh_processes();
        Vec::new()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // CPU gauge
                Constraint::Length(3), // Memory gauge
                Constraint::Min(5),   // Process table
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // CPU gauge
        let cpu = self.total_cpu();
        let cpu_gauge = Gauge::default()
            .block(Block::default().title(" CPU ").borders(Borders::ALL).border_style(theme::dim_style()).title_style(theme::title_style()))
            .gauge_style(theme::bar_style())
            .percent(cpu.min(100.0) as u16)
            .label(format!("{:.1}%", cpu));
        frame.render_widget(cpu_gauge, chunks[0]);

        // Memory gauge
        let total_mem = self.sys.total_memory();
        let used_mem = self.sys.used_memory();
        let mem_pct = if total_mem > 0 { (used_mem as f64 / total_mem as f64) * 100.0 } else { 0.0 };
        let mem_gauge = Gauge::default()
            .block(Block::default().title(" Memory ").borders(Borders::ALL).border_style(theme::dim_style()).title_style(theme::title_style()))
            .gauge_style(theme::bar_style())
            .percent(mem_pct.min(100.0) as u16)
            .label(format!("{} / {} ({:.1}%)", Self::format_bytes(used_mem), Self::format_bytes(total_mem), mem_pct));
        frame.render_widget(mem_gauge, chunks[1]);

        // Process table
        let sort_indicator = |col: SortColumn| -> &str {
            if self.sort_by == col {
                if self.sort_asc { " ▲" } else { " ▼" }
            } else {
                ""
            }
        };

        let header = Row::new(vec![
            Cell::from(format!("PID{}", sort_indicator(SortColumn::Pid))),
            Cell::from(format!("Name{}", sort_indicator(SortColumn::Name))),
            Cell::from(format!("CPU%{}", sort_indicator(SortColumn::Cpu))),
            Cell::from(format!("Memory{}", sort_indicator(SortColumn::Memory))),
        ])
        .style(theme::accent_style());

        let rows: Vec<Row> = self
            .processes
            .iter()
            .map(|p| {
                Row::new(vec![
                    Cell::from(p.pid.to_string()),
                    Cell::from(p.name.clone()),
                    Cell::from(format!("{:.1}", p.cpu)),
                    Cell::from(Self::format_bytes(p.memory)),
                ])
            })
            .collect();

        let table_title = if self.filter.is_empty() {
            format!(" Processes ({}) ", self.processes.len())
        } else {
            format!(" Processes ({}) [filter: {}] ", self.processes.len(), self.filter)
        };

        let table = Table::new(
            rows,
            [
                Constraint::Length(8),
                Constraint::Percentage(45),
                Constraint::Length(10),
                Constraint::Length(12),
            ],
        )
        .header(header)
        .block(Block::default().title(table_title).borders(Borders::ALL).border_style(theme::dim_style()).title_style(theme::title_style()))
        .row_highlight_style(theme::selected_style());

        frame.render_stateful_widget(table, chunks[2], &mut self.table_state);

        // Status bar
        let status = if self.filter_mode {
            Line::from(vec![
                Span::styled(" FILTER: ", theme::accent_style()),
                Span::styled(&self.filter, ratatui::style::Style::default().fg(theme::FG)),
                Span::styled("█", theme::accent_style()),
            ])
        } else {
            Line::from(vec![
                Span::styled(" j/k", theme::accent_style()),
                Span::styled(" nav  ", theme::dim_style()),
                Span::styled("s", theme::accent_style()),
                Span::styled(" sort  ", theme::dim_style()),
                Span::styled("S", theme::accent_style()),
                Span::styled(" reverse  ", theme::dim_style()),
                Span::styled("/", theme::accent_style()),
                Span::styled(" filter  ", theme::dim_style()),
                Span::styled("c", theme::accent_style()),
                Span::styled(" clear", theme::dim_style()),
            ])
        };
        frame.render_widget(Paragraph::new(status), chunks[3]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "cpu_usage" => serde_json::json!({ "cpu_percent": self.total_cpu() }),
            "memory_usage" => {
                serde_json::json!({
                    "total": self.sys.total_memory(),
                    "used": self.sys.used_memory(),
                    "percent": if self.sys.total_memory() > 0 {
                        (self.sys.used_memory() as f64 / self.sys.total_memory() as f64) * 100.0
                    } else { 0.0 }
                })
            }
            "process_list" => {
                let procs: Vec<serde_json::Value> = self
                    .processes
                    .iter()
                    .take(50)
                    .map(|p| {
                        serde_json::json!({
                            "pid": p.pid,
                            "name": p.name,
                            "cpu": p.cpu,
                            "memory": p.memory,
                        })
                    })
                    .collect();
                serde_json::json!({ "processes": procs, "total": self.processes.len() })
            }
            _ => serde_json::json!({ "error": format!("Unknown query: {}", query) }),
        }
    }
}
