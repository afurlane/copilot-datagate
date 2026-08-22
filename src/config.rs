//! Configuration loading (profiles, backends, policy limits).

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sqlx::mysql::MySqlConnectOptions;
use sqlx::postgres::PgConnectOptions;
use sqlx::sqlite::SqliteConnectOptions;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("unknown configuration profile `{0}`")]
    UnknownProfile(String),
    #[error("invalid connection name `{0}`")]
    InvalidConnectionName(String),
    #[error("invalid or missing database environment variable `{variable}`")]
    InvalidEnvironment { variable: &'static str },
    #[error("invalid or missing database environment variable `{variable}`")]
    InvalidConnectionEnvironment { variable: String },
    #[error("invalid backend selector `{value}` in environment variable `DATAGATE_BACKEND`")]
    InvalidBackendSelector { value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendSelector {
    Auto,
    Postgres,
    MySql,
    Sqlite,
}

impl BackendSelector {
    pub fn from_env() -> Result<Self, ConfigError> {
        match optional_env("DATAGATE_BACKEND")?.as_deref() {
            None | Some("auto") => Ok(Self::Auto),
            Some(value) => Self::parse(value),
        }
    }

    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "postgres" => Ok(Self::Postgres),
            "mysql" | "mariadb" => Ok(Self::MySql),
            "sqlite" => Ok(Self::Sqlite),
            value => Err(ConfigError::InvalidBackendSelector {
                value: value.to_string(),
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Postgres => "postgres",
            Self::MySql => "mysql",
            Self::Sqlite => "sqlite",
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Config {
    pub profile: Option<String>,
    #[serde(default)]
    pub connection: Option<ConnectionConfig>,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ProfileConfig {
    #[serde(default)]
    pub connection: Option<ConnectionConfig>,
    #[serde(default)]
    pub policy: PolicyConfig,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct ConnectionConfig {
    #[serde(default = "default_connection_name")]
    pub name: String,
    pub backend: Option<String>,
    pub url_env: Option<String>,
    pub options_env: Option<String>,
    pub host_env: Option<String>,
    pub port_env: Option<String>,
    pub user_env: Option<String>,
    pub password_env: Option<String>,
    pub database_env: Option<String>,
    pub path_env: Option<String>,
    pub max_connections_env: Option<String>,
    pub acquire_timeout_secs_env: Option<String>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            name: default_connection_name(),
            backend: None,
            url_env: None,
            options_env: None,
            host_env: None,
            port_env: None,
            user_env: None,
            password_env: None,
            database_env: None,
            path_env: None,
            max_connections_env: None,
            acquire_timeout_secs_env: None,
        }
    }
}

impl ConnectionConfig {
    pub fn backend_selector(&self) -> Result<Option<BackendSelector>, ConfigError> {
        self.backend
            .as_deref()
            .map(BackendSelector::parse)
            .transpose()
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty()
            || self.name.len() > 64
            || !self
                .name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        {
            return Err(ConfigError::InvalidConnectionName(self.name.clone()));
        }
        Ok(())
    }

    pub fn postgres_env_names(&self) -> RelationalEnvNames<'_> {
        RelationalEnvNames {
            url: env_name(self.url_env.as_deref(), "DB_URL"),
            options: Some(env_name(self.options_env.as_deref(), "DB_OPTIONS")),
            host: env_name(self.host_env.as_deref(), "DB_HOST"),
            port: env_name(self.port_env.as_deref(), "DB_PORT"),
            user: env_name(self.user_env.as_deref(), "DB_USER"),
            password: env_name(self.password_env.as_deref(), "DB_PASSWORD"),
            database: env_name(self.database_env.as_deref(), "DB_NAME"),
            max_connections: env_name(self.max_connections_env.as_deref(), "DB_MAX_CONNECTIONS"),
            acquire_timeout_secs: env_name(
                self.acquire_timeout_secs_env.as_deref(),
                "DB_ACQUIRE_TIMEOUT_SECS",
            ),
        }
    }

    pub fn mysql_env_names(&self) -> RelationalEnvNames<'_> {
        RelationalEnvNames {
            url: env_name(self.url_env.as_deref(), "MYSQL_URL"),
            options: None,
            host: env_name(self.host_env.as_deref(), "MYSQL_HOST"),
            port: env_name(self.port_env.as_deref(), "MYSQL_PORT"),
            user: env_name(self.user_env.as_deref(), "MYSQL_USER"),
            password: env_name(self.password_env.as_deref(), "MYSQL_PASSWORD"),
            database: env_name(self.database_env.as_deref(), "MYSQL_DATABASE"),
            max_connections: env_name(self.max_connections_env.as_deref(), "MYSQL_MAX_CONNECTIONS"),
            acquire_timeout_secs: env_name(
                self.acquire_timeout_secs_env.as_deref(),
                "MYSQL_ACQUIRE_TIMEOUT_SECS",
            ),
        }
    }

    pub fn sqlite_env_names(&self) -> SqliteEnvNames<'_> {
        SqliteEnvNames {
            url: env_name(self.url_env.as_deref(), "SQLITE_URL"),
            path: env_name(self.path_env.as_deref(), "SQLITE_PATH"),
            max_connections: env_name(
                self.max_connections_env.as_deref(),
                "SQLITE_MAX_CONNECTIONS",
            ),
            acquire_timeout_secs: env_name(
                self.acquire_timeout_secs_env.as_deref(),
                "SQLITE_ACQUIRE_TIMEOUT_SECS",
            ),
        }
    }
}

pub struct RelationalEnvNames<'a> {
    pub url: &'a str,
    pub options: Option<&'a str>,
    pub host: &'a str,
    pub port: &'a str,
    pub user: &'a str,
    pub password: &'a str,
    pub database: &'a str,
    pub max_connections: &'a str,
    pub acquire_timeout_secs: &'a str,
}

pub struct SqliteEnvNames<'a> {
    pub url: &'a str,
    pub path: &'a str,
    pub max_connections: &'a str,
    pub acquire_timeout_secs: &'a str,
}

struct ComponentEnvNames<'a> {
    host: &'a str,
    port: &'a str,
    user: &'a str,
    password: &'a str,
    database: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
struct ComponentConnection {
    host: String,
    port: u16,
    user: String,
    password: Option<String>,
    database: String,
}

#[derive(Debug, PartialEq, Eq)]
enum ConnectionEnvSource {
    Missing,
    Url(String),
    Components,
}

#[derive(Debug, PartialEq, Eq)]
struct PoolSettings {
    max_connections: u32,
    acquire_timeout_secs: u64,
}

impl Config {
    /// Loads and parses a TOML config file. Missing file or invalid TOML is a hard error:
    /// there is no implicit "allow everything" fallback for policy-relevant config.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(toml::from_str(&raw)?)
    }

    pub fn effective_policy(&self) -> Result<PolicyConfig, ConfigError> {
        match self.profile.as_deref() {
            Some(profile) if !self.profiles.is_empty() => self
                .profiles
                .get(profile)
                .map(|config| config.policy.clone())
                .ok_or_else(|| ConfigError::UnknownProfile(profile.to_string())),
            _ => Ok(self.policy.clone()),
        }
    }

    pub fn effective_connection(&self) -> Result<ConnectionConfig, ConfigError> {
        let connection = match self.profile.as_deref() {
            Some(profile) if !self.profiles.is_empty() => self
                .profiles
                .get(profile)
                .map(|config| config.connection.clone().unwrap_or_default())
                .ok_or_else(|| ConfigError::UnknownProfile(profile.to_string()))?,
            _ => self.connection.clone().unwrap_or_default(),
        };
        connection.validate()?;
        Ok(connection)
    }
}

/// PostgreSQL pool configuration sourced only from environment variables.
/// The configured database role must itself be read-only; DataGate also enforces
/// a read-only default for every pool session.
#[derive(Clone)]
pub struct PostgresConfig {
    pub connect_options: PgConnectOptions,
    pub max_connections: u32,
    pub acquire_timeout_secs: u64,
}

impl PostgresConfig {
    #[allow(dead_code)]
    pub fn from_env() -> Result<Option<Self>, ConfigError> {
        Self::from_connection_env(&ConnectionConfig::default())
    }

    pub fn from_connection_env(connection: &ConnectionConfig) -> Result<Option<Self>, ConfigError> {
        let env_names = connection.postgres_env_names();
        Self::from_env_names(&env_names)
    }

    fn from_env_names(env_names: &RelationalEnvNames<'_>) -> Result<Option<Self>, ConfigError> {
        let component_envs = ComponentEnvNames {
            host: env_names.host,
            port: env_names.port,
            user: env_names.user,
            password: env_names.password,
            database: env_names.database,
        };
        let mut connect_options = match connection_env_source(env_names.url, &component_envs)? {
            ConnectionEnvSource::Missing => return Ok(None),
            ConnectionEnvSource::Url(url) => PgConnectOptions::from_str(&url)
                .map_err(|_| invalid_connection_env(env_names.url))?,
            ConnectionEnvSource::Components => {
                let component = read_component_connection(&component_envs, 5432)?;
                let mut options = PgConnectOptions::new()
                    .host(&component.host)
                    .port(component.port)
                    .username(&component.user)
                    .database(&component.database);
                if let Some(password) = component.password {
                    options = options.password(&password);
                }
                options
            }
        };

        if let Some(options_env) = env_names.options {
            if let Some(raw_options) = optional_named_env(options_env)? {
                connect_options = connect_options.options(parse_db_options(&raw_options)?);
            }
        }

        let pool = read_pool_settings(env_names.max_connections, env_names.acquire_timeout_secs)?;
        Ok(Some(Self {
            connect_options,
            max_connections: pool.max_connections,
            acquire_timeout_secs: pool.acquire_timeout_secs,
        }))
    }
}

impl std::fmt::Debug for PostgresConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresConfig")
            .field("connect_options", &"[redacted]")
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout_secs", &self.acquire_timeout_secs)
            .finish()
    }
}

/// MySQL/MariaDB pool configuration sourced only from environment variables.
/// The configured database role must itself be read-only; DataGate also enforces
/// a read-only default for every pool session.
#[derive(Clone)]
pub struct MySqlConfig {
    pub connect_options: MySqlConnectOptions,
    pub max_connections: u32,
    pub acquire_timeout_secs: u64,
}

impl MySqlConfig {
    #[allow(dead_code)]
    pub fn from_env() -> Result<Option<Self>, ConfigError> {
        Self::from_connection_env(&ConnectionConfig::default())
    }

    pub fn from_connection_env(connection: &ConnectionConfig) -> Result<Option<Self>, ConfigError> {
        let env_names = connection.mysql_env_names();
        let component_envs = ComponentEnvNames {
            host: env_names.host,
            port: env_names.port,
            user: env_names.user,
            password: env_names.password,
            database: env_names.database,
        };
        let connect_options = match connection_env_source(env_names.url, &component_envs)? {
            ConnectionEnvSource::Missing => return Ok(None),
            ConnectionEnvSource::Url(url) => MySqlConnectOptions::from_str(&url)
                .map_err(|_| invalid_connection_env(env_names.url))?,
            ConnectionEnvSource::Components => {
                let component = read_component_connection(&component_envs, 3306)?;
                let mut options = MySqlConnectOptions::new()
                    .host(&component.host)
                    .port(component.port)
                    .username(&component.user)
                    .database(&component.database);
                if let Some(password) = component.password {
                    options = options.password(&password);
                }
                options
            }
        };

        let pool = read_pool_settings(env_names.max_connections, env_names.acquire_timeout_secs)?;
        Ok(Some(Self {
            connect_options,
            max_connections: pool.max_connections,
            acquire_timeout_secs: pool.acquire_timeout_secs,
        }))
    }
}

impl std::fmt::Debug for MySqlConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MySqlConfig")
            .field("connect_options", &"[redacted]")
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout_secs", &self.acquire_timeout_secs)
            .finish()
    }
}

/// SQLite pool configuration sourced only from environment variables.
///
/// This backend is intended for local/testing scenarios. Connections are opened
/// read-only and query execution remains constrained by DataGate query plans.
#[derive(Clone)]
pub struct SqliteConfig {
    pub connect_options: SqliteConnectOptions,
    pub max_connections: u32,
    pub acquire_timeout_secs: u64,
}

impl SqliteConfig {
    #[allow(dead_code)]
    pub fn from_env() -> Result<Option<Self>, ConfigError> {
        Self::from_connection_env(&ConnectionConfig::default())
    }

    pub fn from_connection_env(connection: &ConnectionConfig) -> Result<Option<Self>, ConfigError> {
        let env_names = connection.sqlite_env_names();
        let url = optional_named_env(env_names.url)?;
        let path = optional_named_env(env_names.path)?;
        if url.is_none() && path.is_none() {
            return Ok(None);
        }

        let connect_options = if let Some(url) = url {
            SqliteConnectOptions::from_str(&url)
                .map_err(|_| invalid_connection_env(env_names.url))?
        } else {
            SqliteConnectOptions::new().filename(path.expect("checked above"))
        }
        .read_only(true)
        .create_if_missing(false);

        Ok(Some(Self {
            connect_options,
            max_connections: named_env_u32_or_default(
                env_names.max_connections,
                default_max_connections(),
            )?,
            acquire_timeout_secs: named_env_u64_or_default(
                env_names.acquire_timeout_secs,
                default_acquire_timeout_secs(),
            )?,
        }))
    }
}

impl std::fmt::Debug for SqliteConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqliteConfig")
            .field("connect_options", &"[redacted]")
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout_secs", &self.acquire_timeout_secs)
            .finish()
    }
}

fn optional_env(variable: &'static str) -> Result<Option<String>, ConfigError> {
    match env::var(variable) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) => Err(ConfigError::InvalidEnvironment { variable }),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidEnvironment { variable }),
    }
}

fn required_named_env(variable: &str) -> Result<String, ConfigError> {
    optional_named_env(variable)?.ok_or_else(|| invalid_connection_env(variable))
}

fn optional_named_env(variable: &str) -> Result<Option<String>, ConfigError> {
    match env::var(variable) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) => Err(invalid_connection_env(variable)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(invalid_connection_env(variable)),
    }
}

fn connection_env_source(
    url_env: &str,
    component_envs: &ComponentEnvNames<'_>,
) -> Result<ConnectionEnvSource, ConfigError> {
    if let Some(url) = optional_named_env(url_env)? {
        return Ok(ConnectionEnvSource::Url(url));
    }
    if [
        component_envs.host,
        component_envs.port,
        component_envs.user,
        component_envs.password,
        component_envs.database,
    ]
    .iter()
    .any(|name| env::var_os(name).is_some())
    {
        return Ok(ConnectionEnvSource::Components);
    }
    Ok(ConnectionEnvSource::Missing)
}

fn read_component_connection(
    env_names: &ComponentEnvNames<'_>,
    default_port: u16,
) -> Result<ComponentConnection, ConfigError> {
    Ok(ComponentConnection {
        host: required_named_env(env_names.host)?,
        port: read_named_port(env_names.port, default_port)?,
        user: required_named_env(env_names.user)?,
        password: optional_named_env(env_names.password)?,
        database: required_named_env(env_names.database)?,
    })
}

fn read_named_port(variable: &str, default: u16) -> Result<u16, ConfigError> {
    optional_named_env(variable)?
        .map(|value| value.parse().map_err(|_| invalid_connection_env(variable)))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn read_pool_settings(
    max_connections_env: &str,
    acquire_timeout_secs_env: &str,
) -> Result<PoolSettings, ConfigError> {
    Ok(PoolSettings {
        max_connections: named_env_u32_or_default(max_connections_env, default_max_connections())?,
        acquire_timeout_secs: named_env_u64_or_default(
            acquire_timeout_secs_env,
            default_acquire_timeout_secs(),
        )?,
    })
}

fn named_env_u32_or_default(variable: &str, default: u32) -> Result<u32, ConfigError> {
    optional_named_env(variable)?
        .map(|value| value.parse().map_err(|_| invalid_connection_env(variable)))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn named_env_u64_or_default(variable: &str, default: u64) -> Result<u64, ConfigError> {
    optional_named_env(variable)?
        .map(|value| value.parse().map_err(|_| invalid_connection_env(variable)))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_db_options(raw: &str) -> Result<Vec<(String, String)>, ConfigError> {
    raw.split('&')
        .map(|item| {
            let (key, value) = item
                .split_once('=')
                .filter(|(key, value)| !key.is_empty() && !value.is_empty())
                .ok_or(ConfigError::InvalidEnvironment {
                    variable: "DB_OPTIONS",
                })?;
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

fn default_max_connections() -> u32 {
    5
}

fn default_acquire_timeout_secs() -> u64 {
    5
}

fn default_connection_name() -> String {
    "default".to_string()
}

fn env_name<'a>(configured: Option<&'a str>, default: &'static str) -> &'a str {
    configured.unwrap_or(default)
}

fn invalid_connection_env(variable: &str) -> ConfigError {
    ConfigError::InvalidConnectionEnvironment {
        variable: variable.to_string(),
    }
}

/// Policy engine configuration: allow-listed tables/columns and row limits.
/// An empty `tables` map denies every table by default.
// Fields read by Policy once the query builder consumes it (see ROADMAP.md).
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct PolicyConfig {
    #[serde(default)]
    pub tables: HashMap<String, TableConfig>,
    #[serde(default = "default_row_limit")]
    pub default_row_limit: u32,
    #[serde(default = "default_max_row_limit")]
    pub max_row_limit: u32,
    #[serde(default = "default_max_query_complexity")]
    pub max_query_complexity: u32,
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: u32,
}

// Manual impl (not derive): Default must use the same values as the serde
// `default = "..."` functions, which a derived Default (all-zero) would not.
impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            tables: HashMap::new(),
            default_row_limit: default_row_limit(),
            max_row_limit: default_max_row_limit(),
            max_query_complexity: default_max_query_complexity(),
            max_output_bytes: default_max_output_bytes(),
        }
    }
}

/// Per-table allow-list. Keys can be unqualified (`users`) or schema-qualified
/// (`audit.users`). An empty `columns` list denies every column on that table.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Default, Clone)]
pub struct TableConfig {
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub filter_operators: HashMap<String, Vec<FilterOperatorConfig>>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperatorConfig {
    Equals,
    NotEquals,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Like,
    #[serde(rename = "ilike")]
    ILike,
    Between,
    FullText,
}

fn default_row_limit() -> u32 {
    100
}

fn default_max_row_limit() -> u32 {
    1000
}

fn default_max_query_complexity() -> u32 {
    100
}

fn default_max_output_bytes() -> u32 {
    128 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static BACKEND_SELECTOR_ENV_LOCK: Mutex<()> = Mutex::new(());
    static CONNECTION_ENV_LOCK: Mutex<()> = Mutex::new(());
    static MYSQL_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn backend_selector_defaults_to_auto_when_unset() {
        with_backend_selector_env(None, || {
            assert_eq!(
                BackendSelector::from_env().expect("selector parsing"),
                BackendSelector::Auto
            );
        });
    }

    #[test]
    fn backend_selector_accepts_supported_values_case_insensitively() {
        with_backend_selector_env(Some("postgres"), || {
            assert_eq!(
                BackendSelector::from_env().unwrap(),
                BackendSelector::Postgres
            );
        });
        with_backend_selector_env(Some("MYSQL"), || {
            assert_eq!(BackendSelector::from_env().unwrap(), BackendSelector::MySql);
        });
        with_backend_selector_env(Some("mariadb"), || {
            assert_eq!(BackendSelector::from_env().unwrap(), BackendSelector::MySql);
        });
        with_backend_selector_env(Some("sqlite"), || {
            assert_eq!(
                BackendSelector::from_env().unwrap(),
                BackendSelector::Sqlite
            );
        });
        with_backend_selector_env(Some("auto"), || {
            assert_eq!(BackendSelector::from_env().unwrap(), BackendSelector::Auto);
        });
    }

    #[test]
    fn backend_selector_rejects_unknown_value() {
        with_backend_selector_env(Some("oracle"), || {
            let err = BackendSelector::from_env().expect_err("unknown selector must fail");
            assert!(matches!(
                err,
                ConfigError::InvalidBackendSelector { value } if value == "oracle"
            ));
        });
    }

    #[test]
    fn selects_named_connection_from_active_profile() {
        let raw = r#"
            profile = "dev"

            [profiles.dev.connection]
            name = "pippo"
            backend = "postgres"
            url_env = "PIPPO_DB_URL"

            [profiles.dev.policy]
            default_row_limit = 10
        "#;
        let config: Config = toml::from_str(raw).expect("valid toml");
        let connection = config.effective_connection().expect("known connection");
        assert_eq!(connection.name, "pippo");
        assert_eq!(
            connection.backend_selector().unwrap(),
            Some(BackendSelector::Postgres)
        );
        assert_eq!(connection.postgres_env_names().url, "PIPPO_DB_URL");
    }

    #[test]
    fn defaults_connection_when_legacy_config_has_no_connection_section() {
        let config: Config = toml::from_str("[policy]\ndefault_row_limit = 5").expect("valid toml");
        let connection = config.effective_connection().expect("default connection");
        assert_eq!(connection.name, "default");
        assert_eq!(connection.backend_selector().unwrap(), None);
        assert_eq!(connection.postgres_env_names().url, "DB_URL");
    }

    #[test]
    fn rejects_invalid_connection_name() {
        let config: Config = toml::from_str(
            r#"
                [connection]
                name = "not safe"
                backend = "sqlite"
            "#,
        )
        .expect("valid toml");
        assert!(matches!(
            config.effective_connection(),
            Err(ConfigError::InvalidConnectionName(name)) if name == "not safe"
        ));
    }

    #[test]
    fn sqlite_config_uses_named_connection_env_vars() {
        with_connection_env(
            &[("DATAGATE_CUSTOM_SQLITE_PATH", Some("/tmp/datagate.db"))],
            || {
                let connection = ConnectionConfig {
                    name: "local".into(),
                    backend: Some("sqlite".into()),
                    path_env: Some("DATAGATE_CUSTOM_SQLITE_PATH".into()),
                    ..ConnectionConfig::default()
                };
                let config = SqliteConfig::from_connection_env(&connection)
                    .expect("sqlite env parsing")
                    .expect("sqlite config present");
                assert_eq!(config.max_connections, default_max_connections());
            },
        );
    }

    #[test]
    fn shared_connection_env_source_prefers_url_over_components() {
        with_connection_env(
            &[
                ("DATAGATE_TEST_URL", Some("postgres://reader@localhost/app")),
                ("DATAGATE_TEST_HOST", Some("localhost")),
            ],
            || {
                let source = connection_env_source(
                    "DATAGATE_TEST_URL",
                    &ComponentEnvNames {
                        host: "DATAGATE_TEST_HOST",
                        port: "DATAGATE_TEST_PORT",
                        user: "DATAGATE_TEST_USER",
                        password: "DATAGATE_TEST_PASSWORD",
                        database: "DATAGATE_TEST_DATABASE",
                    },
                )
                .expect("source");
                assert_eq!(
                    source,
                    ConnectionEnvSource::Url("postgres://reader@localhost/app".into())
                );
            },
        );
    }

    #[test]
    fn shared_component_connection_reads_required_and_optional_values() {
        with_connection_env(
            &[
                ("DATAGATE_TEST_HOST", Some("localhost")),
                ("DATAGATE_TEST_PORT", Some("15432")),
                ("DATAGATE_TEST_USER", Some("reader")),
                ("DATAGATE_TEST_PASSWORD", Some("secret")),
                ("DATAGATE_TEST_DATABASE", Some("app")),
            ],
            || {
                let connection = read_component_connection(
                    &ComponentEnvNames {
                        host: "DATAGATE_TEST_HOST",
                        port: "DATAGATE_TEST_PORT",
                        user: "DATAGATE_TEST_USER",
                        password: "DATAGATE_TEST_PASSWORD",
                        database: "DATAGATE_TEST_DATABASE",
                    },
                    5432,
                )
                .expect("component connection");
                assert_eq!(
                    connection,
                    ComponentConnection {
                        host: "localhost".into(),
                        port: 15432,
                        user: "reader".into(),
                        password: Some("secret".into()),
                        database: "app".into(),
                    }
                );
            },
        );
    }

    #[test]
    fn shared_pool_settings_use_defaults_and_custom_values() {
        assert_eq!(
            read_pool_settings("DATAGATE_TEST_MAX", "DATAGATE_TEST_TIMEOUT").unwrap(),
            PoolSettings {
                max_connections: default_max_connections(),
                acquire_timeout_secs: default_acquire_timeout_secs(),
            }
        );

        with_connection_env(
            &[
                ("DATAGATE_TEST_MAX", Some("9")),
                ("DATAGATE_TEST_TIMEOUT", Some("11")),
            ],
            || {
                assert_eq!(
                    read_pool_settings("DATAGATE_TEST_MAX", "DATAGATE_TEST_TIMEOUT").unwrap(),
                    PoolSettings {
                        max_connections: 9,
                        acquire_timeout_secs: 11,
                    }
                );
            },
        );
    }

    #[test]
    fn parses_policy_from_toml() {
        let raw = r#"
            profile = "dev"

            [policy]
            default_row_limit = 50
            max_row_limit = 500
            max_output_bytes = 8192

            [policy.tables.users]
            columns = ["id", "email"]
        "#;
        let config: Config = toml::from_str(raw).expect("valid toml");
        assert_eq!(config.profile.as_deref(), Some("dev"));
        assert_eq!(config.policy.default_row_limit, 50);
        assert_eq!(config.policy.max_row_limit, 500);
        assert_eq!(config.policy.max_output_bytes, 8192);
        assert!(config.profiles.is_empty());
        let users = config.policy.tables.get("users").expect("users table");
        assert_eq!(users.columns, vec!["id", "email"]);
        assert!(users.filter_operators.is_empty());
    }

    #[test]
    fn parses_filter_operators_per_column() {
        let raw = r#"
            [policy.tables.users]
            columns = ["id", "email"]

            [policy.tables.users.filter_operators]
            email = ["like", "ilike", "full_text"]
            id = ["equals", "between"]
        "#;
        let config: Config = toml::from_str(raw).expect("valid toml");
        let users = config.policy.tables.get("users").expect("users table");

        assert_eq!(
            users
                .filter_operators
                .get("email")
                .cloned()
                .unwrap_or_default(),
            vec![
                FilterOperatorConfig::Like,
                FilterOperatorConfig::ILike,
                FilterOperatorConfig::FullText,
            ]
        );
        assert_eq!(
            users
                .filter_operators
                .get("id")
                .cloned()
                .unwrap_or_default(),
            vec![FilterOperatorConfig::Equals, FilterOperatorConfig::Between]
        );
    }

    #[test]
    fn parses_schema_qualified_table_policy_key() {
        let raw = r#"
            [policy.tables."audit.users"]
            columns = ["id", "email"]
        "#;
        let config: Config = toml::from_str(raw).expect("valid toml");
        let users = config
            .policy
            .tables
            .get("audit.users")
            .expect("audit.users table");
        assert_eq!(users.columns, vec!["id", "email"]);
    }

    #[test]
    fn selects_policy_from_named_profile() {
        let raw = r#"
            profile = "production"

            [policy]
            max_output_bytes = 1024

            [profiles.production.policy]
            default_row_limit = 10
            max_row_limit = 50
        "#;
        let config: Config = toml::from_str(raw).expect("valid toml");
        let policy = config.effective_policy().expect("known profile");
        assert_eq!(policy.default_row_limit, 10);
        assert_eq!(policy.max_row_limit, 50);
        assert_eq!(policy.max_output_bytes, default_max_output_bytes());
    }

    #[test]
    fn rejects_unknown_named_profile() {
        let config: Config = toml::from_str(
            r#"
                profile = "staging"
                [profiles.production.policy]
            "#,
        )
        .expect("valid toml");
        assert!(matches!(
            config.effective_policy(),
            Err(ConfigError::UnknownProfile(profile)) if profile == "staging"
        ));
    }

    #[test]
    fn keeps_flat_policy_when_profiles_are_not_configured() {
        let config: Config = toml::from_str(
            r#"
                profile = "dev"
                [policy]
                default_row_limit = 7
            "#,
        )
        .expect("valid toml");
        assert_eq!(config.effective_policy().unwrap().default_row_limit, 7);
    }

    #[test]
    fn defaults_to_empty_policy_when_section_missing() {
        let config: Config = toml::from_str("profile = \"dev\"").expect("valid toml");
        assert!(config.policy.tables.is_empty());
        assert_eq!(config.policy.default_row_limit, default_row_limit());
        assert_eq!(config.policy.max_row_limit, default_max_row_limit());
        assert_eq!(config.policy.max_output_bytes, default_max_output_bytes());
    }

    #[test]
    fn load_rejects_missing_file() {
        let err = Config::load("/nonexistent/path/datagate.toml").unwrap_err();
        assert!(matches!(err, ConfigError::Read { .. }));
    }

    #[test]
    fn parses_db_options_without_credentials() {
        let options = parse_db_options("statement_timeout=5s&application_name=datagate")
            .expect("valid options");
        assert_eq!(
            options[0],
            ("statement_timeout".to_string(), "5s".to_string())
        );
        assert_eq!(
            options[1],
            ("application_name".to_string(), "datagate".to_string())
        );
    }

    #[test]
    fn rejects_malformed_db_options() {
        let err = parse_db_options("statement_timeout").unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidEnvironment {
                variable: "DB_OPTIONS"
            }
        ));
    }

    #[test]
    fn mysql_config_is_none_when_mysql_env_is_absent() {
        with_mysql_env(
            &[
                ("MYSQL_URL", None),
                ("MYSQL_HOST", None),
                ("MYSQL_PORT", None),
                ("MYSQL_USER", None),
                ("MYSQL_PASSWORD", None),
                ("MYSQL_DATABASE", None),
                ("MYSQL_MAX_CONNECTIONS", None),
                ("MYSQL_ACQUIRE_TIMEOUT_SECS", None),
            ],
            || {
                let config = MySqlConfig::from_env().expect("mysql env parsing should succeed");
                assert!(config.is_none());
            },
        );
    }

    #[test]
    fn mysql_config_parses_url_and_pool_overrides() {
        with_mysql_env(
            &[
                ("MYSQL_URL", Some("mysql://reader@localhost:3306/app")),
                ("MYSQL_MAX_CONNECTIONS", Some("17")),
                ("MYSQL_ACQUIRE_TIMEOUT_SECS", Some("9")),
            ],
            || {
                let config = MySqlConfig::from_env()
                    .expect("mysql env parsing should succeed")
                    .expect("mysql config should be present");
                assert_eq!(config.max_connections, 17);
                assert_eq!(config.acquire_timeout_secs, 9);
            },
        );
    }

    #[test]
    fn mysql_config_parses_components_with_default_port() {
        with_mysql_env(
            &[
                ("MYSQL_HOST", Some("localhost")),
                ("MYSQL_USER", Some("reader")),
                ("MYSQL_DATABASE", Some("app")),
                ("MYSQL_PASSWORD", None),
                ("MYSQL_PORT", None),
            ],
            || {
                let config = MySqlConfig::from_env()
                    .expect("mysql env parsing should succeed")
                    .expect("mysql config should be present");
                assert_eq!(config.max_connections, default_max_connections());
                assert_eq!(config.acquire_timeout_secs, default_acquire_timeout_secs());
            },
        );
    }

    #[test]
    fn mysql_config_rejects_invalid_port() {
        with_mysql_env(
            &[
                ("MYSQL_HOST", Some("localhost")),
                ("MYSQL_USER", Some("reader")),
                ("MYSQL_DATABASE", Some("app")),
                ("MYSQL_PORT", Some("invalid")),
            ],
            || {
                let err = MySqlConfig::from_env().expect_err("invalid port must fail");
                assert!(matches!(
                    err,
                    ConfigError::InvalidConnectionEnvironment { variable } if variable == "MYSQL_PORT"
                ));
            },
        );
    }

    #[test]
    fn mysql_config_rejects_partial_component_configuration() {
        with_mysql_env(
            &[
                ("MYSQL_HOST", Some("localhost")),
                ("MYSQL_USER", None),
                ("MYSQL_DATABASE", Some("app")),
            ],
            || {
                let err = MySqlConfig::from_env().expect_err("missing user must fail");
                assert!(matches!(
                    err,
                    ConfigError::InvalidConnectionEnvironment { variable } if variable == "MYSQL_USER"
                ));
            },
        );
    }

    fn with_mysql_env(overrides: &[(&str, Option<&str>)], test: impl FnOnce()) {
        with_env_lock(&MYSQL_ENV_LOCK, overrides, test);
    }

    fn with_connection_env(overrides: &[(&str, Option<&str>)], test: impl FnOnce()) {
        with_env_lock(&CONNECTION_ENV_LOCK, overrides, test);
    }

    fn with_env_lock(
        lock: &'static Mutex<()>,
        overrides: &[(&str, Option<&str>)],
        test: impl FnOnce(),
    ) {
        let _guard = lock.lock().expect("env lock poisoned");
        let mut original = Vec::with_capacity(overrides.len());
        for (name, _) in overrides {
            original.push((name.to_string(), std::env::var(name).ok()));
        }
        for (name, value) in overrides {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }

        test();

        for (name, value) in original {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }

    fn with_backend_selector_env(value: Option<&str>, test: impl FnOnce()) {
        let _guard = BACKEND_SELECTOR_ENV_LOCK.lock().expect("env lock poisoned");
        let original = std::env::var("DATAGATE_BACKEND").ok();

        match value {
            Some(value) => std::env::set_var("DATAGATE_BACKEND", value),
            None => std::env::remove_var("DATAGATE_BACKEND"),
        }

        test();

        match original {
            Some(value) => std::env::set_var("DATAGATE_BACKEND", value),
            None => std::env::remove_var("DATAGATE_BACKEND"),
        }
    }
}
