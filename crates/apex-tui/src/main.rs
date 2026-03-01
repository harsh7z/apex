mod app;
mod event;
mod ipc_server;
mod theme;
mod widgets;

use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

use app::App;
use event::{spawn_key_reader, spawn_ticker};
use widgets::Widget;

#[derive(Parser)]
#[command(name = "apex-tui", about = "Apex TUI widget renderer")]
struct Cli {
    /// Widget type to display
    #[arg(long)]
    widget: String,

    /// Unix socket path for IPC
    #[arg(long)]
    socket: String,

    /// Optional path for widgets that need it (git, file browser, project)
    #[arg(long)]
    path: Option<String>,

    /// Tick interval in milliseconds
    #[arg(long, default_value = "2000")]
    tick_rate: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Create the widget
    let widget: Box<dyn Widget> = match cli.widget.as_str() {
        "system_monitor" => Box::new(widgets::system_monitor::SystemMonitorWidget::new()),
        "git_dashboard" => Box::new(widgets::git_dashboard::GitDashboardWidget::new(cli.path.clone())),
        "file_browser" => Box::new(widgets::file_browser::FileBrowserWidget::new(cli.path.clone())),
        "project_overview" => Box::new(widgets::project_overview::ProjectOverviewWidget::new(cli.path.clone())),
        "weather" => Box::new(widgets::weather::WeatherWidget::new(cli.path.clone())),
        "todo_list" => Box::new(widgets::todo_list::TodoListWidget::new()),
        "calculator" => Box::new(widgets::calculator::CalculatorWidget::new()),
        "timer" => Box::new(widgets::timer::TimerWidget::new()),
        "disk_usage" => Box::new(widgets::disk_usage::DiskUsageWidget::new()),
        "clipboard_history" => Box::new(widgets::clipboard_history::ClipboardHistoryWidget::new()),
        "image_viewer" => Box::new(widgets::image_viewer::ImageViewerWidget::new(cli.path.clone())),
        other => {
            eprintln!("Unknown widget type: {}", other);
            std::process::exit(1);
        }
    };

    // IPC response channel
    let (response_tx, response_rx) = mpsc::unbounded_channel();

    // Event channel
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    // Start IPC server in background
    let socket_path = cli.socket.clone();
    let ipc_event_tx = event_tx.clone();
    tokio::spawn(async move {
        if let Err(e) = ipc_server::start_ipc_server(socket_path, ipc_event_tx, response_rx).await {
            eprintln!("IPC server error: {}", e);
        }
    });

    // Start key reader and ticker
    spawn_key_reader(event_tx.clone());
    spawn_ticker(event_tx.clone(), Duration::from_millis(cli.tick_rate));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Create app
    let mut app = App::new(widget, response_tx);

    // Main loop
    loop {
        terminal.draw(|frame| app.render(frame))?;
        app.post_render();

        if let Some(event) = event_rx.recv().await {
            app.handle_event(event);
        }

        if app.should_quit {
            break;
        }
    }

    // Cleanup
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Remove socket
    let _ = std::fs::remove_file(&cli.socket);

    // Kill our own tmux pane so it closes immediately
    if std::env::var("TMUX").is_ok() {
        let _ = std::process::Command::new("tmux")
            .args(["kill-pane", "-t", &format!("{}", std::env::var("TMUX_PANE").unwrap_or_default())])
            .output();
    }

    Ok(())
}
