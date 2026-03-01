pub mod calculator;
pub mod clipboard_history;
pub mod disk_usage;
pub mod file_browser;
pub mod image_viewer;
pub mod git_dashboard;
pub mod project_overview;
pub mod system_monitor;
pub mod timer;
pub mod todo_list;
pub mod weather;

use apex_common::{McpToTui, TuiToMcp};
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

pub trait Widget {
    fn title(&self) -> &str;
    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiToMcp>;
    fn handle_ipc(&mut self, msg: McpToTui) -> Vec<TuiToMcp>;
    fn tick(&mut self) -> Vec<TuiToMcp>;
    fn render(&mut self, frame: &mut Frame, area: Rect);
    fn query(&self, query: &str) -> serde_json::Value;
    /// Called after terminal.draw() to render overlays (e.g. kitty images)
    /// that must bypass ratatui's rendering pipeline.
    fn post_render(&mut self) {}
}
