//! Configuration loading (profiles, backends, policy limits).

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use sqlx::postgres::PgConnectOptions;
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
    #[error("invalid or missing database environment variable `{variable}`")]
    InvalidEnvironment { variable: &'static str },
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Config {
    // Not read yet; will select environment-specific settings once profiles land (v0.2).
    #[allow(dead_code)]
    pub profile: Option<String>,
    #[serde(default)]
    pub policy: PolicyConfig,
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
    pub fn from_env() -> Result<Option<Self>, ConfigError> {
        let has_url = env::var_os("DB_URL").is_some();
        let has_components = [
            "DB_HOST",
            "DB_USER",
            "DB_PORT",
            "DB_PASSWORD",
            "DB_NAME",
            "DB_OPTIONS",
        ]
        .iter()
        .any(|name| env::var_os(name).is_some());

        if !has_url && !has_components {
            return Ok(None);
        }

        let mut connect_options = if let Some(url) = env::var_os("DB_URL") {
            let url = url
                .to_str()
                .ok_or(ConfigError::InvalidEnvironment { variable: "DB_URL" })?;
            PgConnectOptions::from_str(url)
                .map_err(|_| ConfigError::InvalidEnvironment { variable: "DB_URL" })?
        } else {
            let host = required_env("DB_HOST")?;
            let user = required_env("DB_USER")?;
            let database = required_env("DB_NAME")?;
            let port = optional_env("DB_PORT")?
                .map(|value| {
                    value
                        .parse::<u16>()
                        .map_err(|_| ConfigError::InvalidEnvironment {
                            variable: "DB_PORT",
                        })
                })
                .transpose()?
                .unwrap_or(5432);

            let mut options = PgConnectOptions::new()
                .host(&host)
                .port(port)
                .username(&user)
                .database(&database);
            if let Some(password) = optional_env("DB_PASSWORD")? {
                options = options.password(&password);
            }
            options
        };

        if let Some(raw_options) = optional_env("DB_OPTIONS")? {
            connect_options = connect_options.options(parse_db_options(&raw_options)?);
        }

        Ok(Some(Self {
            connect_options,
            max_connections: env_u32_or_default("DB_MAX_CONNECTIONS", default_max_connections())?,
            acquire_timeout_secs: env_u64_or_default(
                "DB_ACQUIRE_TIMEOUT_SECS",
                default_acquire_timeout_secs(),
            )?,
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

fn required_env(variable: &'static str) -> Result<String, ConfigError> {
    optional_env(variable)?.ok_or(ConfigError::InvalidEnvironment { variable })
}

fn optional_env(variable: &'static str) -> Result<Option<String>, ConfigError> {
    match env::var(variable) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) => Err(ConfigError::InvalidEnvironment { variable }),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidEnvironment { variable }),
    }
}

fn env_u32_or_default(variable: &'static str, default: u32) -> Result<u32, ConfigError> {
    optional_env(variable)?
        .map(|value| {
            value
                .parse()
                .map_err(|_| ConfigError::InvalidEnvironment { variable })
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn env_u64_or_default(variable: &'static str, default: u64) -> Result<u64, ConfigError> {
    optional_env(variable)?
        .map(|value| {
            value
                .parse()
                .map_err(|_| ConfigError::InvalidEnvironment { variable })
        })
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
}

// Manual impl (not derive): Default must use the same values as the serde
// `default = "..."` functions, which a derived Default (all-zero) would not.
impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            tables: HashMap::new(),
            default_row_limit: default_row_limit(),
            max_row_limit: default_max_row_limit(),
        }
    }
}

/// Per-table allow-list. An empty `columns` list denies every column on that table.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Default, Clone)]
pub struct TableConfig {
    #[serde(default)]
    pub columns: Vec<String>,
}

fn default_row_limit() -> u32 {
    100
}

fn default_max_row_limit() -> u32 {
    1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_policy_from_toml() {
        let raw = r#"
            profile = "dev"

            [policy]
            default_row_limit = 50
            max_row_limit = 500

            [policy.tables.users]
            columns = ["id", "email"]
        "#;
        let config: Config = toml::from_str(raw).expect("valid toml");
        assert_eq!(config.profile.as_deref(), Some("dev"));
        assert_eq!(config.policy.default_row_limit, 50);
        assert_eq!(config.policy.max_row_limit, 500);
        let users = config.policy.tables.get("users").expect("users table");
        assert_eq!(users.columns, vec!["id", "email"]);
    }

    #[test]
    fn defaults_to_empty_policy_when_section_missing() {
        let config: Config = toml::from_str("profile = \"dev\"").expect("valid toml");
        assert!(config.policy.tables.is_empty());
        assert_eq!(config.policy.default_row_limit, default_row_limit());
        assert_eq!(config.policy.max_row_limit, default_max_row_limit());
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
}
