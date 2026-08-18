//! Policy engine: tables/columns allow-lists, row limits.
//! Evaluated before any query is built — nothing downstream may bypass it.

use thiserror::Error;

use crate::config::PolicyConfig;

// Not yet raised by production code paths until the query builder calls Policy (v0.1).
#[allow(dead_code)]
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("table `{0}` is not allowed by policy")]
    TableNotAllowed(String),
    #[error("column `{column}` is not allowed on table `{table}`")]
    ColumnNotAllowed { table: String, column: String },
    #[error("requested row limit {requested} exceeds policy maximum {max}")]
    RowLimitExceeded { requested: u32, max: u32 },
}

#[derive(Debug, Default, Clone)]
pub struct Policy {
    // Consumed by check_table/check_column/resolve_row_limit below (v0.1 in progress).
    #[allow(dead_code)]
    config: PolicyConfig,
}

// check_table/check_column/resolve_row_limit are only exercised by tests until the
// query builder (v0.1) calls into Policy before generating SQL.
#[allow(dead_code)]
impl Policy {
    pub fn new(config: PolicyConfig) -> Self {
        Self { config }
    }

    /// Rejects any table not explicitly present in the allow-list.
    pub fn check_table(&self, table: &str) -> Result<(), PolicyError> {
        if self.config.tables.contains_key(table) {
            Ok(())
        } else {
            Err(PolicyError::TableNotAllowed(table.to_string()))
        }
    }

    /// Rejects any column not explicitly allow-listed for the given table.
    pub fn check_column(&self, table: &str, column: &str) -> Result<(), PolicyError> {
        self.check_table(table)?;
        let allowed = &self.config.tables[table].columns;
        if allowed.iter().any(|c| c == column) {
            Ok(())
        } else {
            Err(PolicyError::ColumnNotAllowed {
                table: table.to_string(),
                column: column.to_string(),
            })
        }
    }

    /// Resolves the effective row limit for a request, applying the default when unset
    /// and rejecting anything above the configured maximum.
    pub fn resolve_row_limit(&self, requested: Option<u32>) -> Result<u32, PolicyError> {
        let limit = requested.unwrap_or(self.config.default_row_limit);
        if limit > self.config.max_row_limit {
            Err(PolicyError::RowLimitExceeded {
                requested: limit,
                max: self.config.max_row_limit,
            })
        } else {
            Ok(limit)
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
            },
        );
        Policy::new(PolicyConfig {
            tables,
            default_row_limit: 50,
            max_row_limit: 200,
        })
    }

    #[test]
    fn allows_table_in_allowlist() {
        assert!(policy_with_users_id_email().check_table("users").is_ok());
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
}
