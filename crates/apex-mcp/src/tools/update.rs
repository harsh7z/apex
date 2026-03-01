use rmcp::schemars;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateWidgetParams {
    #[schemars(description = "The widget ID returned from spawn_widget")]
    pub widget_id: String,

    #[schemars(description = "Command to send (e.g., filter, sort, refresh, navigate, switch_tab)")]
    pub command: String,

    #[schemars(description = "Optional data for the command")]
    pub data: Option<serde_json::Value>,
}
