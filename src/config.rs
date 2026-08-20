//! Configuration loading (profiles, backends, policy limits).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
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
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Config {
    // Not read yet; will select environment-specific settings once profiles land (v0.2).
    #[allow(dead_code)]
    pub profile: Option<String>,
    pub postgres: Option<PostgresConfig>,
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

/// PostgreSQL pool configuration. The configured database role must itself be
/// read-only; DataGate also enforces a read-only default for every pool session.
#[derive(Debug, Deserialize, Clone)]
pub struct PostgresConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_acquire_timeout_secs")]
    pub acquire_timeout_secs: u64,
}

impl PostgresConfig {
    pub fn validate(&self) -> bool {
        !self.url.trim().is_empty() && self.max_connections > 0 && self.acquire_timeout_secs > 0
    }
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
    fn parses_postgres_config_with_safe_defaults() {
        let config: Config = toml::from_str(
            r#"
                [postgres]
                url = "postgres://readonly:secret@localhost/application"
            "#,
        )
        .expect("valid toml");
        let postgres = config.postgres.expect("postgres config");
        assert!(postgres.validate());
        assert_eq!(postgres.max_connections, default_max_connections());
        assert_eq!(
            postgres.acquire_timeout_secs,
            default_acquire_timeout_secs()
        );
    }

    #[test]
    fn rejects_invalid_postgres_config_values() {
        let postgres = PostgresConfig {
            url: " ".to_string(),
            max_connections: 0,
            acquire_timeout_secs: 0,
        };
        assert!(!postgres.validate());
    }
}
