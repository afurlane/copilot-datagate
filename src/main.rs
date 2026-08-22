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
use config::{BackendSelector, Config, ConnectionConfig};
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliCommand {
    Run,
    McpStdio,
    Init(InitOptions),
    Doctor,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InitOptions {
    name: String,
    backend: BackendSelector,
    force: bool,
}

enum BackendConfig {
    Postgres(config::PostgresConfig),
    MySql(config::MySqlConfig),
    Sqlite(config::SqliteConfig),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = parse_cli_command(std::env::args().skip(1))?;
    let mcp_stdio_mode = command == CliCommand::McpStdio
        || std::env::var("DATAGATE_MCP_STDIO").ok().as_deref() == Some("1");
    init_logging(
        std::env::var("LOG_FORMAT").ok().as_deref(),
        std::env::var("LOG_LEVEL").ok().as_deref(),
        mcp_stdio_mode,
    )?;

    match command {
        CliCommand::Help => return print_help(),
        CliCommand::Init(options) => return run_init(options),
        CliCommand::Doctor => return run_doctor(),
        CliCommand::Run | CliCommand::McpStdio => {}
    }

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

    let connection_config = config.effective_connection()?;
    tracing::info!(connection = %connection_config.name, "connection config ready");

    let backend_selector = resolve_backend_selector(&connection_config)?;
    if mcp_stdio_mode {
        let backend = initialize_mcp_backend(backend_selector, &connection_config).await?;
        return mcp_transport::serve_stdio(mcp_transport::McpServer::new(backend, policy)).await;
    }
    initialize_backend(backend_selector, &connection_config).await?;

    Ok(())
}

fn init_metrics_snapshot() -> metrics::MetricsSnapshot {
    MetricsRegistry::new().snapshot()
}

fn init_performance_profile() -> metrics::PerformanceProfile {
    MetricsRegistry::new().performance_profile()
}

fn init_logging(
    format: Option<&str>,
    level: Option<&str>,
    mcp_stdio_mode: bool,
) -> anyhow::Result<()> {
    let format = parse_log_format(format);
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

fn parse_cli_command(args: impl IntoIterator<Item = String>) -> anyhow::Result<CliCommand> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [] => Ok(CliCommand::Run),
        [command] if command == "--help" || command == "-h" || command == "help" => {
            Ok(CliCommand::Help)
        }
        [command] if command == "doctor" => Ok(CliCommand::Doctor),
        [command, transport] if command == "mcp" && transport == "stdio" => {
            Ok(CliCommand::McpStdio)
        }
        [command, rest @ ..] if command == "init" => parse_init_options(rest),
        _ => anyhow::bail!("unknown command; run `copilot-datagate --help`"),
    }
}

fn parse_init_options(args: &[String]) -> anyhow::Result<CliCommand> {
    let mut name = "default".to_string();
    let mut backend = BackendSelector::Postgres;
    let mut force = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--name" => {
                index += 1;
                name = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("--name requires a value"))?;
            }
            "--backend" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("--backend requires a value"))?;
                backend = BackendSelector::parse(value)?;
            }
            "--force" => force = true,
            value => anyhow::bail!("unknown init option `{value}`"),
        }
        index += 1;
    }

    Ok(CliCommand::Init(InitOptions {
        name,
        backend,
        force,
    }))
}

fn print_help() -> anyhow::Result<()> {
    println!("copilot-datagate");
    println!();
    println!("Commands:");
    println!("  mcp stdio                 Start the MCP stdio server");
    println!("  init [--name N] [--backend postgres|mysql|sqlite] [--force]");
    println!("                            Create .datagate/datagate.toml without secrets");
    println!(
        "  doctor                    Inspect config and required env vars without printing secrets"
    );
    Ok(())
}

fn run_init(options: InitOptions) -> anyhow::Result<()> {
    let connection = ConnectionConfig {
        name: options.name.clone(),
        backend: Some(options.backend.as_str().to_string()),
        ..ConnectionConfig::default()
    };
    connection.validate()?;

    let project_root = std::env::var_os("DATAGATE_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let config_dir = project_root.join(".datagate");
    let config_path = config_dir.join("datagate.toml");
    if config_path.exists() && !options.force {
        anyhow::bail!(
            "configuration already exists at {}; use --force to overwrite",
            config_path.display()
        );
    }

    std::fs::create_dir_all(&config_dir)?;
    std::fs::write(
        config_path,
        initial_config_template(&connection, options.backend),
    )?;
    println!(
        "created .datagate/datagate.toml for connection `{}`",
        connection.name
    );
    Ok(())
}

fn run_doctor() -> anyhow::Result<()> {
    let config_path = resolve_config_path_from_env();
    println!("config_path={}", config_path.display());
    let config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(err) => {
            println!("config_status=error");
            println!("config_error={err}");
            println!("effective_policy=deny_all");
            return Ok(());
        }
    };
    let connection = config.effective_connection()?;
    let backend = resolve_backend_selector(&connection)?;
    let policy = config.effective_policy()?;

    println!("config_status=ok");
    println!("profile={}", config.profile.as_deref().unwrap_or("default"));
    println!("connection_name={}", connection.name);
    println!("backend={}", backend.as_str());
    for env_name in required_env_names_for_backend(backend, &connection) {
        println!(
            "env.{env_name}={}",
            if std::env::var_os(env_name).is_some() {
                "set"
            } else {
                "missing"
            }
        );
    }
    println!("policy_tables={}", policy.tables.len());
    Ok(())
}

fn resolve_backend_selector(connection: &ConnectionConfig) -> anyhow::Result<BackendSelector> {
    if std::env::var_os("DATAGATE_BACKEND").is_some() {
        return Ok(BackendSelector::from_env()?);
    }
    Ok(connection
        .backend_selector()?
        .unwrap_or(BackendSelector::Auto))
}

fn required_env_names_for_backend(
    backend: BackendSelector,
    connection: &ConnectionConfig,
) -> Vec<&str> {
    match backend {
        BackendSelector::Postgres => {
            let env_names = connection.postgres_env_names();
            if std::env::var_os(env_names.url).is_some() {
                vec![env_names.url]
            } else {
                vec![env_names.host, env_names.user, env_names.database]
            }
        }
        BackendSelector::MySql => {
            let env_names = connection.mysql_env_names();
            if std::env::var_os(env_names.url).is_some() {
                vec![env_names.url]
            } else {
                vec![env_names.host, env_names.user, env_names.database]
            }
        }
        BackendSelector::Sqlite => {
            let env_names = connection.sqlite_env_names();
            vec![env_names.url, env_names.path]
        }
        BackendSelector::Auto => Vec::new(),
    }
}

fn initial_config_template(connection: &ConnectionConfig, backend: BackendSelector) -> String {
    let backend = backend.as_str();
    let env_line = if backend == "sqlite" {
        "path_env = \"SQLITE_PATH\"".to_string()
    } else {
        format!("url_env = \"{}\"", default_url_env_for_backend(backend))
    };
    format!(
        r#"profile = "dev"

[profiles.dev.connection]
name = "{}"
backend = "{}"
{}

[profiles.dev.policy]
default_row_limit = 100
max_row_limit = 1000
max_query_complexity = 100
max_output_bytes = 131072

# Add allowed tables and columns explicitly. Everything else is denied.
#[profiles.dev.policy.tables.example]
#columns = ["id"]
"#,
        connection.name, backend, env_line,
    )
}

fn default_url_env_for_backend(backend: &str) -> &'static str {
    match backend {
        "mysql" => "MYSQL_URL",
        "sqlite" => "SQLITE_URL",
        _ => "DB_URL",
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

async fn initialize_backend(
    selector: BackendSelector,
    connection: &ConnectionConfig,
) -> anyhow::Result<()> {
    if let Some(config) = resolve_backend_config(selector, connection)? {
        match config {
            BackendConfig::Postgres(config) => initialize_postgres(config).await?,
            BackendConfig::MySql(config) => initialize_mysql(config).await?,
            BackendConfig::Sqlite(config) => initialize_sqlite(config).await?,
        }
    }

    Ok(())
}

async fn initialize_mcp_backend(
    selector: BackendSelector,
    connection: &ConnectionConfig,
) -> anyhow::Result<Arc<dyn ReadOnlyBackend + Send + Sync>> {
    match resolve_backend_config(selector, connection)?.ok_or_else(|| {
        anyhow::anyhow!("DATAGATE_MCP_STDIO=1 requires database environment configuration")
    })? {
        BackendConfig::Postgres(config) => Ok(Arc::new(PostgresBackend::connect(&config).await?)),
        BackendConfig::MySql(config) => Ok(Arc::new(MySqlBackend::connect(&config).await?)),
        BackendConfig::Sqlite(config) => Ok(Arc::new(SqliteBackend::connect(&config).await?)),
    }
}

fn resolve_backend_config(
    selector: BackendSelector,
    connection: &ConnectionConfig,
) -> anyhow::Result<Option<BackendConfig>> {
    match selector {
        BackendSelector::Auto => {
            if let Some(config) = config::PostgresConfig::from_connection_env(connection)? {
                Ok(Some(BackendConfig::Postgres(config)))
            } else if let Some(config) = config::MySqlConfig::from_connection_env(connection)? {
                Ok(Some(BackendConfig::MySql(config)))
            } else {
                Ok(config::SqliteConfig::from_connection_env(connection)?
                    .map(BackendConfig::Sqlite))
            }
        }
        BackendSelector::Postgres => config::PostgresConfig::from_connection_env(connection)?
            .map(BackendConfig::Postgres)
            .ok_or_else(|| anyhow::anyhow!(missing_backend_config_message(selector)))
            .map(Some),
        BackendSelector::MySql => config::MySqlConfig::from_connection_env(connection)?
            .map(BackendConfig::MySql)
            .ok_or_else(|| anyhow::anyhow!(missing_backend_config_message(selector)))
            .map(Some),
        BackendSelector::Sqlite => config::SqliteConfig::from_connection_env(connection)?
            .map(BackendConfig::Sqlite)
            .ok_or_else(|| anyhow::anyhow!(missing_backend_config_message(selector)))
            .map(Some),
    }
}

fn missing_backend_config_message(selector: BackendSelector) -> &'static str {
    match selector {
        BackendSelector::Postgres => {
            "DATAGATE_BACKEND=postgres but no PostgreSQL environment configuration was found"
        }
        BackendSelector::MySql => {
            "DATAGATE_BACKEND=mysql but no MySQL/MariaDB environment configuration was found"
        }
        BackendSelector::Sqlite => {
            "DATAGATE_BACKEND=sqlite but no SQLite environment configuration was found"
        }
        BackendSelector::Auto => "no database environment configuration was found",
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
    fn parses_mcp_stdio_command() {
        assert_eq!(
            parse_cli_command(["mcp".to_string(), "stdio".to_string()]).unwrap(),
            CliCommand::McpStdio
        );
    }

    #[test]
    fn parses_init_command_with_name_backend_and_force() {
        assert_eq!(
            parse_cli_command([
                "init".to_string(),
                "--name".to_string(),
                "pippo".to_string(),
                "--backend".to_string(),
                "sqlite".to_string(),
                "--force".to_string(),
            ])
            .unwrap(),
            CliCommand::Init(InitOptions {
                name: "pippo".to_string(),
                backend: BackendSelector::Sqlite,
                force: true,
            })
        );
    }

    #[test]
    fn init_template_records_env_name_without_secret_value() {
        let config = ConnectionConfig {
            name: "pippo".to_string(),
            backend: Some("postgres".to_string()),
            ..ConnectionConfig::default()
        };
        let template = initial_config_template(&config, BackendSelector::Postgres);
        assert!(template.contains("name = \"pippo\""));
        assert!(template.contains("backend = \"postgres\""));
        assert!(template.contains("url_env = \"DB_URL\""));
        assert!(!template.contains("postgresql://"));
        assert!(!template.contains("password"));
    }

    #[test]
    fn parses_json_log_format_case_insensitively() {
        assert_eq!(parse_log_format(Some("json")), LogFormat::Json);
        assert_eq!(parse_log_format(Some("JSON")), LogFormat::Json);
        assert_eq!(parse_log_format(Some("  json  ")), LogFormat::Json);
    }
}
