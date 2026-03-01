use serde::{Deserialize, Serialize};

/// Messages sent from MCP server to TUI widget
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpToTui {
    Command {
        command: String,
        #[serde(default)]
        data: serde_json::Value,
    },
    Query {
        request_id: String,
        query: String,
    },
    Shutdown,
}

/// Messages sent from TUI widget back to MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TuiToMcp {
    QueryResponse {
        request_id: String,
        data: serde_json::Value,
    },
    Event {
        event_type: String,
        data: serde_json::Value,
    },
    Ack {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Widget types supported by Apex
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
    SystemMonitor,
    GitDashboard,
    FileBrowser,
    ProjectOverview,
    Weather,
    TodoList,
    Calculator,
    Timer,
    DiskUsage,
    ClipboardHistory,
    ImageViewer,
}

impl WidgetType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "system_monitor" => Some(Self::SystemMonitor),
            "git_dashboard" => Some(Self::GitDashboard),
            "file_browser" => Some(Self::FileBrowser),
            "project_overview" => Some(Self::ProjectOverview),
            "weather" => Some(Self::Weather),
            "todo_list" => Some(Self::TodoList),
            "calculator" => Some(Self::Calculator),
            "timer" => Some(Self::Timer),
            "disk_usage" => Some(Self::DiskUsage),
            "clipboard_history" => Some(Self::ClipboardHistory),
            "image_viewer" => Some(Self::ImageViewer),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SystemMonitor => "system_monitor",
            Self::GitDashboard => "git_dashboard",
            Self::FileBrowser => "file_browser",
            Self::ProjectOverview => "project_overview",
            Self::Weather => "weather",
            Self::TodoList => "todo_list",
            Self::Calculator => "calculator",
            Self::Timer => "timer",
            Self::DiskUsage => "disk_usage",
            Self::ClipboardHistory => "clipboard_history",
            Self::ImageViewer => "image_viewer",
        }
    }
}

/// Position for spawning tmux panes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PanePosition {
    #[default]
    Right,
    Bottom,
}

/// Socket path for a widget
pub fn socket_path(widget_id: &str) -> String {
    format!("/tmp/{}.sock", widget_id)
}
