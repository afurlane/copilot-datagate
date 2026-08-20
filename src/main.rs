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

    let metrics_snapshot = init_metrics_snapshot();
    tracing::debug!(
        requests_total = metrics_snapshot.requests_total,
        "metrics registry initialized"
    );

    let _rate_limiter = build_rate_limiter(
        std::env::var("RATE_LIMIT_REQUESTS").ok().as_deref(),
        std::env::var("RATE_LIMIT_WINDOW_SECS").ok().as_deref(),
    );

    if let Ok(path) = std::env::var("AUDIT_LOG_PATH") {
        AuditLogger::new(path)
            .record(&AuditEvent::new("startup", "process_start"))
            .await?;
    }

    let config_path = resolve_config_path(std::env::var("DATAGATE_CONFIG").ok());
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

fn init_metrics_snapshot() -> metrics::MetricsSnapshot {
    MetricsRegistry::new().snapshot()
}

fn resolve_config_path(override_path: Option<String>) -> String {
    override_path.unwrap_or_else(|| "datagate.toml".to_string())
}

fn build_rate_limiter(
    max_requests: Option<&str>,
    window_secs: Option<&str>,
) -> Option<RateLimiter> {
    let max_requests = max_requests?.parse().ok()?;
    let window_secs = window_secs?.parse().ok()?;
    RateLimiter::new(max_requests, std::time::Duration::from_secs(window_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_default_config_path_when_override_missing() {
        assert_eq!(resolve_config_path(None), "datagate.toml");
    }

    #[test]
    fn uses_config_path_override_when_provided() {
        assert_eq!(
            resolve_config_path(Some("custom.toml".to_string())),
            "custom.toml"
        );
    }

    #[test]
    fn builds_rate_limiter_for_valid_values() {
        assert!(build_rate_limiter(Some("10"), Some("60")).is_some());
    }

    #[test]
    fn does_not_build_rate_limiter_for_invalid_values() {
        assert!(build_rate_limiter(None, Some("60")).is_none());
        assert!(build_rate_limiter(Some("10"), None).is_none());
        assert!(build_rate_limiter(Some("x"), Some("60")).is_none());
        assert!(build_rate_limiter(Some("10"), Some("0")).is_none());
    }

    #[test]
    fn initializes_empty_metrics_snapshot() {
        let snapshot = init_metrics_snapshot();
        assert_eq!(snapshot.requests_total, 0);
        assert_eq!(snapshot.requests_accepted, 0);
        assert_eq!(snapshot.requests_rejected, 0);
        assert_eq!(snapshot.backend_errors, 0);
        assert_eq!(snapshot.p95_latency_ms, None);
        assert_eq!(snapshot.pool, None);
    }
}
