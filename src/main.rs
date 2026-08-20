//! DataGate entrypoint. Real wiring (schema loader, backends, MCP tools)
//! will be added incrementally per the roadmap.

mod audit;
mod backend;
mod config;
#[allow(dead_code)]
mod error;
#[allow(dead_code)]
mod mcp;
mod metrics;
mod policy;
#[allow(dead_code)]
mod query;
mod rate_limit;
mod schema;

use audit::{AuditEvent, AuditLogger};
use backend::postgres::PostgresBackend;
use config::Config;
use metrics::MetricsRegistry;
use policy::Policy;
use rate_limit::RateLimiter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("DataGate starting (skeleton build, no backend wired yet)");

    let metrics = MetricsRegistry::new();
    let metrics_snapshot = metrics.snapshot();
    tracing::debug!(
        requests_total = metrics_snapshot.requests_total,
        "metrics registry initialized"
    );

    let _rate_limiter = match (
        std::env::var("RATE_LIMIT_REQUESTS"),
        std::env::var("RATE_LIMIT_WINDOW_SECS"),
    ) {
        (Ok(max_requests), Ok(window_secs)) => match (max_requests.parse(), window_secs.parse()) {
            (Ok(max_requests), Ok(window_secs)) => {
                RateLimiter::new(max_requests, std::time::Duration::from_secs(window_secs))
            }
            _ => None,
        },
        _ => None,
    };

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
