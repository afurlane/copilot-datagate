//! MCP stdio transport backed by the policy-safe tool implementations.

use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use serde_json::Value;
#[cfg(test)]
use sqlx::sqlite::SqliteConnectOptions;

#[cfg(test)]
use crate::backend::sqlite::SqliteBackend;
use crate::backend::ReadOnlyBackend;
#[cfg(test)]
use crate::config::SqliteConfig;
use crate::mcp::{
    AggregateTool, AggregateToolRequest, SearchTool, SearchToolRequest, SelectTool,
    SelectToolRequest,
};
use crate::policy::Policy;

#[derive(Clone)]
pub struct McpServer {
    backend: Arc<dyn ReadOnlyBackend + Send + Sync>,
    policy: Policy,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {}

#[tool_router(router = tool_router)]
impl McpServer {
    pub fn new(backend: Arc<dyn ReadOnlyBackend + Send + Sync>, policy: Policy) -> Self {
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

#[cfg(test)]
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
    use std::time::{SystemTime, UNIX_EPOCH};

    use rmcp::model::CallToolRequestParams;
    use rmcp::{handler::server::wrapper::Parameters, ServiceExt};

    fn policy() -> Policy {
        let mut tables = HashMap::new();
        tables.insert(
            "users".into(),
            TableConfig {
                columns: vec!["id".into(), "email".into(), "active".into()],
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

    #[tokio::test]
    async fn invokes_select_search_and_aggregate_over_read_only_sqlite() {
        let path = std::env::temp_dir().join(format!(
            "datagate-mcp-{}-{}.db",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let setup_options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let setup_pool = sqlx::SqlitePool::connect_with(setup_options)
            .await
            .expect("setup database");
        sqlx::query("CREATE TABLE users (id INTEGER, email TEXT, active INTEGER)")
            .execute(&setup_pool)
            .await
            .expect("create users table");
        sqlx::query("INSERT INTO users (id, email, active) VALUES (1, 'alice@example.com', 1)")
            .execute(&setup_pool)
            .await
            .expect("insert user");
        setup_pool.close().await;

        let backend = connect_sqlite(path.to_string_lossy().into_owned())
            .await
            .expect("read-only backend");
        let server = McpServer::new(backend, policy());

        let select = server
            .select(Parameters(SelectToolRequest {
                request_id: "transport-select".into(),
                table: "users".into(),
                columns: vec!["id".into()],
                filters: vec![],
                limit: Some(1),
            }))
            .await;
        assert!(select.contains("transport-select"));
        assert!(select.contains("\"id\":1"));

        let search = server
            .search(Parameters(SearchToolRequest {
                request_id: "transport-search".into(),
                table: "users".into(),
                columns: vec!["id".into(), "email".into()],
                searchable_columns: vec!["email".into()],
                text: "alice".into(),
                limit: Some(1),
            }))
            .await;
        assert!(search.contains("alice@example.com"));

        let aggregate = server
            .aggregate(Parameters(AggregateToolRequest {
                request_id: "transport-aggregate".into(),
                table: "users".into(),
                operations: vec![crate::mcp::AggregateToolOperation {
                    function: crate::mcp::AggregateToolFunction::Count,
                    column: "*".into(),
                    alias: Some("total".into()),
                }],
                filters: vec![],
            }))
            .await;
        assert!(aggregate.contains("transport-aggregate"));
        assert!(aggregate.contains("total"));

        std::fs::remove_file(path).expect("remove test database");
    }

    #[tokio::test]
    async fn serves_real_mcp_client_discovery_and_policy_rejection() {
        let path = std::env::temp_dir().join(format!(
            "datagate-mcp-client-{}-{}.db",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let setup_pool = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true),
        )
        .await
        .expect("setup database");
        sqlx::query("CREATE TABLE users (id INTEGER)")
            .execute(&setup_pool)
            .await
            .expect("create users table");
        sqlx::query("INSERT INTO users (id) VALUES (7)")
            .execute(&setup_pool)
            .await
            .expect("insert user");
        setup_pool.close().await;

        let backend = connect_sqlite(path.to_string_lossy().into_owned())
            .await
            .expect("read-only backend");
        let server = McpServer::new(backend, policy());
        let (server_transport, client_transport) = tokio::io::duplex(16_384);
        tokio::spawn(async move {
            server
                .serve(server_transport)
                .await
                .expect("server should start")
                .waiting()
                .await
                .expect("server should stop cleanly");
        });

        let client = ().serve(client_transport).await.expect("client should connect");
        let tools = client
            .list_tools(Default::default())
            .await
            .expect("list tools");
        let names = tools
            .tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(names, ["aggregate", "search", "select"]);

        let result = client
            .call_tool(
                CallToolRequestParams::new("select").with_arguments(
                    serde_json::json!({
                        "request_id": "client-select",
                        "table": "secrets",
                        "columns": ["id"],
                        "limit": 1
                    })
                    .as_object()
                    .expect("object arguments")
                    .clone(),
                ),
            )
            .await
            .expect("tool call response");
        let text = result
            .content
            .first()
            .and_then(|content| content.as_text())
            .expect("text result")
            .text
            .clone();
        assert!(text.contains("policy_denied"));

        client.cancel().await.expect("client should cancel");
        std::fs::remove_file(path).expect("remove test database");
    }
}
