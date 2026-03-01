use rmcp::schemars;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct QueryWidgetParams {
    #[schemars(description = "The widget ID returned from spawn_widget")]
    pub widget_id: String,

    #[schemars(description = "Query to run (e.g., cpu_usage, memory_usage, process_list, status, log, branches, summary)")]
    pub query: String,
}
