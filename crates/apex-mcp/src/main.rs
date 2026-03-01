mod ipc;
mod tmux;
mod tools;

use std::future::Future;

use apex_common::{McpToTui, TuiToMcp, generate_widget_id, socket_path};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    handler::server::tool::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use tools::close::CloseWidgetParams;
use tools::query::QueryWidgetParams;
use tools::spawn::SpawnWidgetParams;
use tools::update::UpdateWidgetParams;

#[derive(Debug, Clone)]
struct WidgetInfo {
    widget_id: String,
    widget_type: String,
    pane_id: String,
    socket_path: String,
}

#[derive(Clone)]
pub struct ApexMcp {
    widgets: Arc<Mutex<HashMap<String, WidgetInfo>>>,
    tool_router: ToolRouter<Self>,
}

impl ApexMcp {
    fn new() -> Self {
        Self {
            widgets: Arc::new(Mutex::new(HashMap::new())),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl ApexMcp {
    #[tool(description = "Spawn a new TUI widget in a tmux pane. Returns the widget ID for later reference. Available widget types: system_monitor (CPU/memory/processes), git_dashboard (status/log/branches), file_browser (tree view with syntax highlighting), project_overview (stats/deps/activity), weather (current conditions and forecast), todo_list (task manager with priorities), calculator (math expression evaluator), timer (stopwatch/countdown/pomodoro), disk_usage (storage visualization), clipboard_history (recent clipboard entries), image_viewer (display images in terminal - pass image path via 'path' parameter, or send 'load' command via update_widget).")]
    async fn spawn_widget(
        &self,
        Parameters(params): Parameters<SpawnWidgetParams>,
    ) -> Result<CallToolResult, McpError> {
        if apex_common::WidgetType::from_str(&params.widget_type).is_none() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Unknown widget type '{}'. Valid types: system_monitor, git_dashboard, file_browser, project_overview",
                params.widget_type
            ))]));
        }

        let widget_id = generate_widget_id();
        let sock_path = socket_path(&widget_id);
        let size = params.size.clamp(10, 80);

        // Special case: image_viewer uses mcat for full-quality rendering
        if params.widget_type == "image_viewer" {
            let image_path = match &params.path {
                Some(p) => p.clone(),
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "image_viewer requires a 'path' parameter with the image file path",
                    )]));
                }
            };

            match tmux::spawn_image_pane(&image_path, &params.position, size) {
                Ok(pane_id) => {
                    let info = WidgetInfo {
                        widget_id: widget_id.clone(),
                        widget_type: params.widget_type.clone(),
                        pane_id,
                        socket_path: sock_path,
                    };
                    self.widgets.lock().await.insert(widget_id.clone(), info);
                    return Ok(CallToolResult::success(vec![Content::text(format!(
                        "Image displayed successfully using mcat.\nwidget_id: {}\nfile: {}",
                        widget_id, image_path
                    ))]));
                }
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Error displaying image: {}",
                        e
                    ))]));
                }
            }
        }

        match tmux::spawn_pane(
            &params.widget_type,
            &sock_path,
            &params.position,
            size,
            params.path.as_deref(),
        ) {
            Ok(pane_id) => {
                let info = WidgetInfo {
                    widget_id: widget_id.clone(),
                    widget_type: params.widget_type.clone(),
                    pane_id,
                    socket_path: sock_path,
                };
                self.widgets.lock().await.insert(widget_id.clone(), info);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Widget spawned successfully.\nwidget_id: {}\ntype: {}",
                    widget_id, params.widget_type
                ))]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error spawning widget: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Send a command to an existing widget. Commands vary by widget type: system_monitor (filter, sort), git_dashboard (refresh, switch_tab), file_browser (navigate, refresh), project_overview (refresh).")]
    async fn update_widget(
        &self,
        Parameters(params): Parameters<UpdateWidgetParams>,
    ) -> Result<CallToolResult, McpError> {
        let widgets = self.widgets.lock().await;
        let info = match widgets.get(&params.widget_id) {
            Some(info) => info.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Widget '{}' not found",
                    params.widget_id
                ))]));
            }
        };
        drop(widgets);

        let msg = McpToTui::Command {
            command: params.command.clone(),
            data: params.data.unwrap_or(serde_json::Value::Null),
        };

        match ipc::send_and_receive(&info.socket_path, msg).await {
            Ok(TuiToMcp::Ack { success, error }) => {
                if success {
                    Ok(CallToolResult::success(vec![Content::text(
                        "Command sent successfully",
                    )]))
                } else {
                    Ok(CallToolResult::error(vec![Content::text(format!(
                        "Command failed: {}",
                        error.unwrap_or_default()
                    ))]))
                }
            }
            Ok(resp) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Unexpected response: {:?}",
                resp
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "IPC error: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Query a widget for structured data. Queries vary by widget type: system_monitor (cpu_usage, memory_usage, process_list), git_dashboard (status, log, branches, diff), file_browser (current_file, file_content, tree), project_overview (summary, dependencies, activity).")]
    async fn query_widget(
        &self,
        Parameters(params): Parameters<QueryWidgetParams>,
    ) -> Result<CallToolResult, McpError> {
        let widgets = self.widgets.lock().await;
        let info = match widgets.get(&params.widget_id) {
            Some(info) => info.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Widget '{}' not found",
                    params.widget_id
                ))]));
            }
        };
        drop(widgets);

        let request_id = uuid::Uuid::new_v4().to_string();
        let msg = McpToTui::Query {
            request_id,
            query: params.query.clone(),
        };

        match ipc::send_and_receive(&info.socket_path, msg).await {
            Ok(TuiToMcp::QueryResponse { data, .. }) => {
                let json_str = serde_json::to_string_pretty(&data).unwrap_or_else(|_| "{}".into());
                Ok(CallToolResult::success(vec![Content::text(json_str)]))
            }
            Ok(resp) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Unexpected response: {:?}",
                resp
            ))])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "IPC error: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Close a widget and its tmux pane. Use widget_id='all' to close all widgets.")]
    async fn close_widget(
        &self,
        Parameters(params): Parameters<CloseWidgetParams>,
    ) -> Result<CallToolResult, McpError> {
        if params.widget_id == "all" {
            let mut widgets = self.widgets.lock().await;
            let mut results = Vec::new();
            let all_ids: Vec<String> = widgets.keys().cloned().collect();
            for id in all_ids {
                if let Some(info) = widgets.remove(&id) {
                    let _ = ipc::send_shutdown(&info.socket_path).await;
                    let _ = tmux::kill_pane(&info.pane_id);
                    let _ = std::fs::remove_file(&info.socket_path);
                    results.push(format!("Closed {}", id));
                }
            }
            if results.is_empty() {
                Ok(CallToolResult::success(vec![Content::text(
                    "No widgets to close",
                )]))
            } else {
                Ok(CallToolResult::success(vec![Content::text(
                    results.join("\n"),
                )]))
            }
        } else {
            let mut widgets = self.widgets.lock().await;
            match widgets.remove(&params.widget_id) {
                Some(info) => {
                    let _ = ipc::send_shutdown(&info.socket_path).await;
                    let _ = tmux::kill_pane(&info.pane_id);
                    let _ = std::fs::remove_file(&info.socket_path);
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Widget {} closed",
                        params.widget_id
                    ))]))
                }
                None => Ok(CallToolResult::error(vec![Content::text(format!(
                    "Widget '{}' not found",
                    params.widget_id
                ))])),
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for ApexMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Apex — AI-native TUI workspace. Spawn rich terminal widgets in tmux panes. \
                 IMPORTANT: Only use Apex tools when the user EXPLICITLY asks to show a widget, \
                 display something visually, or interact with an existing widget. \
                 Do NOT use query_widget or update_widget unless the user specifically asks \
                 about widget data or wants to control a widget. For normal conversations and \
                 questions, respond normally without using any Apex tools. \
                 When spawning: use spawn_widget. To send commands: update_widget. \
                 To get data from widget: query_widget. To close: close_widget. \
                 After spawning, tell the user: Focus is now on the widget. \
                 Press Ctrl-a then arrow keys to switch between the widget and Claude panes. \
                 Press q to close the widget."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Clean up stale sockets on startup
    cleanup_stale_sockets();

    // All logging to stderr (stdout = MCP transport)
    eprintln!("apex-mcp: Starting MCP server...");

    let service = ApexMcp::new()
        .serve(transport::stdio())
        .await?;

    eprintln!("apex-mcp: Server running");
    service.waiting().await?;

    eprintln!("apex-mcp: Server stopped");
    Ok(())
}

fn cleanup_stale_sockets() {
    if let Ok(entries) = std::fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("apex-") && name.ends_with(".sock") {
                let _ = std::fs::remove_file(entry.path());
                eprintln!("apex-mcp: Cleaned up stale socket: {}", name);
            }
        }
    }
}
