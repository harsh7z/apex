use rmcp::schemars;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CloseWidgetParams {
    #[schemars(description = "The widget ID to close, or 'all' to close all widgets")]
    pub widget_id: String,
}
