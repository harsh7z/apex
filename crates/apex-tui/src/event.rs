use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};
use std::time::Duration;
use tokio::sync::mpsc;

/// Events that the TUI event loop handles
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    Ipc(apex_common::McpToTui),
}

/// Spawns a crossterm key event reader on a background thread
pub fn spawn_key_reader(tx: mpsc::UnboundedSender<AppEvent>) {
    std::thread::spawn(move || {
        loop {
            if event::poll(Duration::from_millis(250)).unwrap_or(false) {
                if let Ok(CrosstermEvent::Key(key)) = event::read() {
                    if tx.send(AppEvent::Key(key)).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

/// Spawns a tick sender
pub fn spawn_ticker(tx: mpsc::UnboundedSender<AppEvent>, interval: Duration) {
    tokio::spawn(async move {
        let mut interval_timer = tokio::time::interval(interval);
        loop {
            interval_timer.tick().await;
            if tx.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });
}
