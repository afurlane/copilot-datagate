//! MCP stdio transport backed by the policy-safe tool implementations.

use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use serde_json::Value;
use sqlx::sqlite::SqliteConnectOptions;

use crate::backend::sqlite::SqliteBackend;
use crate::config::SqliteConfig;
use crate::mcp::{
    AggregateTool, AggregateToolRequest, SearchTool, SearchToolRequest, SelectTool,
    SelectToolRequest,
};
use crate::policy::Policy;

#[derive(Clone)]
pub struct McpServer {
    backend: Arc<SqliteBackend>,
    policy: Policy,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {}

#[tool_router(router = tool_router)]
impl McpServer {
    pub fn new(backend: Arc<SqliteBackend>, policy: Policy) -> Self {
        Self {
            backend,
            policy,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "select",
        description = "Policy-validated read-only row selection"
    )]
    async fn select(&self, Parameters(request): Parameters<SelectToolRequest>) -> String {
        execute_json(
            SelectTool::new(&self.policy, None)
                .execute(self.backend.as_ref(), request)
                .await,
        )
    }

    #[tool(
        name = "search",
        description = "Policy-validated read-only text search"
    )]
    async fn search(&self, Parameters(request): Parameters<SearchToolRequest>) -> String {
        execute_json(
            SearchTool::new(&self.policy, None)
                .execute(self.backend.as_ref(), request)
                .await,
        )
    }

    #[tool(
        name = "aggregate",
        description = "Policy-validated read-only aggregation"
    )]
    async fn aggregate(&self, Parameters(request): Parameters<AggregateToolRequest>) -> String {
        execute_json(
            AggregateTool::new(&self.policy, None)
                .execute(self.backend.as_ref(), request)
                .await,
        )
    }
}

fn execute_json<T: serde::Serialize>(result: Result<T, crate::error::PublicError>) -> String {
    let serialized = match result {
        Ok(value) => serde_json::to_string(&value),
        Err(error) => serde_json::to_string(&error),
    };
    serialized.unwrap_or_else(|_| {
        serde_json::to_string(&Value::String(
            "the operation could not be completed".into(),
        ))
        .unwrap_or_else(|_| "null".into())
    })
}

pub async fn connect_sqlite(path: String) -> anyhow::Result<Arc<SqliteBackend>> {
    let config = SqliteConfig {
        connect_options: SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .create_if_missing(false),
        max_connections: 5,
        acquire_timeout_secs: 5,
    };
    Ok(Arc::new(SqliteBackend::connect(&config).await?))
}

pub async fn serve_stdio(server: McpServer) -> anyhow::Result<()> {
    server
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PolicyConfig, TableConfig};
    use std::collections::HashMap;

    fn policy() -> Policy {
        let mut tables = HashMap::new();
        tables.insert(
            "users".into(),
            TableConfig {
                columns: vec!["id".into()],
                filter_operators: HashMap::new(),
            },
        );
        Policy::new(PolicyConfig {
            tables,
            default_row_limit: 10,
            max_row_limit: 10,
            max_query_complexity: 10,
            max_output_bytes: 4096,
        })
    }

    #[test]
    fn serializes_public_tool_errors_without_internal_details() {
        let error = execute_json::<Value>(Err(crate::error::PublicError::policy_denied()));
        assert!(error.contains("policy_denied"));
        assert!(!error.contains("SELECT"));
    }

    #[test]
    fn constructs_policy_for_transport_state() {
        assert!(policy().check_table("users").is_ok());
    }
}
