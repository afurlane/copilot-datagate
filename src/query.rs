//! Controlled, parameterized query plans. No caller-provided SQL is accepted.

use thiserror::Error;

use crate::policy::{Policy, PolicyError};

#[derive(Debug, Error, PartialEq)]
pub enum QueryBuilderError {
    #[error("query policy rejected the request: {0}")]
    Policy(#[from] PolicyError),
    #[error("identifier `{0}` contains unsupported characters")]
    InvalidIdentifier(String),
    #[error("a select request must contain at least one column")]
    EmptyColumns,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BindValue {
    Text(String),
    Integer(i64),
    Decimal(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Like,
    ILike,
}

impl FilterOperator {
    fn sql(self) -> &'static str {
        match self {
            Self::Equals => "=",
            Self::NotEquals => "<>",
            Self::LessThan => "<",
            Self::LessThanOrEqual => "<=",
            Self::GreaterThan => ">",
            Self::GreaterThanOrEqual => ">=",
            Self::Like => "LIKE",
            Self::ILike => "ILIKE",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub column: String,
    pub operator: FilterOperator,
    pub value: BindValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectRequest {
    pub table: String,
    pub columns: Vec<String>,
    pub filters: Vec<Filter>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryPlan {
    pub sql: String,
    pub binds: Vec<BindValue>,
}

impl SelectRequest {
    /// Builds a parameterized SELECT after validating every identifier and limit.
    pub fn build(&self, policy: &Policy) -> Result<QueryPlan, QueryBuilderError> {
        if self.columns.is_empty() {
            return Err(QueryBuilderError::EmptyColumns);
        }

        policy.check_table(&self.table)?;
        let table = quote_identifier(&self.table)?;
        let mut columns = Vec::with_capacity(self.columns.len());
        for column in &self.columns {
            policy.check_column(&self.table, column)?;
            columns.push(quote_identifier(column)?);
        }

        let limit = policy.resolve_row_limit(self.limit)?;
        let mut sql = format!("SELECT {} FROM {}", columns.join(", "), table);
        let mut binds = Vec::with_capacity(self.filters.len() + 1);

        if !self.filters.is_empty() {
            let mut predicates = Vec::with_capacity(self.filters.len());
            for (index, filter) in self.filters.iter().enumerate() {
                policy.check_column(&self.table, &filter.column)?;
                predicates.push(format!(
                    "{} {} ${}",
                    quote_identifier(&filter.column)?,
                    filter.operator.sql(),
                    index + 1
                ));
                binds.push(filter.value.clone());
            }
            sql.push_str(" WHERE ");
            sql.push_str(&predicates.join(" AND "));
        }

        binds.push(BindValue::Integer(i64::from(limit)));
        sql.push_str(&format!(" LIMIT ${}", binds.len()));

        Ok(QueryPlan { sql, binds })
    }
}

fn quote_identifier(identifier: &str) -> Result<String, QueryBuilderError> {
    let valid = !identifier.is_empty()
        && identifier.bytes().enumerate().all(|(index, byte)| {
            byte == b'_'
                || byte.is_ascii_alphanumeric()
                    && (index > 0 || byte.is_ascii_alphabetic() || byte == b'_')
        });
    if valid {
        Ok(format!("\"{identifier}\""))
    } else {
        Err(QueryBuilderError::InvalidIdentifier(identifier.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::{PolicyConfig, TableConfig};

    fn users_policy() -> Policy {
        let mut tables = HashMap::new();
        tables.insert(
            "users".to_string(),
            TableConfig {
                columns: vec!["id".into(), "email".into(), "active".into()],
            },
        );
        Policy::new(PolicyConfig {
            tables,
            default_row_limit: 25,
            max_row_limit: 100,
        })
    }

    #[test]
    fn builds_parameterized_select_with_filters_and_server_limit() {
        let request = SelectRequest {
            table: "users".into(),
            columns: vec!["id".into(), "email".into()],
            filters: vec![Filter {
                column: "active".into(),
                operator: FilterOperator::Equals,
                value: BindValue::Boolean(true),
            }],
            limit: Some(10),
        };

        let plan = request.build(&users_policy()).expect("valid query plan");
        assert_eq!(
            plan.sql,
            "SELECT \"id\", \"email\" FROM \"users\" WHERE \"active\" = $1 LIMIT $2"
        );
        assert_eq!(
            plan.binds,
            vec![BindValue::Boolean(true), BindValue::Integer(10)]
        );
    }

    #[test]
    fn uses_policy_default_limit_when_request_omits_limit() {
        let request = SelectRequest {
            table: "users".into(),
            columns: vec!["id".into()],
            filters: vec![],
            limit: None,
        };
        let plan = request.build(&users_policy()).expect("valid query plan");
        assert_eq!(plan.sql, "SELECT \"id\" FROM \"users\" LIMIT $1");
        assert_eq!(plan.binds, vec![BindValue::Integer(25)]);
    }

    #[test]
    fn rejects_table_not_allowed_by_policy() {
        let request = SelectRequest {
            table: "secrets".into(),
            columns: vec!["value".into()],
            filters: vec![],
            limit: Some(1),
        };
        assert!(matches!(
            request.build(&users_policy()),
            Err(QueryBuilderError::Policy(PolicyError::TableNotAllowed(_)))
        ));
    }

    #[test]
    fn rejects_column_not_allowed_by_policy() {
        let request = SelectRequest {
            table: "users".into(),
            columns: vec!["password_hash".into()],
            filters: vec![],
            limit: Some(1),
        };
        assert!(matches!(
            request.build(&users_policy()),
            Err(QueryBuilderError::Policy(
                PolicyError::ColumnNotAllowed { .. }
            ))
        ));
    }

    #[test]
    fn rejects_identifier_injection() {
        assert!(matches!(
            quote_identifier("users; DROP TABLE users"),
            Err(QueryBuilderError::InvalidIdentifier(_))
        ));
    }

    #[test]
    fn rejects_empty_columns() {
        let request = SelectRequest {
            table: "users".into(),
            columns: vec![],
            filters: vec![],
            limit: Some(1),
        };
        assert_eq!(
            request.build(&users_policy()),
            Err(QueryBuilderError::EmptyColumns)
        );
    }
}
