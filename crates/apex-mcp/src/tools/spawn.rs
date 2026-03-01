use rmcp::schemars;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SpawnWidgetParams {
    #[schemars(description = "Widget type: system_monitor, git_dashboard, file_browser, project_overview")]
    pub widget_type: String,

    #[schemars(description = "Position: right or bottom")]
    #[serde(default = "default_position")]
    pub position: String,

    #[schemars(description = "Pane size as percentage (10-80)")]
    #[serde(default = "default_size")]
    pub size: u32,

    #[schemars(description = "Optional working directory path for the widget")]
    pub path: Option<String>,
}

fn default_position() -> String {
    "right".to_string()
}

fn default_size() -> u32 {
    40
}
