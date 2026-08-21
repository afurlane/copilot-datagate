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
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogFormat {
    Pretty,
    Json,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging(
        std::env::var("LOG_FORMAT").ok().as_deref(),
        std::env::var("LOG_LEVEL").ok().as_deref(),
    )?;
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
    let policy_config = match config.effective_policy() {
        Ok(policy) => policy,
        Err(err) => {
            tracing::warn!(%err, "invalid configuration profile, starting with an empty (deny-all) policy");
            config::PolicyConfig::default()
        }
    };
    let policy_table_count = policy_config.tables.len();
    let _policy = Policy::new(policy_config);
    tracing::info!(tables = policy_table_count, "policy engine ready");

    if let Some(postgres_config) = config::PostgresConfig::from_env()? {
        let postgres_backend = PostgresBackend::connect(&postgres_config).await?;
        tracing::info!("PostgreSQL read-only connection established");
        let schema = postgres_backend.load_schema().await?;
        tracing::info!(
            tables = schema.tables.len(),
            views = schema.views.len(),
            indexes = schema.indexes.len(),
            constraints = schema.constraints.len(),
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

fn init_logging(format: Option<&str>, level: Option<&str>) -> anyhow::Result<()> {
    let format = parse_log_format(format);
    let level = level.unwrap_or("info");
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    match format {
        LogFormat::Pretty => tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .with_ansi(true)
            .try_init()
            .map_err(|err| anyhow::anyhow!(err.to_string())),
        LogFormat::Json => tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .with_target(true)
            .with_current_span(false)
            .with_span_list(false)
            .try_init()
            .map_err(|err| anyhow::anyhow!(err.to_string())),
    }
}

fn parse_log_format(format: Option<&str>) -> LogFormat {
    match format
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => LogFormat::Json,
        _ => LogFormat::Pretty,
    }
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

    #[test]
    fn defaults_to_pretty_log_format() {
        assert_eq!(parse_log_format(None), LogFormat::Pretty);
        assert_eq!(parse_log_format(Some("")), LogFormat::Pretty);
        assert_eq!(parse_log_format(Some("unknown")), LogFormat::Pretty);
    }

    #[test]
    fn parses_json_log_format_case_insensitively() {
        assert_eq!(parse_log_format(Some("json")), LogFormat::Json);
        assert_eq!(parse_log_format(Some("JSON")), LogFormat::Json);
        assert_eq!(parse_log_format(Some("  json  ")), LogFormat::Json);
    }
}
