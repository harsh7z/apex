use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::{KeyCode, KeyEvent};
use ignore::WalkBuilder;
use image::GenericImageView;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use syntect::highlighting::{ThemeSet, Style as SyntectStyle};
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::theme;
use super::Widget;

#[derive(Debug, Clone)]
struct TreeEntry {
    path: PathBuf,
    name: String,
    depth: usize,
    is_dir: bool,
    expanded: bool,
}

pub struct FileBrowserWidget {
    root: PathBuf,
    entries: Vec<TreeEntry>,
    list_state: ListState,
    preview_content: Vec<String>,
    preview_highlighted: Vec<Vec<(SyntectStyle, String)>>,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    preview_scroll: usize,
    // Image preview state
    preview_is_image: bool,
    preview_image_path: Option<PathBuf>,
    preview_image_w: u32,
    preview_image_h: u32,
    // Pending image render (set during render(), executed in post_render())
    pending_image_area: Option<Rect>,
    pending_image_file: Option<PathBuf>,
    last_rendered_image: Option<PathBuf>,
    last_rendered_area: Option<Rect>,
    in_tmux: bool,
    mcat_path: Option<String>,
}

impl FileBrowserWidget {
    pub fn new(path: Option<String>) -> Self {
        let root = path
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        let in_tmux = std::env::var("TMUX").is_ok();

        // Find mcat binary
        let mcat_path = Command::new("which")
            .arg("mcat")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let mut w = Self {
            root,
            entries: Vec::new(),
            list_state: ListState::default(),
            preview_content: Vec::new(),
            preview_highlighted: Vec::new(),
            syntax_set,
            theme_set,
            preview_scroll: 0,
            preview_is_image: false,
            preview_image_path: None,
            preview_image_w: 0,
            preview_image_h: 0,
            pending_image_area: None,
            pending_image_file: None,
            last_rendered_image: None,
            last_rendered_area: None,
            in_tmux,
            mcat_path,
        };
        w.rebuild_tree();
        w.list_state.select(Some(0));
        w.update_preview();
        w
    }

    fn rebuild_tree(&mut self) {
        self.entries.clear();
        self.walk_dir(&self.root.clone(), 0);
    }

    fn walk_dir(&mut self, dir: &Path, depth: usize) {
        let mut children: Vec<(bool, PathBuf, String)> = Vec::new();

        let walker = WalkBuilder::new(dir)
            .max_depth(Some(1))
            .hidden(false)
            .build();

        for entry in walker.flatten() {
            let path = entry.path().to_path_buf();
            if path == dir {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.starts_with('.') && depth == 0 && name != ".gitignore" {
                continue;
            }
            children.push((path.is_dir(), path, name));
        }

        children.sort_by(|a, b| {
            b.0.cmp(&a.0).then_with(|| a.2.to_lowercase().cmp(&b.2.to_lowercase()))
        });

        for (is_dir, path, name) in children {
            let already_exists = self.entries.iter().any(|e| e.path == path);
            if already_exists {
                continue;
            }
            self.entries.push(TreeEntry {
                path,
                name,
                depth,
                is_dir,
                expanded: false,
            });
        }
    }

    fn toggle_expand(&mut self) {
        let idx = match self.list_state.selected() {
            Some(i) => i,
            None => return,
        };
        if idx >= self.entries.len() {
            return;
        }

        if !self.entries[idx].is_dir {
            return;
        }

        if self.entries[idx].expanded {
            self.entries[idx].expanded = false;
            let depth = self.entries[idx].depth;
            let remove_start = idx + 1;
            let mut remove_end = idx + 1;
            while remove_end < self.entries.len() && self.entries[remove_end].depth > depth {
                remove_end += 1;
            }
            self.entries.drain(remove_start..remove_end);
        } else {
            self.entries[idx].expanded = true;
            let dir_path = self.entries[idx].path.clone();
            let depth = self.entries[idx].depth + 1;
            let insert_at = idx + 1;

            let mut children: Vec<TreeEntry> = Vec::new();
            let walker = WalkBuilder::new(&dir_path)
                .max_depth(Some(1))
                .hidden(false)
                .build();

            let mut raw: Vec<(bool, PathBuf, String)> = Vec::new();
            for entry in walker.flatten() {
                let path = entry.path().to_path_buf();
                if path == dir_path {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                raw.push((path.is_dir(), path, name));
            }
            raw.sort_by(|a, b| {
                b.0.cmp(&a.0).then_with(|| a.2.to_lowercase().cmp(&b.2.to_lowercase()))
            });

            for (is_dir, path, name) in raw {
                children.push(TreeEntry {
                    path,
                    name,
                    depth,
                    is_dir,
                    expanded: false,
                });
            }

            for (i, child) in children.into_iter().enumerate() {
                self.entries.insert(insert_at + i, child);
            }
        }
    }

    fn is_image_file(path: &Path) -> bool {
        match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) {
            Some(ext) => matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" | "tif" | "ico" | "svg"),
            None => false,
        }
    }

    fn update_preview(&mut self) {
        let idx = match self.list_state.selected() {
            Some(i) => i,
            None => return,
        };
        if idx >= self.entries.len() {
            return;
        }

        let entry_path = self.entries[idx].path.clone();
        let entry_is_dir = self.entries[idx].is_dir;

        if entry_is_dir {
            self.preview_content = vec!["<directory>".to_string()];
            self.preview_highlighted.clear();
            self.clear_image_state();
            return;
        }

        self.preview_scroll = 0;

        // Check if it's an image
        if Self::is_image_file(&entry_path) {
            self.preview_highlighted.clear();
            self.preview_content.clear();
            self.preview_is_image = true;
            self.preview_image_path = Some(entry_path.clone());

            // Get dimensions using image crate (fast, doesn't decode full image)
            match image::open(&entry_path) {
                Ok(img) => {
                    let (w, h) = img.dimensions();
                    self.preview_image_w = w;
                    self.preview_image_h = h;
                    self.preview_content = vec![format!("{}x{}", w, h)];
                }
                Err(e) => {
                    self.clear_image_state();
                    self.preview_content = vec![format!("<cannot load image: {}>", e)];
                }
            }
            return;
        }

        // Not an image
        self.clear_image_state();

        // Read file (limit to 500 lines)
        match std::fs::read_to_string(&entry_path) {
            Ok(content) => {
                let lines: Vec<String> = content.lines().take(500).map(String::from).collect();

                let ext = entry_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let syntax = self.syntax_set
                    .find_syntax_by_extension(ext)
                    .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
                let theme = &self.theme_set.themes["base16-ocean.dark"];
                let mut highlighter = HighlightLines::new(syntax, theme);

                self.preview_highlighted = lines
                    .iter()
                    .map(|line| {
                        highlighter
                            .highlight_line(line, &self.syntax_set)
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(style, text)| (style, text.to_string()))
                            .collect()
                    })
                    .collect();

                self.preview_content = lines;
            }
            Err(_) => {
                self.preview_content = vec!["<binary or unreadable>".to_string()];
                self.preview_highlighted.clear();
            }
        }
    }

    fn clear_image_state(&mut self) {
        let needs_clear = self.preview_is_image || self.last_rendered_image.is_some();
        self.preview_is_image = false;
        self.preview_image_path = None;
        self.preview_image_w = 0;
        self.preview_image_h = 0;
        self.pending_image_area = None;
        self.pending_image_file = None;
        if needs_clear {
            // Clear any kitty images from terminal
            self.clear_kitty_images();
            self.last_rendered_image = None;
            self.last_rendered_area = None;
        }
    }

    fn clear_kitty_images(&self) {
        let mut stdout = std::io::stdout();
        if self.in_tmux {
            // Tmux passthrough: ESC P tmux; ESC ESC _ G a=d,d=A ; ESC ESC \ ESC \
            let _ = stdout.write_all(b"\x1bPtmux;\x1b\x1b_Ga=d,d=A;\x1b\x1b\\\x1b\\");
        } else {
            let _ = stdout.write_all(b"\x1b_Ga=d,d=A;\x1b\\");
        }
        let _ = stdout.flush();
    }

    /// Send kitty graphics protocol with tmux passthrough wrapping if needed.
    /// Reads the image file, converts to PNG, and sends via kitty protocol.
    fn render_image_at(&mut self, path: &Path, area: Rect) {
        // Skip if already rendered this exact image at this exact area
        if self.last_rendered_image.as_deref() == Some(path)
            && self.last_rendered_area == Some(area) {
            return;
        }

        // Read and convert image to PNG bytes
        let (png_data, img_w, img_h) = match self.encode_image_as_png(path, area) {
            Some(data) => data,
            None => return,
        };

        let b64 = base64_encode(&png_data);

        let avail_cols = area.width.saturating_sub(2) as u32;
        let avail_rows = area.height.saturating_sub(2) as u32;

        // Calculate aspect-ratio-preserving display size in cells.
        // Terminal cells are roughly 2x taller than wide (each cell ~8px wide, ~16px tall).
        // Convert available space to a uniform pixel grid to compare ratios.
        let avail_px_w = avail_cols * 8;   // approximate pixel width
        let avail_px_h = avail_rows * 16;  // approximate pixel height

        let scale_w = avail_px_w as f64 / img_w as f64;
        let scale_h = avail_px_h as f64 / img_h as f64;
        let scale = scale_w.min(scale_h); // fit inside, don't stretch

        let display_cols = ((img_w as f64 * scale) / 8.0).round().max(1.0) as u16;
        let display_rows = ((img_h as f64 * scale) / 16.0).round().max(1.0) as u16;

        // Clamp to available space
        let display_cols = display_cols.min(avail_cols as u16);
        let display_rows = display_rows.min(avail_rows as u16);

        // Center the image in the preview area
        let offset_x = (avail_cols as u16).saturating_sub(display_cols) / 2;
        let offset_y = (avail_rows as u16).saturating_sub(display_rows) / 2;

        let mut stdout = std::io::stdout();

        // Clear previous images
        self.write_kitty_sequence(&mut stdout, "a=d,d=A", "");

        // Position cursor inside the preview border, with centering offset
        let _ = write!(stdout, "\x1b[{};{}H",
            area.y + 2 + offset_y,
            area.x + 2 + offset_x);

        // Send image in chunks via kitty protocol
        let chunk_size = 4096;
        let chunks: Vec<&str> = b64.as_bytes()
            .chunks(chunk_size)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect();

        for (i, chunk) in chunks.iter().enumerate() {
            let is_last = i == chunks.len() - 1;
            let more = if is_last { 0 } else { 1 };

            if i == 0 {
                let params = format!("a=T,f=100,c={},r={},m={}",
                    display_cols, display_rows, more);
                self.write_kitty_sequence(&mut stdout, &params, chunk);
            } else {
                let params = format!("m={}", more);
                self.write_kitty_sequence(&mut stdout, &params, chunk);
            }
        }

        let _ = stdout.flush();
        self.last_rendered_image = Some(path.to_path_buf());
        self.last_rendered_area = Some(area);
    }

    /// Write a kitty graphics protocol sequence, wrapped for tmux if needed.
    fn write_kitty_sequence(&self, stdout: &mut impl Write, params: &str, data: &str) {
        if self.in_tmux {
            // Tmux DCS passthrough:
            // ESC P tmux; ESC ESC _ G <params> ; <data> ESC ESC \ ESC \
            let _ = write!(stdout,
                "\x1bPtmux;\x1b\x1b_G{};{}\x1b\x1b\\\x1b\\",
                params, data
            );
        } else {
            let _ = write!(stdout,
                "\x1b_G{};{}\x1b\\",
                params, data
            );
        }
    }

    /// Encode image as PNG bytes, resized to fit the given area.
    /// Returns (png_bytes, width, height) of the resized image.
    fn encode_image_as_png(&self, path: &Path, area: Rect) -> Option<(Vec<u8>, u32, u32)> {
        let img = image::open(path).ok()?;

        // Resize to fit the preview area (approximate: 8px per col, 16px per row)
        let max_px_w = (area.width.saturating_sub(2) as u32) * 8;
        let max_px_h = (area.height.saturating_sub(2) as u32) * 16;

        let resized = img.resize(max_px_w, max_px_h, image::imageops::FilterType::Lanczos3);
        let (rw, rh) = resized.dimensions();

        let mut png_data = Vec::new();
        let cursor = std::io::Cursor::new(&mut png_data);
        let encoder = image::codecs::png::PngEncoder::new(cursor);
        resized.write_with_encoder(encoder).ok()?;
        Some((png_data, rw, rh))
    }

    fn render_halfblock_preview(&self, path: &Path, width: u16, height: u16) -> Vec<Line<'static>> {
        let img = match image::open(path) {
            Ok(img) => img,
            Err(_) => return vec![Line::from("Cannot load image")],
        };

        let (img_w, img_h) = img.dimensions();
        let term_rows = height as usize;
        let term_cols = width as usize;
        let scale_x = img_w as f32 / term_cols as f32;
        let scale_y = img_h as f32 / (term_rows * 2) as f32;
        let scale = scale_x.max(scale_y);

        let rgba = img.to_rgba8();
        let raw = rgba.as_raw();

        let mut lines = Vec::with_capacity(term_rows);
        for row in 0..term_rows {
            let mut spans = Vec::with_capacity(term_cols);
            for col in 0..term_cols {
                let ix = (col as f32 * scale) as u32;
                let iy_top = (row as f32 * 2.0 * scale) as u32;
                let iy_bot = ((row as f32 * 2.0 + 1.0) * scale) as u32;

                let top = get_pixel(raw, img_w, img_h, ix, iy_top);
                let bot = get_pixel(raw, img_w, img_h, ix, iy_bot);

                spans.push(Span::styled("▀",
                    Style::default().fg(Color::Rgb(top.0, top.1, top.2)).bg(Color::Rgb(bot.0, bot.1, bot.2))));
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

impl Widget for FileBrowserWidget {
    fn title(&self) -> &str {
        "File Browser"
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let i = self.list_state.selected().unwrap_or(0);
                if i > 0 {
                    self.list_state.select(Some(i - 1));
                    self.update_preview();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.list_state.selected().unwrap_or(0);
                if i + 1 < self.entries.len() {
                    self.list_state.select(Some(i + 1));
                    self.update_preview();
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                self.toggle_expand();
                self.update_preview();
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let idx = self.list_state.selected().unwrap_or(0);
                if idx < self.entries.len() && self.entries[idx].is_dir && self.entries[idx].expanded {
                    self.toggle_expand();
                }
            }
            KeyCode::Char('J') => {
                self.preview_scroll = self.preview_scroll.saturating_add(3);
            }
            KeyCode::Char('K') => {
                self.preview_scroll = self.preview_scroll.saturating_sub(3);
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp> {
        match msg {
            McpToTui::Command { command, data } => {
                match command.as_str() {
                    "navigate" => {
                        if let Some(path) = data.as_str() {
                            self.root = PathBuf::from(path);
                            self.rebuild_tree();
                            self.list_state.select(Some(0));
                            self.update_preview();
                        }
                    }
                    "refresh" => {
                        self.rebuild_tree();
                        self.update_preview();
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
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        // Tree view
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|e| {
                let indent = "  ".repeat(e.depth);
                let icon = if e.is_dir {
                    if e.expanded { "▼ " } else { "▶ " }
                } else {
                    "  "
                };
                let style = if e.is_dir {
                    theme::accent_style()
                } else {
                    ratatui::style::Style::default().fg(theme::FG)
                };
                ListItem::new(Span::styled(format!("{}{}{}", indent, icon, e.name), style))
            })
            .collect();

        let tree = List::new(items)
            .block(
                Block::default()
                    .title(format!(" {} ", self.root.display()))
                    .borders(Borders::ALL)
                    .border_style(theme::dim_style())
                    .title_style(theme::title_style()),
            )
            .highlight_style(theme::selected_style());
        frame.render_stateful_widget(tree, chunks[0], &mut self.list_state);

        // Preview
        let preview_title = self
            .list_state
            .selected()
            .and_then(|i| self.entries.get(i))
            .map(|e| {
                if self.preview_is_image {
                    format!(" {} ({}x{}) ", e.name, self.preview_image_w, self.preview_image_h)
                } else {
                    format!(" {} ", e.name)
                }
            })
            .unwrap_or_else(|| " Preview ".to_string());

        if self.preview_is_image && self.preview_image_path.is_some() {
            // For kitty: render empty block now, image in post_render()
            let block = Block::default()
                .title(preview_title)
                .borders(Borders::ALL)
                .border_style(theme::dim_style())
                .title_style(theme::title_style());

            // Show a loading hint inside the block
            let inner = block.inner(chunks[1]);
            frame.render_widget(block, chunks[1]);

            // Check if mcat is available or we can use kitty protocol
            if self.mcat_path.is_some() || !self.in_tmux {
                // Schedule image render for post_render()
                self.pending_image_area = Some(chunks[1]);
                self.pending_image_file = self.preview_image_path.clone();
            } else {
                // Fallback: halfblock rendering (no mcat, inside tmux)
                let path = self.preview_image_path.clone().unwrap();
                let image_lines = self.render_halfblock_preview(&path, inner.width, inner.height);
                let preview = Paragraph::new(image_lines);
                frame.render_widget(preview, inner);
                self.pending_image_area = None;
                self.pending_image_file = None;
            }
        } else if !self.preview_highlighted.is_empty() {
            let lines: Vec<Line> = self
                .preview_highlighted
                .iter()
                .skip(self.preview_scroll)
                .map(|spans| {
                    Line::from(
                        spans
                            .iter()
                            .map(|(style, text)| {
                                let fg = ratatui::style::Color::Rgb(
                                    style.foreground.r,
                                    style.foreground.g,
                                    style.foreground.b,
                                );
                                Span::styled(text.clone(), ratatui::style::Style::default().fg(fg))
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect();

            let preview = Paragraph::new(lines).block(
                Block::default()
                    .title(preview_title)
                    .borders(Borders::ALL)
                    .border_style(theme::dim_style())
                    .title_style(theme::title_style()),
            );
            frame.render_widget(preview, chunks[1]);
            // Clear any pending image
            self.pending_image_area = None;
            self.pending_image_file = None;
        } else {
            let text = self.preview_content.join("\n");
            let preview = Paragraph::new(text).block(
                Block::default()
                    .title(preview_title)
                    .borders(Borders::ALL)
                    .border_style(theme::dim_style())
                    .title_style(theme::title_style()),
            );
            frame.render_widget(preview, chunks[1]);
            self.pending_image_area = None;
            self.pending_image_file = None;
        }
    }

    fn post_render(&mut self) {
        // Render pending image overlay AFTER ratatui has flushed
        if let (Some(area), Some(path)) = (self.pending_image_area.take(), self.pending_image_file.take()) {
            self.render_image_at(&path, area);
        }
    }

    fn query(&self, query: &str) -> serde_json::Value {
        match query {
            "current_file" => {
                let entry = self
                    .list_state
                    .selected()
                    .and_then(|i| self.entries.get(i));
                match entry {
                    Some(e) => serde_json::json!({
                        "path": e.path.display().to_string(),
                        "name": e.name,
                        "is_dir": e.is_dir,
                    }),
                    None => serde_json::json!({"error": "No file selected"}),
                }
            }
            "file_content" => {
                serde_json::json!({
                    "lines": self.preview_content.len(),
                    "content": self.preview_content.iter().take(100).cloned().collect::<Vec<_>>().join("\n"),
                })
            }
            "tree" => {
                let files: Vec<serde_json::Value> = self
                    .entries
                    .iter()
                    .map(|e| serde_json::json!({"path": e.path.display().to_string(), "is_dir": e.is_dir}))
                    .collect();
                serde_json::json!({"root": self.root.display().to_string(), "entries": files})
            }
            _ => serde_json::json!({"error": format!("Unknown query: {}", query)}),
        }
    }
}

fn get_pixel(data: &[u8], w: u32, h: u32, x: u32, y: u32) -> (u8, u8, u8) {
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
        if chunk.len() > 1 { result.push(CHARS[((n >> 6) & 0x3F) as usize] as char); } else { result.push('='); }
        if chunk.len() > 2 { result.push(CHARS[(n & 0x3F) as usize] as char); } else { result.push('='); }
    }
    result
}
