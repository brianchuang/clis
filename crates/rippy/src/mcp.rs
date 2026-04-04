use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool_handler, tool_router, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

use crate::db;

#[derive(Debug, Deserialize, JsonSchema)]
struct ListParams {
    /// Number of entries to return (default: 20)
    count: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchParams {
    /// Search query
    query: String,
    /// Max results (default: 20)
    count: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CopyParams {
    /// Entry ID to copy back to clipboard
    id: i64,
}

#[derive(Debug, Clone)]
pub struct McpServer {
    db_path: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl McpServer {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            tool_router: Self::tool_router(),
        }
    }

    fn open_store(&self) -> Result<db::Store, String> {
        db::Store::open(&self.db_path).map_err(|e| format!("Failed to open database: {e}"))
    }
}

#[tool_router]
impl McpServer {
    /// List recent clipboard entries, ordered by most recent first.
    #[rmcp::tool(description = "List recent clipboard entries")]
    fn list_entries(
        &self,
        Parameters(ListParams { count }): Parameters<ListParams>,
    ) -> Result<String, String> {
        let store = self.open_store()?;
        let entries = store
            .recent(count.unwrap_or(20))
            .map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())
    }

    /// Search clipboard history by substring match.
    #[rmcp::tool(description = "Search clipboard history")]
    fn search_entries(
        &self,
        Parameters(SearchParams { query, count }): Parameters<SearchParams>,
    ) -> Result<String, String> {
        let store = self.open_store()?;
        let entries = store
            .search(&query, count.unwrap_or(20))
            .map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())
    }

    /// Copy a clipboard history entry back to the system clipboard by its ID.
    #[rmcp::tool(description = "Copy a history entry back to clipboard by ID")]
    fn copy_entry(
        &self,
        Parameters(CopyParams { id }): Parameters<CopyParams>,
    ) -> Result<String, String> {
        let store = self.open_store()?;
        let entry = store
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Entry {id} not found"))?;
        crate::clipboard::set_clipboard(&entry.content);
        Ok(format!("Copied entry {id} to clipboard"))
    }

    /// Clear all clipboard history.
    #[rmcp::tool(description = "Clear all clipboard history")]
    fn clear_history(&self) -> Result<String, String> {
        let store = self.open_store()?;
        let count = store.clear().map_err(|e| e.to_string())?;
        Ok(format!("Cleared {count} entries"))
    }
}

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("Rippy clipboard history manager. Use these tools to list, search, and copy clipboard entries.".to_string())
    }
}

pub async fn run(db_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let server = McpServer::new(db_path);
    let service = server
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| format!("MCP server error: {e}"))?;
    service.waiting().await?;
    Ok(())
}
