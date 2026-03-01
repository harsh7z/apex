use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::theme;

use super::Widget;

#[derive(Debug, Clone)]
struct DayForecast {
    day: &'static str,
    high: i32,
    low: i32,
    condition: &'static str,
    icon: &'static str,
}

#[derive(Debug, Clone)]
struct WeatherData {
    city: String,
    temp: i32,
    feels_like: i32,
    humidity: u32,
    wind_speed: u32,
    condition: String,
    icon: &'static str,
    forecast: Vec<DayForecast>,
}

impl WeatherData {
    fn for_city(city: &str) -> Self {
        // Generate deterministic mock data based on city name length
        let seed = city.len() as i32;
        let base_temp = 18 + (seed % 15);
        let conditions = [
            ("Sunny", "☀"),
            ("Partly Cloudy", "⛅"),
            ("Cloudy", "☁"),
            ("Rainy", "🌧"),
            ("Thunderstorm", "⛈"),
        ];
        let idx = (seed as usize) % conditions.len();

        Self {
            city: city.to_string(),
            temp: base_temp,
            feels_like: base_temp - 2,
            humidity: 40 + (seed as u32 * 3) % 50,
            wind_speed: 5 + (seed as u32 * 2) % 25,
            condition: conditions[idx].0.to_string(),
            icon: conditions[idx].1,
            forecast: vec![
                DayForecast { day: "Mon", high: base_temp + 2, low: base_temp - 5, condition: "Sunny", icon: "☀" },
                DayForecast { day: "Tue", high: base_temp + 1, low: base_temp - 6, condition: "Cloudy", icon: "☁" },
                DayForecast { day: "Wed", high: base_temp - 1, low: base_temp - 8, condition: "Rainy", icon: "🌧" },
                DayForecast { day: "Thu", high: base_temp + 3, low: base_temp - 4, condition: "Sunny", icon: "☀" },
                DayForecast { day: "Fri", high: base_temp, low: base_temp - 7, condition: "P.Cloudy", icon: "⛅" },
            ],
        }
    }
}

pub struct WeatherWidget {
    data: WeatherData,
    input_mode: bool,
    city_input: String,
}

impl WeatherWidget {
    pub fn new(city: Option<String>) -> Self {
        let city_name = city.unwrap_or_else(|| "San Francisco".to_string());
        Self {
            data: WeatherData::for_city(&city_name),
            input_mode: false,
            city_input: String::new(),
        }
    }

    fn set_city(&mut self, city: &str) {
        self.data = WeatherData::for_city(city);
    }
}

impl Widget for WeatherWidget {
    fn title(&self) -> &str {
        "Weather"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        if self.input_mode {
            match key.code {
                KeyCode::Enter => {
                    if !self.city_input.is_empty() {
                        let city = self.city_input.clone();
                        self.set_city(&city);
                        self.city_input.clear();
                    }
                    self.input_mode = false;
                }
                KeyCode::Esc => {
                    self.city_input.clear();
                    self.input_mode = false;
                }
                KeyCode::Char(c) => {
                    self.city_input.push(c);
                }
                KeyCode::Backspace => {
                    self.city_input.pop();
                }
                _ => {}
            }
            return Vec::new();
        }

        match key.code {
            KeyCode::Char('c') => {
                self.input_mode = true;
                self.city_input.clear();
            }
            KeyCode::Char('r') => {
                // Refresh with same city (re-generate mock data)
                let city = self.data.city.clone();
                self.set_city(&city);
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, data } => {
                match command.as_str() {
                    "set_city" => {
                        if let Some(city) = data.as_str() {
                            self.set_city(city);
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
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // Current weather
                Constraint::Min(5),    // Forecast table
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Current weather display
        let current_text = vec![
            Line::from(vec![
                Span::styled(
                    format!("  {} ", self.data.icon),
                    theme::accent_style(),
                ),
                Span::styled(
                    format!("{}°C", self.data.temp),
                    ratatui::style::Style::default()
                        .fg(theme::FG)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Condition: ", theme::dim_style()),
                Span::styled(&self.data.condition, ratatui::style::Style::default().fg(theme::FG)),
            ]),
            Line::from(vec![
                Span::styled("  Feels Like: ", theme::dim_style()),
                Span::styled(format!("{}°C", self.data.feels_like), ratatui::style::Style::default().fg(theme::FG)),
            ]),
            Line::from(vec![
                Span::styled("  Humidity: ", theme::dim_style()),
                Span::styled(format!("{}%", self.data.humidity), ratatui::style::Style::default().fg(theme::FG)),
            ]),
            Line::from(vec![
                Span::styled("  Wind: ", theme::dim_style()),
                Span::styled(format!("{} km/h", self.data.wind_speed), ratatui::style::Style::default().fg(theme::FG)),
            ]),
        ];

        let current_block = Block::default()
            .title(format!(" {} - Current Weather ", self.data.city))
            .borders(Borders::ALL)
            .border_style(theme::dim_style())
            .title_style(theme::title_style());
        frame.render_widget(Paragraph::new(current_text).block(current_block), chunks[0]);

        // 5-day forecast table
        let header = Row::new(vec![
            Cell::from("Day"),
            Cell::from(""),
            Cell::from("Condition"),
            Cell::from("High"),
            Cell::from("Low"),
        ])
        .style(theme::accent_style());

        let rows: Vec<Row> = self
            .data
            .forecast
            .iter()
            .map(|d| {
                Row::new(vec![
                    Cell::from(d.day),
                    Cell::from(d.icon),
                    Cell::from(d.condition),
                    Cell::from(format!("{}°C", d.high)),
                    Cell::from(format!("{}°C", d.low)),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(5),
                Constraint::Length(4),
                Constraint::Percentage(35),
                Constraint::Length(8),
                Constraint::Length(8),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .title(" 5-Day Forecast ")
                .borders(Borders::ALL)
                .border_style(theme::dim_style())
                .title_style(theme::title_style()),
        );
        frame.render_widget(table, chunks[1]);

        // Status bar
        let status = if self.input_mode {
            Line::from(vec![
                Span::styled(" City: ", theme::accent_style()),
                Span::styled(&self.city_input, ratatui::style::Style::default().fg(theme::FG)),
                Span::styled("█", theme::accent_style()),
            ])
        } else {
            Line::from(vec![
                Span::styled(" c", theme::accent_style()),
                Span::styled(" change city  ", theme::dim_style()),
                Span::styled("r", theme::accent_style()),
                Span::styled(" refresh", theme::dim_style()),
            ])
        };
        frame.render_widget(Paragraph::new(status), chunks[2]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "current" => serde_json::json!({
                "city": self.data.city,
                "temp": self.data.temp,
                "feels_like": self.data.feels_like,
                "humidity": self.data.humidity,
                "wind_speed": self.data.wind_speed,
                "condition": self.data.condition,
            }),
            "forecast" => {
                let days: Vec<serde_json::Value> = self.data.forecast.iter().map(|d| {
                    serde_json::json!({
                        "day": d.day,
                        "high": d.high,
                        "low": d.low,
                        "condition": d.condition,
                    })
                }).collect();
                serde_json::json!({ "forecast": days })
            }
            _ => serde_json::json!({ "error": format!("Unknown query: {}", query) }),
        }
    }
}
