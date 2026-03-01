use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use image::GenericImageView;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::io::Write;
use std::path::PathBuf;

use crate::theme;
use super::Widget;

pub struct ImageViewerWidget {
    path: Option<PathBuf>,
    img_width: u32,
    img_height: u32,
    image_data: Option<Vec<u8>>, // Raw RGBA pixels
    error: Option<String>,
    rendered: bool,
    needs_rerender: bool,
    last_area: Option<Rect>,
    zoom: f32,
    fit_mode: FitMode,
    use_kitty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FitMode {
    Fit,
    Actual,
    Manual,
}

impl ImageViewerWidget {
    pub fn new(path: Option<String>) -> Self {
        // Detect if terminal supports Kitty graphics protocol
        let use_kitty = detect_kitty_support();

        let mut w = Self {
            path: None,
            img_width: 0,
            img_height: 0,
            image_data: None,
            error: None,
            rendered: false,
            needs_rerender: true,
            last_area: None,
            zoom: 1.0,
            fit_mode: FitMode::Fit,
            use_kitty,
        };
        if let Some(p) = path {
            w.load_image(&p);
        }
        w
    }

    fn load_image(&mut self, path: &str) {
        self.error = None;
        self.path = Some(PathBuf::from(path));
        self.rendered = false;
        self.needs_rerender = true;
        self.zoom = 1.0;
        self.fit_mode = FitMode::Fit;

        match image::open(path) {
            Ok(img) => {
                let (w, h) = img.dimensions();
                self.img_width = w;
                self.img_height = h;
                // Store as RGBA bytes
                self.image_data = Some(img.to_rgba8().into_raw());
            }
            Err(e) => {
                self.error = Some(format!("Failed to load image: {}", e));
                self.image_data = None;
            }
        }
    }

    /// Render image using Kitty graphics protocol directly to terminal.
    /// This bypasses ratatui and writes escape sequences to stdout.
    fn render_kitty(&mut self, area: Rect) {
        if self.image_data.is_none() || !self.needs_rerender {
            return;
        }

        let img_data = self.image_data.as_ref().unwrap();

        // Calculate display size based on fit mode
        let max_cols = area.width.saturating_sub(2) as u32;
        let max_rows = area.height.saturating_sub(3) as u32; // Leave room for border + status

        // Each terminal cell is roughly 8x16 pixels, but we let Kitty scale
        let (display_cols, display_rows) = match self.fit_mode {
            FitMode::Fit => {
                // Fit to available area
                (max_cols, max_rows)
            }
            FitMode::Actual => {
                // 1:1 pixel mapping (1 pixel per cell... not ideal, let kitty handle)
                (max_cols, max_rows)
            }
            FitMode::Manual => {
                let c = (max_cols as f32 * self.zoom) as u32;
                let r = (max_rows as f32 * self.zoom) as u32;
                (c.max(1), r.max(1))
            }
        };

        // First, clear any existing kitty images
        let mut stdout = std::io::stdout();
        // Delete all images
        let _ = write!(stdout, "\x1b_Ga=d,d=A;\x1b\\");

        // Encode image data as base64
        use std::io::Cursor;
        let mut png_data = Vec::new();
        {
            let img = image::RgbaImage::from_raw(self.img_width, self.img_height, img_data.clone());
            if let Some(img) = img {
                let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut png_data));
                if let Err(e) = img.write_with_encoder(encoder) {
                    self.error = Some(format!("Failed to encode: {}", e));
                    return;
                }
            }
        }

        let b64 = base64_encode(&png_data);

        // Move cursor to the image position (inside the border)
        let start_row = area.y + 1;
        let start_col = area.x + 1;
        let _ = write!(stdout, "\x1b[{};{}H", start_row + 1, start_col + 1);

        // Send image using Kitty protocol with chunked transmission
        // Format: ESC_G<key>=<val>,... ; <base64data> ESC\
        // f=100 means PNG, a=T means transmit and display
        // c=cols, r=rows for display size in cells
        let chunk_size = 4096;
        let chunks: Vec<&str> = b64.as_bytes()
            .chunks(chunk_size)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect();

        for (i, chunk) in chunks.iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == chunks.len() - 1;
            let more = if is_last { 0 } else { 1 };

            if is_first {
                let _ = write!(
                    stdout,
                    "\x1b_Ga=T,f=100,c={},r={},m={};{}\x1b\\",
                    display_cols, display_rows, more, chunk
                );
            } else {
                let _ = write!(
                    stdout,
                    "\x1b_Gm={};{}\x1b\\",
                    more, chunk
                );
            }
        }

        let _ = stdout.flush();
        self.rendered = true;
        self.needs_rerender = false;
    }

    /// Fallback: render using half-block characters
    fn render_halfblocks(&self, width: u16, height: u16) -> Vec<Line<'static>> {
        let img_data = match &self.image_data {
            Some(d) => d,
            None => return vec![Line::from("No image loaded")],
        };

        let term_rows = height as usize;
        let term_cols = width as usize;

        let scale_x = self.img_width as f32 / term_cols as f32;
        let scale_y = self.img_height as f32 / (term_rows * 2) as f32;
        let scale = match self.fit_mode {
            FitMode::Fit => scale_x.max(scale_y),
            FitMode::Actual | FitMode::Manual => 1.0 / self.zoom,
        };

        let mut lines = Vec::with_capacity(term_rows);

        for row in 0..term_rows {
            let mut spans = Vec::with_capacity(term_cols);

            for col in 0..term_cols {
                let img_x = (col as f32 * scale) as u32;
                let img_y_top = (row as f32 * 2.0 * scale) as u32;
                let img_y_bot = ((row as f32 * 2.0 + 1.0) * scale) as u32;

                let top = get_pixel_rgba(img_data, self.img_width, self.img_height, img_x, img_y_top);
                let bot = get_pixel_rgba(img_data, self.img_width, self.img_height, img_x, img_y_bot);

                spans.push(Span::styled(
                    "▀",
                    Style::default()
                        .fg(Color::Rgb(top.0, top.1, top.2))
                        .bg(Color::Rgb(bot.0, bot.1, bot.2)),
                ));
            }

            lines.push(Line::from(spans));
        }

        lines
    }
}

fn get_pixel_rgba(data: &[u8], w: u32, h: u32, x: u32, y: u32) -> (u8, u8, u8) {
    if x >= w || y >= h {
        return (15, 17, 26);
    }
    let idx = ((y * w + x) * 4) as usize;
    if idx + 2 < data.len() {
        (data[idx], data[idx + 1], data[idx + 2])
    } else {
        (15, 17, 26)
    }
}

fn detect_kitty_support() -> bool {
    // Check for Kitty-compatible terminals
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("kitty") || term.contains("ghostty") || term.contains("xterm-ghostty") {
            return true;
        }
    }
    if let Ok(term_prog) = std::env::var("TERM_PROGRAM") {
        let tp = term_prog.to_lowercase();
        if tp.contains("kitty") || tp.contains("ghostty") || tp.contains("wezterm") {
            return true;
        }
    }
    false
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };

        let n = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

impl Widget for ImageViewerWidget {
    fn title(&self) -> &str {
        "Image Viewer"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        match key.code {
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.zoom = (self.zoom * 1.2).min(10.0);
                self.fit_mode = FitMode::Manual;
                self.needs_rerender = true;
            }
            KeyCode::Char('-') => {
                self.zoom = (self.zoom / 1.2).max(0.1);
                self.fit_mode = FitMode::Manual;
                self.needs_rerender = true;
            }
            KeyCode::Char('f') => {
                self.fit_mode = FitMode::Fit;
                self.zoom = 1.0;
                self.needs_rerender = true;
            }
            KeyCode::Char('1') => {
                self.fit_mode = FitMode::Actual;
                self.zoom = 1.0;
                self.needs_rerender = true;
            }
            KeyCode::Char('r') => {
                self.zoom = 1.0;
                self.fit_mode = FitMode::Fit;
                self.needs_rerender = true;
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, data } => {
                match command.as_str() {
                    "load" | "open" => {
                        if let Some(path) = data.as_str() {
                            self.load_image(path);
                        }
                        vec![TuiToMcp::Ack {
                            success: self.error.is_none(),
                            error: self.error.clone(),
                        }]
                    }
                    "zoom_in" => {
                        self.zoom = (self.zoom * 1.5).min(10.0);
                        self.fit_mode = FitMode::Manual;
                        self.needs_rerender = true;
                        vec![TuiToMcp::Ack { success: true, error: None }]
                    }
                    "zoom_out" => {
                        self.zoom = (self.zoom / 1.5).max(0.1);
                        self.fit_mode = FitMode::Manual;
                        self.needs_rerender = true;
                        vec![TuiToMcp::Ack { success: true, error: None }]
                    }
                    "fit" => {
                        self.fit_mode = FitMode::Fit;
                        self.zoom = 1.0;
                        self.needs_rerender = true;
                        vec![TuiToMcp::Ack { success: true, error: None }]
                    }
                    _ => vec![TuiToMcp::Ack {
                        success: false,
                        error: Some(format!("Unknown command: {}", command)),
                    }],
                }
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
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(area);

        let title = match &self.path {
            Some(p) => format!(" {} ({}x{}) {} ",
                p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                self.img_width, self.img_height,
                if self.use_kitty { "[Kitty]" } else { "[Blocks]" }),
            None => " Image Viewer ".to_string(),
        };

        if let Some(ref err) = self.error {
            let p = Paragraph::new(err.as_str())
                .block(Block::default().title(title).borders(Borders::ALL)
                    .border_style(theme::error_style()).title_style(theme::title_style()));
            frame.render_widget(p, chunks[0]);
        } else if self.image_data.is_none() {
            let p = Paragraph::new("No image loaded. Use IPC 'load' command with a file path.")
                .block(Block::default().title(title).borders(Borders::ALL)
                    .border_style(theme::dim_style()).title_style(theme::title_style()));
            frame.render_widget(p, chunks[0]);
        } else if self.use_kitty {
            // Render border via ratatui, image via Kitty protocol
            let block = Block::default().title(title).borders(Borders::ALL)
                .border_style(theme::dim_style()).title_style(theme::title_style());
            frame.render_widget(block, chunks[0]);

            // Check if area changed (need re-render)
            if self.last_area != Some(chunks[0]) {
                self.needs_rerender = true;
            }
            self.last_area = Some(chunks[0]);

            // Kitty rendering happens after ratatui flush via post-render
            // We set a flag and do it in the next tick or directly
            if self.needs_rerender {
                self.render_kitty(chunks[0]);
            }
        } else {
            // Halfblock fallback
            let inner_w = chunks[0].width.saturating_sub(2);
            let inner_h = chunks[0].height.saturating_sub(2);
            let image_lines = self.render_halfblocks(inner_w, inner_h);

            let p = Paragraph::new(image_lines)
                .block(Block::default().title(title).borders(Borders::ALL)
                    .border_style(theme::dim_style()).title_style(theme::title_style()));
            frame.render_widget(p, chunks[0]);
        }

        // Status bar
        let zoom_str = match self.fit_mode {
            FitMode::Fit => "fit".to_string(),
            FitMode::Actual => "1:1".to_string(),
            FitMode::Manual => format!("{:.0}%", self.zoom * 100.0),
        };
        let mode_str = if self.use_kitty { "kitty" } else { "blocks" };
        let status = Line::from(vec![
            Span::styled(" +/-", theme::accent_style()),
            Span::styled(" zoom  ", theme::dim_style()),
            Span::styled("f", theme::accent_style()),
            Span::styled(" fit  ", theme::dim_style()),
            Span::styled("1", theme::accent_style()),
            Span::styled(" 1:1  ", theme::dim_style()),
            Span::styled("r", theme::accent_style()),
            Span::styled(" reset  ", theme::dim_style()),
            Span::styled(format!("[{} | {}]", zoom_str, mode_str), theme::accent_style()),
        ]);
        frame.render_widget(Paragraph::new(status), chunks[1]);
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "info" => {
                serde_json::json!({
                    "path": self.path.as_ref().map(|p| p.display().to_string()),
                    "width": self.img_width,
                    "height": self.img_height,
                    "zoom": self.zoom,
                    "renderer": if self.use_kitty { "kitty" } else { "halfblocks" },
                })
            }
            "loaded" => {
                serde_json::json!({
                    "loaded": self.image_data.is_some(),
                    "error": self.error,
                })
            }
            _ => serde_json::json!({"error": format!("Unknown query: {}", query)}),
        }
    }
}
