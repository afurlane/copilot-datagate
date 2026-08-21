//! Policy engine: tables/columns allow-lists, row limits.
//! Evaluated before any query is built — nothing downstream may bypass it.

use std::sync::{Arc, RwLock};

use thiserror::Error;

use crate::config::{Config, ConfigError, FilterOperatorConfig, PolicyConfig};

// Not yet raised by production code paths until the query builder calls Policy (v0.1).
#[allow(dead_code)]
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("table `{0}` is not allowed by policy")]
    TableNotAllowed(String),
    #[error("unqualified table `{table}` is ambiguous; use one of: {candidates:?}")]
    AmbiguousTableReference {
        table: String,
        candidates: Vec<String>,
    },
    #[error("column `{column}` is not allowed on table `{table}`")]
    ColumnNotAllowed { table: String, column: String },
    #[error("operator is not allowed on `{table}.{column}`")]
    FilterOperatorNotAllowed {
        table: String,
        column: String,
        operator: FilterOperatorConfig,
    },
    #[error("requested row limit {requested} exceeds policy maximum {max}")]
    RowLimitExceeded { requested: u32, max: u32 },
    #[error("query complexity {requested} exceeds policy maximum {max}")]
    ComplexityExceeded { requested: u32, max: u32 },
    #[error("output size {requested} bytes exceeds policy maximum {max} bytes")]
    OutputLimitExceeded { requested: u32, max: u32 },
}

#[derive(Debug, Clone)]
pub struct Policy {
    config: Arc<RwLock<PolicyConfig>>,
}

impl Default for Policy {
    fn default() -> Self {
        Self::new(PolicyConfig::default())
    }
}

// check_table/check_column/resolve_row_limit are only exercised by tests until the
// query builder (v0.1) calls into Policy before generating SQL.
#[allow(dead_code)]
impl Policy {
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Atomically replaces the active policy for subsequent requests.
    pub fn reload(&self, config: PolicyConfig) {
        let mut current = match self.config.write() {
            Ok(current) => current,
            Err(poisoned) => poisoned.into_inner(),
        };
        *current = config;
    }

    /// Loads and activates a policy file without replacing the shared Policy handle.
    pub fn reload_from_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), ConfigError> {
        let config = Config::load(path)?;
        self.reload(config.effective_policy()?);
        Ok(())
    }

    /// Rejects any table not explicitly present in the allow-list.
    pub fn check_table(&self, table: &str) -> Result<(), PolicyError> {
        self.resolve_table_key(table).map(|_| ())
    }

    /// Rejects any column not explicitly allow-listed for the given table.
    pub fn check_column(&self, table: &str, column: &str) -> Result<(), PolicyError> {
        let key = self.resolve_table_key(table)?;
        let config = match self.config.read() {
            Ok(config) => config,
            Err(poisoned) => poisoned.into_inner(),
        };
        let allowed = &config.tables[&key].columns;
        if allowed.iter().any(|c| c == column) {
            Ok(())
        } else {
            Err(PolicyError::ColumnNotAllowed {
                table: key,
                column: column.to_string(),
            })
        }
    }

    /// Rejects unsupported filter operators when a table defines explicit
    /// per-column operator constraints. If no constraints are present for the
    /// column, all operators remain allowed for backward compatibility.
    pub fn check_filter_operator(
        &self,
        table: &str,
        column: &str,
        operator: FilterOperatorConfig,
    ) -> Result<(), PolicyError> {
        let key = self.resolve_table_key(table)?;
        let config = match self.config.read() {
            Ok(config) => config,
            Err(poisoned) => poisoned.into_inner(),
        };
        let table_config = &config.tables[&key];
        let allowed_columns = &table_config.columns;
        if !allowed_columns.iter().any(|candidate| candidate == column) {
            return Err(PolicyError::ColumnNotAllowed {
                table: key,
                column: column.to_string(),
            });
        }
        match table_config.filter_operators.get(column) {
            Some(allowed) if !allowed.contains(&operator) => {
                Err(PolicyError::FilterOperatorNotAllowed {
                    table: key,
                    column: column.to_string(),
                    operator,
                })
            }
            _ => Ok(()),
        }
    }

    fn resolve_table_key(&self, table: &str) -> Result<String, PolicyError> {
        let config = match self.config.read() {
            Ok(config) => config,
            Err(poisoned) => poisoned.into_inner(),
        };
        if config.tables.contains_key(table) {
            return Ok(table.to_string());
        }

        if table.contains('.') {
            return Err(PolicyError::TableNotAllowed(table.to_string()));
        }

        let suffix = format!(".{table}");
        let mut candidates = config
            .tables
            .keys()
            .filter(|key| key.ends_with(&suffix))
            .cloned()
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err(PolicyError::TableNotAllowed(table.to_string()));
        }
        candidates.sort();
        candidates.dedup();
        if candidates.len() > 1 {
            return Err(PolicyError::AmbiguousTableReference {
                table: table.to_string(),
                candidates,
            });
        }
        Ok(candidates[0].clone())
    }

    /// Resolves the effective row limit for a request, applying the default when unset
    /// and rejecting anything above the configured maximum.
    pub fn resolve_row_limit(&self, requested: Option<u32>) -> Result<u32, PolicyError> {
        let config = match self.config.read() {
            Ok(config) => config,
            Err(poisoned) => poisoned.into_inner(),
        };
        let limit = requested.unwrap_or(config.default_row_limit);
        if limit > config.max_row_limit {
            Err(PolicyError::RowLimitExceeded {
                requested: limit,
                max: config.max_row_limit,
            })
        } else {
            Ok(limit)
        }
    }

    pub fn check_complexity(&self, complexity: u32) -> Result<(), PolicyError> {
        let config = match self.config.read() {
            Ok(config) => config,
            Err(poisoned) => poisoned.into_inner(),
        };
        if complexity > config.max_query_complexity {
            Err(PolicyError::ComplexityExceeded {
                requested: complexity,
                max: config.max_query_complexity,
            })
        } else {
            Ok(())
        }
    }

    pub fn check_output_bytes(&self, output_bytes: u32) -> Result<(), PolicyError> {
        let config = match self.config.read() {
            Ok(config) => config,
            Err(poisoned) => poisoned.into_inner(),
        };
        if output_bytes > config.max_output_bytes {
            Err(PolicyError::OutputLimitExceeded {
                requested: output_bytes,
                max: config.max_output_bytes,
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::TableConfig;

    fn policy_with_users_id_email() -> Policy {
        let mut tables = HashMap::new();
        tables.insert(
            "users".to_string(),
            TableConfig {
                columns: vec!["id".to_string(), "email".to_string()],
                filter_operators: HashMap::new(),
            },
        );
        Policy::new(PolicyConfig {
            tables,
            default_row_limit: 50,
            max_row_limit: 200,
            max_query_complexity: 100,
            max_output_bytes: 10_000,
        })
    }

    #[test]
    fn allows_table_in_allowlist() {
        assert!(policy_with_users_id_email().check_table("users").is_ok());
    }

    #[test]
    fn reload_updates_existing_policy_handle() {
        let policy = policy_with_users_id_email();
        assert!(policy.check_table("users").is_ok());

        policy.reload(PolicyConfig::default());

        assert_eq!(
            policy.check_table("users").unwrap_err(),
            PolicyError::TableNotAllowed("users".to_string())
        );
    }

    #[test]
    fn reload_from_file_activates_effective_profile_policy() {
        let path = std::env::temp_dir().join(format!(
            "datagate-policy-{}-{}.toml",
            std::process::id(),
            "valid"
        ));
        std::fs::write(
            &path,
            r#"
                profile = "restricted"
                [profiles.restricted.policy.tables.users]
                columns = ["id"]
            "#,
        )
        .expect("write policy fixture");

        let policy = policy_with_users_id_email();
        policy
            .reload_from_file(&path)
            .expect("reload policy fixture");
        std::fs::remove_file(path).expect("remove policy fixture");

        assert!(policy.check_column("users", "id").is_ok());
        assert_eq!(
            policy.check_column("users", "email").unwrap_err(),
            PolicyError::ColumnNotAllowed {
                table: "users".to_string(),
                column: "email".to_string(),
            }
        );
    }

    #[test]
    fn failed_file_reload_keeps_current_policy_active() {
        let policy = policy_with_users_id_email();
        let error = policy.reload_from_file("/nonexistent/path/datagate.toml");

        assert!(matches!(error, Err(ConfigError::Read { .. })));
        assert!(policy.check_column("users", "email").is_ok());
    }

    #[test]
    fn denies_table_not_in_allowlist() {
        let err = policy_with_users_id_email()
            .check_table("secrets")
            .unwrap_err();
        assert_eq!(err, PolicyError::TableNotAllowed("secrets".to_string()));
    }

    #[test]
    fn allows_column_in_allowlist() {
        assert!(policy_with_users_id_email()
            .check_column("users", "email")
            .is_ok());
    }

    #[test]
    fn denies_column_not_in_allowlist() {
        let err = policy_with_users_id_email()
            .check_column("users", "password_hash")
            .unwrap_err();
        assert_eq!(
            err,
            PolicyError::ColumnNotAllowed {
                table: "users".to_string(),
                column: "password_hash".to_string(),
            }
        );
    }

    #[test]
    fn denies_column_on_table_not_in_allowlist() {
        let err = policy_with_users_id_email()
            .check_column("secrets", "value")
            .unwrap_err();
        assert_eq!(err, PolicyError::TableNotAllowed("secrets".to_string()));
    }

    #[test]
    fn resolves_default_row_limit_when_unspecified() {
        assert_eq!(
            policy_with_users_id_email()
                .resolve_row_limit(None)
                .unwrap(),
            50
        );
    }

    #[test]
    fn allows_requested_row_limit_within_max() {
        assert_eq!(
            policy_with_users_id_email()
                .resolve_row_limit(Some(150))
                .unwrap(),
            150
        );
    }

    #[test]
    fn denies_requested_row_limit_above_max() {
        let err = policy_with_users_id_email()
            .resolve_row_limit(Some(500))
            .unwrap_err();
        assert_eq!(
            err,
            PolicyError::RowLimitExceeded {
                requested: 500,
                max: 200,
            }
        );
    }

    #[test]
    fn denies_query_complexity_above_max() {
        let err = policy_with_users_id_email()
            .check_complexity(101)
            .unwrap_err();
        assert_eq!(
            err,
            PolicyError::ComplexityExceeded {
                requested: 101,
                max: 100,
            }
        );
    }

    #[test]
    fn denies_output_size_above_max() {
        let err = policy_with_users_id_email()
            .check_output_bytes(10_001)
            .unwrap_err();
        assert_eq!(
            err,
            PolicyError::OutputLimitExceeded {
                requested: 10_001,
                max: 10_000,
            }
        );
    }

    #[test]
    fn allows_filter_operator_when_no_constraints_are_configured() {
        assert!(policy_with_users_id_email()
            .check_filter_operator("users", "email", FilterOperatorConfig::FullText)
            .is_ok());
    }

    #[test]
    fn denies_filter_operator_when_not_in_column_allowlist() {
        let mut tables = HashMap::new();
        let mut filter_operators = HashMap::new();
        filter_operators.insert(
            "email".to_string(),
            vec![FilterOperatorConfig::Like, FilterOperatorConfig::ILike],
        );
        tables.insert(
            "users".to_string(),
            TableConfig {
                columns: vec!["id".to_string(), "email".to_string()],
                filter_operators,
            },
        );
        let policy = Policy::new(PolicyConfig {
            tables,
            default_row_limit: 50,
            max_row_limit: 200,
            max_query_complexity: 100,
            max_output_bytes: 10_000,
        });

        let err = policy
            .check_filter_operator("users", "email", FilterOperatorConfig::FullText)
            .unwrap_err();
        assert_eq!(
            err,
            PolicyError::FilterOperatorNotAllowed {
                table: "users".to_string(),
                column: "email".to_string(),
                operator: FilterOperatorConfig::FullText,
            }
        );
    }

    #[test]
    fn allows_explicit_schema_qualified_table_keys() {
        let mut tables = HashMap::new();
        tables.insert(
            "audit.users".to_string(),
            TableConfig {
                columns: vec!["id".to_string(), "email".to_string()],
                filter_operators: HashMap::new(),
            },
        );
        let policy = Policy::new(PolicyConfig {
            tables,
            default_row_limit: 50,
            max_row_limit: 200,
            max_query_complexity: 100,
            max_output_bytes: 10_000,
        });

        assert!(policy.check_table("audit.users").is_ok());
        assert!(policy.check_column("audit.users", "email").is_ok());
    }

    #[test]
    fn resolves_unqualified_table_when_single_schema_match_exists() {
        let mut tables = HashMap::new();
        tables.insert(
            "audit.users".to_string(),
            TableConfig {
                columns: vec!["id".to_string()],
                filter_operators: HashMap::new(),
            },
        );
        let policy = Policy::new(PolicyConfig {
            tables,
            default_row_limit: 50,
            max_row_limit: 200,
            max_query_complexity: 100,
            max_output_bytes: 10_000,
        });

        assert!(policy.check_table("users").is_ok());
        assert!(policy.check_column("users", "id").is_ok());
    }

    #[test]
    fn rejects_ambiguous_unqualified_table_across_multiple_schemas() {
        let mut tables = HashMap::new();
        tables.insert(
            "audit.users".to_string(),
            TableConfig {
                columns: vec!["id".to_string()],
                filter_operators: HashMap::new(),
            },
        );
        tables.insert(
            "public.users".to_string(),
            TableConfig {
                columns: vec!["id".to_string()],
                filter_operators: HashMap::new(),
            },
        );
        let policy = Policy::new(PolicyConfig {
            tables,
            default_row_limit: 50,
            max_row_limit: 200,
            max_query_complexity: 100,
            max_output_bytes: 10_000,
        });

        assert!(matches!(
            policy.check_table("users"),
            Err(PolicyError::AmbiguousTableReference { .. })
        ));
    }
}
