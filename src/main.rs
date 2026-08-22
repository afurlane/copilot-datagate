//! DataGate entrypoint. Real wiring (schema loader, backends, MCP tools)
//! will be added incrementally per the roadmap.

mod audit;
mod backend;
mod config;
#[allow(dead_code)]
mod error;
#[allow(dead_code)]
mod mcp;
mod mcp_transport;
mod metrics;
mod policy;
#[allow(dead_code)]
mod query;
mod rate_limit;
mod schema;

use audit::{AuditEvent, AuditLogger};
use backend::mysql::MySqlBackend;
use backend::postgres::PostgresBackend;
use backend::sqlite::SqliteBackend;
use backend::ReadOnlyBackend;
use config::{BackendSelector, Config};
use metrics::MetricsRegistry;
use policy::Policy;
use rate_limit::RateLimiter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    let performance_profile = init_performance_profile();
    tracing::debug!(
        samples = performance_profile.samples,
        p50_latency_ms = ?performance_profile.p50_latency_ms,
        p95_latency_ms = ?performance_profile.p95_latency_ms,
        p99_latency_ms = ?performance_profile.p99_latency_ms,
        avg_latency_ms = ?performance_profile.avg_latency_ms,
        "performance profile initialized"
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

    let config_path = resolve_config_path_from_env();
    let config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(err) => {
            tracing::warn!(%err, path = %config_path.display(), "no valid config found, starting with an empty (deny-all) policy");
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
    let policy = Policy::new(policy_config);
    tracing::info!(tables = policy_table_count, "policy engine ready");

    let backend_selector = BackendSelector::from_env()?;
    if std::env::var("DATAGATE_MCP_STDIO").ok().as_deref() == Some("1") {
        let backend = initialize_mcp_backend(backend_selector).await?;
        return mcp_transport::serve_stdio(mcp_transport::McpServer::new(backend, policy)).await;
    }
    initialize_backend(backend_selector).await?;

    Ok(())
}

fn init_metrics_snapshot() -> metrics::MetricsSnapshot {
    MetricsRegistry::new().snapshot()
}

fn init_performance_profile() -> metrics::PerformanceProfile {
    MetricsRegistry::new().performance_profile()
}

fn init_logging(format: Option<&str>, level: Option<&str>) -> anyhow::Result<()> {
    let format = parse_log_format(format);
    let mcp_stdio_mode = std::env::var("DATAGATE_MCP_STDIO").ok().as_deref() == Some("1");
    let level = level.unwrap_or(if mcp_stdio_mode { "warn" } else { "info" });
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let ansi_enabled = !mcp_stdio_mode;

    match format {
        LogFormat::Pretty => tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .with_ansi(ansi_enabled)
            .with_writer(std::io::stderr)
            .try_init()
            .map_err(|err| anyhow::anyhow!(err.to_string())),
        LogFormat::Json => tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .with_target(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_writer(std::io::stderr)
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

fn resolve_config_path_from_env() -> PathBuf {
    let override_path = std::env::var_os("DATAGATE_CONFIG").map(PathBuf::from);
    let project_root = std::env::var_os("DATAGATE_PROJECT_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    let user_home = user_home_dir();
    resolve_config_path(override_path, project_root, user_home, Path::exists)
}

fn resolve_config_path(
    override_path: Option<PathBuf>,
    project_root: Option<PathBuf>,
    user_home: Option<PathBuf>,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    if let Some(path) = override_path {
        return path;
    }

    if let Some(root) = project_root {
        for candidate in [
            root.join(".datagate").join("datagate.toml"),
            root.join("datagate.toml"),
        ] {
            if exists(&candidate) {
                return candidate;
            }
        }
    }

    if let Some(home) = user_home {
        return home.join(".config").join("datagate").join("config.toml");
    }

    PathBuf::from("datagate.toml")
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn build_rate_limiter(
    max_requests: Option<&str>,
    window_secs: Option<&str>,
) -> Option<RateLimiter> {
    let max_requests = max_requests?.parse().ok()?;
    let window_secs = window_secs?.parse().ok()?;
    RateLimiter::new(max_requests, std::time::Duration::from_secs(window_secs))
}

async fn initialize_backend(selector: BackendSelector) -> anyhow::Result<()> {
    match selector {
        BackendSelector::Auto => {
            if let Some(postgres_config) = config::PostgresConfig::from_env()? {
                initialize_postgres(postgres_config).await?;
            } else if let Some(mysql_config) = config::MySqlConfig::from_env()? {
                initialize_mysql(mysql_config).await?;
            } else if let Some(sqlite_config) = config::SqliteConfig::from_env()? {
                initialize_sqlite(sqlite_config).await?;
            }
        }
        BackendSelector::Postgres => {
            let postgres_config = config::PostgresConfig::from_env()?.ok_or_else(|| {
                anyhow::anyhow!(
                    "DATAGATE_BACKEND=postgres but no PostgreSQL environment configuration was found"
                )
            })?;
            initialize_postgres(postgres_config).await?;
        }
        BackendSelector::MySql => {
            let mysql_config = config::MySqlConfig::from_env()?.ok_or_else(|| {
                anyhow::anyhow!(
                    "DATAGATE_BACKEND=mysql but no MySQL/MariaDB environment configuration was found"
                )
            })?;
            initialize_mysql(mysql_config).await?;
        }
        BackendSelector::Sqlite => {
            let sqlite_config = config::SqliteConfig::from_env()?.ok_or_else(|| {
                anyhow::anyhow!(
                    "DATAGATE_BACKEND=sqlite but no SQLite environment configuration was found"
                )
            })?;
            initialize_sqlite(sqlite_config).await?;
        }
    }

    Ok(())
}

async fn initialize_mcp_backend(
    selector: BackendSelector,
) -> anyhow::Result<Arc<dyn ReadOnlyBackend + Send + Sync>> {
    match selector {
        BackendSelector::Auto => {
            if let Some(postgres_config) = config::PostgresConfig::from_env()? {
                Ok(Arc::new(PostgresBackend::connect(&postgres_config).await?))
            } else if let Some(mysql_config) = config::MySqlConfig::from_env()? {
                Ok(Arc::new(MySqlBackend::connect(&mysql_config).await?))
            } else if let Some(sqlite_config) = config::SqliteConfig::from_env()? {
                Ok(Arc::new(SqliteBackend::connect(&sqlite_config).await?))
            } else {
                anyhow::bail!("DATAGATE_MCP_STDIO=1 requires database environment configuration");
            }
        }
        BackendSelector::Postgres => {
            let postgres_config = config::PostgresConfig::from_env()?.ok_or_else(|| {
                anyhow::anyhow!(
                    "DATAGATE_BACKEND=postgres but no PostgreSQL environment configuration was found"
                )
            })?;
            Ok(Arc::new(PostgresBackend::connect(&postgres_config).await?))
        }
        BackendSelector::MySql => {
            let mysql_config = config::MySqlConfig::from_env()?.ok_or_else(|| {
                anyhow::anyhow!(
                    "DATAGATE_BACKEND=mysql but no MySQL/MariaDB environment configuration was found"
                )
            })?;
            Ok(Arc::new(MySqlBackend::connect(&mysql_config).await?))
        }
        BackendSelector::Sqlite => {
            let sqlite_config = config::SqliteConfig::from_env()?.ok_or_else(|| {
                anyhow::anyhow!(
                    "DATAGATE_BACKEND=sqlite but no SQLite environment configuration was found"
                )
            })?;
            Ok(Arc::new(SqliteBackend::connect(&sqlite_config).await?))
        }
    }
}

async fn initialize_postgres(config: config::PostgresConfig) -> anyhow::Result<()> {
    let postgres_backend = PostgresBackend::connect(&config).await?;
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
    Ok(())
}

async fn initialize_mysql(config: config::MySqlConfig) -> anyhow::Result<()> {
    let _mysql_backend = MySqlBackend::connect(&config).await?;
    tracing::info!("MySQL/MariaDB read-only connection established");
    Ok(())
}

async fn initialize_sqlite(config: config::SqliteConfig) -> anyhow::Result<()> {
    let _sqlite_backend = SqliteBackend::connect(&config).await?;
    tracing::info!("SQLite read-only connection established");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_override_has_highest_precedence() {
        let path = resolve_config_path(
            Some(PathBuf::from("/explicit/datagate.toml")),
            Some(PathBuf::from("/workspace")),
            Some(PathBuf::from("/home/alice")),
            |_| true,
        );
        assert_eq!(path, PathBuf::from("/explicit/datagate.toml"));
    }

    #[test]
    fn resolves_project_dot_datagate_config_before_project_root_config() {
        let path = resolve_config_path(
            None,
            Some(PathBuf::from("/workspace")),
            Some(PathBuf::from("/home/alice")),
            |path| {
                matches!(
                    path.to_str(),
                    Some("/workspace/.datagate/datagate.toml") | Some("/workspace/datagate.toml")
                )
            },
        );
        assert_eq!(path, PathBuf::from("/workspace/.datagate/datagate.toml"));
    }

    #[test]
    fn resolves_project_root_config_before_user_config() {
        let path = resolve_config_path(
            None,
            Some(PathBuf::from("/workspace")),
            Some(PathBuf::from("/home/alice")),
            |path| matches!(path.to_str(), Some("/workspace/datagate.toml")),
        );
        assert_eq!(path, PathBuf::from("/workspace/datagate.toml"));
    }

    #[test]
    fn falls_back_to_user_config_when_project_configs_are_missing() {
        let path = resolve_config_path(
            None,
            Some(PathBuf::from("/workspace")),
            Some(PathBuf::from("/home/alice")),
            |_| false,
        );
        assert_eq!(
            path,
            PathBuf::from("/home/alice/.config/datagate/config.toml")
        );
    }

    #[test]
    fn falls_back_to_legacy_relative_config_without_project_or_home() {
        let path = resolve_config_path(None, None, None, |_| false);
        assert_eq!(path, PathBuf::from("datagate.toml"));
    }

    #[test]
    fn resolves_home_from_unix_or_windows_environment() {
        let home = user_home_dir();
        if cfg!(windows) {
            assert!(home.is_some() || std::env::var_os("USERPROFILE").is_none());
        } else {
            assert!(home.is_some() || std::env::var_os("HOME").is_none());
        }
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
    fn initializes_empty_performance_profile() {
        let profile = init_performance_profile();
        assert_eq!(profile.samples, 0);
        assert_eq!(profile.min_latency_ms, None);
        assert_eq!(profile.avg_latency_ms, None);
        assert_eq!(profile.p50_latency_ms, None);
        assert_eq!(profile.p95_latency_ms, None);
        assert_eq!(profile.p99_latency_ms, None);
        assert_eq!(profile.max_latency_ms, None);
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
