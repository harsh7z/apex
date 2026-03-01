use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme;

use super::Widget;

const MAX_HISTORY: usize = 20;

#[derive(Debug, Clone)]
struct HistoryEntry {
    expression: String,
    result: String,
}

pub struct CalculatorWidget {
    display: String,
    history: Vec<HistoryEntry>,
    error: Option<String>,
}

impl CalculatorWidget {
    pub fn new() -> Self {
        Self {
            display: String::new(),
            history: Vec::new(),
            error: None,
        }
    }

    fn evaluate_expr(input: &str) -> Result<f64, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("Empty expression".to_string());
        }

        // Tokenize
        let tokens = Self::tokenize(input)?;
        // Parse and evaluate with operator precedence
        let mut pos = 0;
        let result = Self::parse_addition(&tokens, &mut pos)?;
        if pos != tokens.len() {
            return Err("Unexpected token".to_string());
        }
        Ok(result)
    }

    fn tokenize(input: &str) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            match chars[i] {
                ' ' => {
                    i += 1;
                }
                '+' => {
                    tokens.push(Token::Plus);
                    i += 1;
                }
                '-' => {
                    // Handle negative numbers: minus is unary if at start or after operator/open paren
                    let is_unary = tokens.is_empty()
                        || matches!(
                            tokens.last(),
                            Some(Token::Plus | Token::Minus | Token::Star | Token::Slash | Token::LParen)
                        );
                    if is_unary {
                        // Parse as negative number
                        i += 1;
                        let start = i;
                        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                            i += 1;
                        }
                        if i == start {
                            return Err("Expected number after minus".to_string());
                        }
                        let num_str: String = chars[start..i].iter().collect();
                        let num: f64 = num_str
                            .parse()
                            .map_err(|_| format!("Invalid number: -{}", num_str))?;
                        tokens.push(Token::Number(-num));
                    } else {
                        tokens.push(Token::Minus);
                        i += 1;
                    }
                }
                '*' => {
                    tokens.push(Token::Star);
                    i += 1;
                }
                '/' => {
                    tokens.push(Token::Slash);
                    i += 1;
                }
                '(' => {
                    tokens.push(Token::LParen);
                    i += 1;
                }
                ')' => {
                    tokens.push(Token::RParen);
                    i += 1;
                }
                c if c.is_ascii_digit() || c == '.' => {
                    let start = i;
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                        i += 1;
                    }
                    let num_str: String = chars[start..i].iter().collect();
                    let num: f64 = num_str
                        .parse()
                        .map_err(|_| format!("Invalid number: {}", num_str))?;
                    tokens.push(Token::Number(num));
                }
                c => {
                    return Err(format!("Unexpected character: {}", c));
                }
            }
        }
        Ok(tokens)
    }

    fn parse_addition(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
        let mut left = Self::parse_multiplication(tokens, pos)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                Token::Plus => {
                    *pos += 1;
                    let right = Self::parse_multiplication(tokens, pos)?;
                    left += right;
                }
                Token::Minus => {
                    *pos += 1;
                    let right = Self::parse_multiplication(tokens, pos)?;
                    left -= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_multiplication(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
        let mut left = Self::parse_primary(tokens, pos)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                Token::Star => {
                    *pos += 1;
                    let right = Self::parse_primary(tokens, pos)?;
                    left *= right;
                }
                Token::Slash => {
                    *pos += 1;
                    let right = Self::parse_primary(tokens, pos)?;
                    if right == 0.0 {
                        return Err("Division by zero".to_string());
                    }
                    left /= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
        if *pos >= tokens.len() {
            return Err("Unexpected end of expression".to_string());
        }
        match tokens[*pos] {
            Token::Number(n) => {
                *pos += 1;
                Ok(n)
            }
            Token::LParen => {
                *pos += 1;
                let result = Self::parse_addition(tokens, pos)?;
                if *pos >= tokens.len() || tokens[*pos] != Token::RParen {
                    return Err("Missing closing parenthesis".to_string());
                }
                *pos += 1;
                Ok(result)
            }
            _ => Err("Expected number or parenthesis".to_string()),
        }
    }

    fn do_evaluate(&mut self) {
        if self.display.is_empty() {
            return;
        }
        let expr = self.display.clone();
        match Self::evaluate_expr(&expr) {
            Ok(result) => {
                let result_str = if result == result.floor() && result.abs() < 1e15 {
                    format!("{}", result as i64)
                } else {
                    format!("{:.6}", result)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                };
                self.history.push(HistoryEntry {
                    expression: expr,
                    result: result_str.clone(),
                });
                if self.history.len() > MAX_HISTORY {
                    self.history.remove(0);
                }
                self.display = result_str;
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

impl Widget for CalculatorWidget {
    fn title(&self) -> &str {
        "Calculator"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        match key.code {
            KeyCode::Char(c @ '0'..='9') | KeyCode::Char(c @ '.') => {
                self.error = None;
                self.display.push(c);
            }
            KeyCode::Char(c @ '+') | KeyCode::Char(c @ '-') | KeyCode::Char(c @ '*') | KeyCode::Char(c @ '/') => {
                self.error = None;
                self.display.push(c);
            }
            KeyCode::Char('(') => {
                self.error = None;
                self.display.push('(');
            }
            KeyCode::Char(')') => {
                self.error = None;
                self.display.push(')');
            }
            KeyCode::Enter => {
                self.do_evaluate();
            }
            KeyCode::Char('c') => {
                self.display.clear();
                self.error = None;
            }
            KeyCode::Backspace => {
                self.display.pop();
                self.error = None;
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, data } => {
                match command.as_str() {
                    "evaluate" => {
                        if let Some(expr) = data.as_str() {
                            self.display = expr.to_string();
                            self.do_evaluate();
                        }
                    }
                    "clear" => {
                        self.display.clear();
                        self.error = None;
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
                Constraint::Length(5),  // Display
                Constraint::Min(5),    // History
                Constraint::Length(8), // Button grid
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Display
        let display_text = if self.display.is_empty() {
            "0".to_string()
        } else {
            self.display.clone()
        };
        let mut display_lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", display_text),
                ratatui::style::Style::default()
                    .fg(theme::FG)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )),
        ];
        if let Some(ref err) = self.error {
            display_lines.push(Line::from(Span::styled(
                format!("  Error: {}", err),
                theme::error_style(),
            )));
        }

        let display_block = Block::default()
            .title(" Display ")
            .borders(Borders::ALL)
            .border_style(theme::accent_style())
            .title_style(theme::title_style());
        frame.render_widget(Paragraph::new(display_lines).block(display_block), chunks[0]);

        // History
        let history_items: Vec<Line> = self
            .history
            .iter()
            .rev()
            .map(|h| {
                Line::from(vec![
                    Span::styled(format!("  {} ", h.expression), theme::dim_style()),
                    Span::styled("= ", theme::accent_style()),
                    Span::styled(&h.result, ratatui::style::Style::default().fg(theme::FG)),
                ])
            })
            .collect();

        let history_block = Block::default()
            .title(format!(" History ({}) ", self.history.len()))
            .borders(Borders::ALL)
            .border_style(theme::dim_style())
            .title_style(theme::title_style());
        frame.render_widget(Paragraph::new(history_items).block(history_block), chunks[1]);

        // Button grid (visual only)
        let button_rows = vec![
            Line::from(vec![
                Span::styled("  ", theme::dim_style()),
                Span::styled(" 7 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" 8 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" 9 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" / ", theme::warning_style()),
            ]),
            Line::from(vec![
                Span::styled("  ", theme::dim_style()),
                Span::styled(" 4 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" 5 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" 6 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" * ", theme::warning_style()),
            ]),
            Line::from(vec![
                Span::styled("  ", theme::dim_style()),
                Span::styled(" 1 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" 2 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" 3 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" - ", theme::warning_style()),
            ]),
            Line::from(vec![
                Span::styled("  ", theme::dim_style()),
                Span::styled(" 0 ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" . ", theme::accent_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" = ", theme::success_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" + ", theme::warning_style()),
            ]),
            Line::from(vec![
                Span::styled("  ", theme::dim_style()),
                Span::styled(" ( ", theme::dim_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" ) ", theme::dim_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled(" C ", theme::error_style()),
                Span::styled(" ", theme::dim_style()),
                Span::styled("BS ", theme::error_style()),
            ]),
        ];

        let button_block = Block::default()
            .title(" Keypad ")
            .borders(Borders::ALL)
            .border_style(theme::dim_style())
            .title_style(theme::title_style());
        frame.render_widget(Paragraph::new(button_rows).block(button_block), chunks[2]);

        // Status bar
        let status = Line::from(vec![
            Span::styled(" 0-9", theme::accent_style()),
            Span::styled(" num  ", theme::dim_style()),
            Span::styled("+-*/", theme::accent_style()),
            Span::styled(" ops  ", theme::dim_style()),
            Span::styled("()", theme::accent_style()),
            Span::styled(" parens  ", theme::dim_style()),
            Span::styled("Enter", theme::accent_style()),
            Span::styled(" eval  ", theme::dim_style()),
            Span::styled("c", theme::accent_style()),
            Span::styled(" clear  ", theme::dim_style()),
            Span::styled("Bksp", theme::accent_style()),
            Span::styled(" back", theme::dim_style()),
        ]);
        frame.render_widget(Paragraph::new(status), chunks[3]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "result" => {
                serde_json::json!({
                    "display": self.display,
                    "error": self.error,
                })
            }
            "history" => {
                let items: Vec<serde_json::Value> = self
                    .history
                    .iter()
                    .map(|h| {
                        serde_json::json!({
                            "expression": h.expression,
                            "result": h.result,
                        })
                    })
                    .collect();
                serde_json::json!({ "history": items })
            }
            _ => serde_json::json!({ "error": format!("Unknown query: {}", query) }),
        }
    }
}
