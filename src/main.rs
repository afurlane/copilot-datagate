//! DataGate entrypoint. Real wiring (schema loader, backends, MCP tools)
//! will be added incrementally per the roadmap.

mod audit;
mod backend;
mod config;
#[allow(dead_code)]
mod error;
#[allow(dead_code)]
mod mcp;
mod policy;
#[allow(dead_code)]
mod query;
mod schema;

use audit::{AuditEvent, AuditLogger};
use backend::postgres::PostgresBackend;
use config::Config;
use policy::Policy;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("DataGate starting (skeleton build, no backend wired yet)");

    if let Ok(path) = std::env::var("AUDIT_LOG_PATH") {
        AuditLogger::new(path)
            .record(&AuditEvent::new("startup", "process_start"))
            .await?;
    }

    let config_path =
        std::env::var("DATAGATE_CONFIG").unwrap_or_else(|_| "datagate.toml".to_string());
    let config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(err) => {
            tracing::warn!(%err, path = %config_path, "no valid config found, starting with an empty (deny-all) policy");
            Config::default()
        }
    };
    let policy_table_count = config.policy.tables.len();
    let _policy = Policy::new(config.policy);
    tracing::info!(tables = policy_table_count, "policy engine ready");

    if let Some(postgres_config) = config::PostgresConfig::from_env()? {
        let postgres_backend = PostgresBackend::connect(&postgres_config).await?;
        tracing::info!("PostgreSQL read-only connection established");
        let schema = postgres_backend.load_schema().await?;
        tracing::info!(
            tables = schema.tables.len(),
            views = schema.views.len(),
            indexes = schema.indexes.len(),
            triggers = schema.triggers.len(),
            routines = schema.routines.len(),
            sequences = schema.sequences.len(),
            "database schema loaded"
        );
    }

    Ok(())
}
