//! Policy engine: tables/columns allow-lists, row limits.
//! Evaluated before any query is built — nothing downstream may bypass it.

use thiserror::Error;

use crate::config::{FilterOperatorConfig, PolicyConfig};

// Not yet raised by production code paths until the query builder calls Policy (v0.1).
#[allow(dead_code)]
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("table `{0}` is not allowed by policy")]
    TableNotAllowed(String),
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

    /// Rejects unsupported filter operators when a table defines explicit
    /// per-column operator constraints. If no constraints are present for the
    /// column, all operators remain allowed for backward compatibility.
    pub fn check_filter_operator(
        &self,
        table: &str,
        column: &str,
        operator: FilterOperatorConfig,
    ) -> Result<(), PolicyError> {
        self.check_column(table, column)?;
        let table_config = &self.config.tables[table];
        match table_config.filter_operators.get(column) {
            Some(allowed) if !allowed.contains(&operator) => {
                Err(PolicyError::FilterOperatorNotAllowed {
                    table: table.to_string(),
                    column: column.to_string(),
                    operator,
                })
            }
            _ => Ok(()),
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

    pub fn check_complexity(&self, complexity: u32) -> Result<(), PolicyError> {
        if complexity > self.config.max_query_complexity {
            Err(PolicyError::ComplexityExceeded {
                requested: complexity,
                max: self.config.max_query_complexity,
            })
        } else {
            Ok(())
        }
    }

    pub fn check_output_bytes(&self, output_bytes: u32) -> Result<(), PolicyError> {
        if output_bytes > self.config.max_output_bytes {
            Err(PolicyError::OutputLimitExceeded {
                requested: output_bytes,
                max: self.config.max_output_bytes,
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
}
