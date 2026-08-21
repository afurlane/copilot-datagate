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
    #[error("a search request must contain at least one searchable column")]
    EmptySearchColumns,
    #[error("an aggregate request must contain at least one operation")]
    EmptyAggregates,
    #[error("filter on column `{0}` requires a secondary value")]
    MissingSecondaryFilterValue(String),
    #[error("filter on column `{0}` does not accept a secondary value")]
    UnexpectedSecondaryFilterValue(String),
    #[error("filter on column `{0}` requires text values")]
    InvalidTextFilterValue(String),
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
    Between,
    FullText,
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
            Self::Between => "BETWEEN",
            Self::FullText => "@@",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub column: String,
    pub operator: FilterOperator,
    pub value: BindValue,
    pub value_to: Option<BindValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectRequest {
    pub table: String,
    pub columns: Vec<String>,
    pub filters: Vec<Filter>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchRequest {
    pub table: String,
    pub columns: Vec<String>,
    pub searchable_columns: Vec<String>,
    pub text: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl AggregateFunction {
    fn sql(self) -> &'static str {
        match self {
            Self::Count => "COUNT",
            Self::Sum => "SUM",
            Self::Avg => "AVG",
            Self::Min => "MIN",
            Self::Max => "MAX",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AggregateOperation {
    pub function: AggregateFunction,
    pub column: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AggregateRequest {
    pub table: String,
    pub operations: Vec<AggregateOperation>,
    pub filters: Vec<Filter>,
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

        let table = validate_table(policy, &self.table)?;
        let columns = validate_columns(policy, &self.table, &self.columns)?;

        let limit = policy.resolve_row_limit(self.limit)?;
        policy.check_complexity(query_complexity(self.columns.len(), self.filters.len()))?;
        let mut sql = format!("SELECT {} FROM {}", columns.join(", "), table);
        let mut binds = Vec::with_capacity(self.filters.len() + 1);

        if !self.filters.is_empty() {
            let (predicates, filter_binds) = build_filters(policy, &self.table, &self.filters)?;
            binds.extend(filter_binds);
            sql.push_str(" WHERE ");
            sql.push_str(&predicates);
        }

        binds.push(BindValue::Integer(i64::from(limit)));
        sql.push_str(&format!(" LIMIT ${}", binds.len()));

        Ok(QueryPlan { sql, binds })
    }
}

impl SearchRequest {
    /// Builds a parameterized ILIKE search over policy-allowed columns only.
    pub fn build(&self, policy: &Policy) -> Result<QueryPlan, QueryBuilderError> {
        if self.columns.is_empty() {
            return Err(QueryBuilderError::EmptyColumns);
        }
        if self.searchable_columns.is_empty() {
            return Err(QueryBuilderError::EmptySearchColumns);
        }

        let table = validate_table(policy, &self.table)?;
        let columns = validate_columns(policy, &self.table, &self.columns)?;
        let searchable_columns = validate_columns(policy, &self.table, &self.searchable_columns)?;

        let limit = policy.resolve_row_limit(self.limit)?;
        policy.check_complexity(query_complexity(
            self.columns.len(),
            self.searchable_columns.len(),
        ))?;

        let mut sql = format!("SELECT {} FROM {}", columns.join(", "), table);
        let binds = vec![
            BindValue::Text(format!("%{}%", self.text)),
            BindValue::Integer(i64::from(limit)),
        ];

        let predicates = searchable_columns
            .iter()
            .map(|column| format!("{column} ILIKE $1"))
            .collect::<Vec<_>>()
            .join(" OR ");
        sql.push_str(" WHERE (");
        sql.push_str(&predicates);
        sql.push(')');
        sql.push_str(" LIMIT $2");

        Ok(QueryPlan { sql, binds })
    }
}

impl AggregateRequest {
    /// Builds a parameterized aggregate over policy-allowed columns only.
    pub fn build(&self, policy: &Policy) -> Result<QueryPlan, QueryBuilderError> {
        if self.operations.is_empty() {
            return Err(QueryBuilderError::EmptyAggregates);
        }

        let table = validate_table(policy, &self.table)?;
        let mut expressions = Vec::with_capacity(self.operations.len());
        for operation in &self.operations {
            let column =
                if operation.function == AggregateFunction::Count && operation.column == "*" {
                    "*".to_string()
                } else {
                    policy.check_column(&self.table, &operation.column)?;
                    quote_identifier(&operation.column)?
                };
            let mut expression = format!("{}({column})", operation.function.sql());
            if let Some(alias) = &operation.alias {
                expression.push_str(" AS ");
                expression.push_str(&quote_identifier(alias)?);
            }
            expressions.push(expression);
        }

        policy.check_complexity(query_complexity(self.operations.len(), self.filters.len()))?;

        let mut sql = format!("SELECT {} FROM {table}", expressions.join(", "));
        let mut binds = Vec::with_capacity(self.filters.len());
        if !self.filters.is_empty() {
            let (predicates, filter_binds) = build_filters(policy, &self.table, &self.filters)?;
            binds.extend(filter_binds);
            sql.push_str(" WHERE ");
            sql.push_str(&predicates);
        }

        Ok(QueryPlan { sql, binds })
    }
}

fn validate_table(policy: &Policy, table: &str) -> Result<String, QueryBuilderError> {
    policy.check_table(table)?;
    quote_identifier(table)
}

fn validate_columns(
    policy: &Policy,
    table: &str,
    columns: &[String],
) -> Result<Vec<String>, QueryBuilderError> {
    columns
        .iter()
        .map(|column| {
            policy.check_column(table, column)?;
            quote_identifier(column)
        })
        .collect()
}

fn build_filters(
    policy: &Policy,
    table: &str,
    filters: &[Filter],
) -> Result<(String, Vec<BindValue>), QueryBuilderError> {
    let mut predicates = Vec::with_capacity(filters.len());
    let mut binds = Vec::with_capacity(filters.len());
    let mut bind_index = 1;
    for filter in filters {
        policy.check_column(table, &filter.column)?;
        let quoted_column = quote_identifier(&filter.column)?;
        match filter.operator {
            FilterOperator::Between => {
                let value_to = filter.value_to.clone().ok_or_else(|| {
                    QueryBuilderError::MissingSecondaryFilterValue(filter.column.clone())
                })?;
                predicates.push(format!(
                    "{quoted_column} BETWEEN ${bind_index} AND ${}",
                    bind_index + 1
                ));
                binds.push(filter.value.clone());
                binds.push(value_to);
                bind_index += 2;
            }
            FilterOperator::FullText => {
                ensure_text_value(&filter.column, &filter.value)?;
                if filter.value_to.is_some() {
                    return Err(QueryBuilderError::UnexpectedSecondaryFilterValue(
                        filter.column.clone(),
                    ));
                }
                predicates.push(format!(
                    "to_tsvector('simple', coalesce({quoted_column}::text, '')) @@ plainto_tsquery('simple', ${bind_index})"
                ));
                binds.push(filter.value.clone());
                bind_index += 1;
            }
            FilterOperator::Like | FilterOperator::ILike => {
                ensure_text_value(&filter.column, &filter.value)?;
                if filter.value_to.is_some() {
                    return Err(QueryBuilderError::UnexpectedSecondaryFilterValue(
                        filter.column.clone(),
                    ));
                }
                predicates.push(format!(
                    "{quoted_column} {} ${bind_index}",
                    filter.operator.sql()
                ));
                binds.push(filter.value.clone());
                bind_index += 1;
            }
            _ => {
                if filter.value_to.is_some() {
                    return Err(QueryBuilderError::UnexpectedSecondaryFilterValue(
                        filter.column.clone(),
                    ));
                }
                predicates.push(format!(
                    "{quoted_column} {} ${bind_index}",
                    filter.operator.sql()
                ));
                binds.push(filter.value.clone());
                bind_index += 1;
            }
        }
    }
    Ok((predicates.join(" AND "), binds))
}

fn ensure_text_value(column: &str, value: &BindValue) -> Result<(), QueryBuilderError> {
    if matches!(value, BindValue::Text(_)) {
        Ok(())
    } else {
        Err(QueryBuilderError::InvalidTextFilterValue(
            column.to_string(),
        ))
    }
}

fn query_complexity(primary_terms: usize, secondary_terms: usize) -> u32 {
    u32::try_from(primary_terms)
        .unwrap_or(u32::MAX)
        .saturating_add(
            u32::try_from(secondary_terms)
                .unwrap_or(u32::MAX)
                .saturating_mul(2),
        )
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
            max_query_complexity: 100,
            max_output_bytes: 10_000,
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
                value_to: None,
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

    #[test]
    fn rejects_query_that_exceeds_complexity_budget() {
        let mut tables = HashMap::new();
        tables.insert(
            "users".to_string(),
            TableConfig {
                columns: vec!["id".into(), "email".into(), "active".into()],
            },
        );
        let policy = Policy::new(PolicyConfig {
            tables,
            default_row_limit: 25,
            max_row_limit: 100,
            max_query_complexity: 2,
            max_output_bytes: 10_000,
        });
        let request = SelectRequest {
            table: "users".into(),
            columns: vec!["id".into(), "email".into()],
            filters: vec![Filter {
                column: "active".into(),
                operator: FilterOperator::Equals,
                value: BindValue::Boolean(true),
                value_to: None,
            }],
            limit: Some(1),
        };
        assert!(matches!(
            request.build(&policy),
            Err(QueryBuilderError::Policy(
                PolicyError::ComplexityExceeded { .. }
            ))
        ));
    }

    #[test]
    fn builds_parameterized_search_with_ilike_over_allowed_columns() {
        let request = SearchRequest {
            table: "users".into(),
            columns: vec!["id".into(), "email".into()],
            searchable_columns: vec!["email".into()],
            text: "alice".into(),
            limit: Some(5),
        };

        let plan = request.build(&users_policy()).expect("valid query plan");
        assert_eq!(
            plan.sql,
            "SELECT \"id\", \"email\" FROM \"users\" WHERE (\"email\" ILIKE $1) LIMIT $2"
        );
        assert_eq!(
            plan.binds,
            vec![BindValue::Text("%alice%".into()), BindValue::Integer(5)]
        );
    }

    #[test]
    fn builds_parameterized_aggregates_with_aliases_and_filters() {
        let request = AggregateRequest {
            table: "users".into(),
            operations: vec![
                AggregateOperation {
                    function: AggregateFunction::Count,
                    column: "*".into(),
                    alias: Some("total_users".into()),
                },
                AggregateOperation {
                    function: AggregateFunction::Avg,
                    column: "id".into(),
                    alias: Some("average_id".into()),
                },
            ],
            filters: vec![Filter {
                column: "active".into(),
                operator: FilterOperator::Equals,
                value: BindValue::Boolean(true),
                value_to: None,
            }],
        };

        let plan = request
            .build(&users_policy())
            .expect("valid aggregate plan");
        assert_eq!(
            plan.sql,
            "SELECT COUNT(*) AS \"total_users\", AVG(\"id\") AS \"average_id\" FROM \"users\" WHERE \"active\" = $1"
        );
        assert_eq!(plan.binds, vec![BindValue::Boolean(true)]);
    }

    #[test]
    fn builds_all_supported_aggregate_functions() {
        let request = AggregateRequest {
            table: "users".into(),
            operations: vec![
                AggregateOperation {
                    function: AggregateFunction::Count,
                    column: "id".into(),
                    alias: None,
                },
                AggregateOperation {
                    function: AggregateFunction::Sum,
                    column: "id".into(),
                    alias: None,
                },
                AggregateOperation {
                    function: AggregateFunction::Avg,
                    column: "id".into(),
                    alias: None,
                },
                AggregateOperation {
                    function: AggregateFunction::Min,
                    column: "id".into(),
                    alias: None,
                },
                AggregateOperation {
                    function: AggregateFunction::Max,
                    column: "id".into(),
                    alias: None,
                },
            ],
            filters: vec![],
        };
        let plan = request
            .build(&users_policy())
            .expect("valid aggregate plan");
        assert_eq!(
            plan.sql,
            "SELECT COUNT(\"id\"), SUM(\"id\"), AVG(\"id\"), MIN(\"id\"), MAX(\"id\") FROM \"users\""
        );
    }

    #[test]
    fn rejects_empty_aggregate_operations() {
        let request = AggregateRequest {
            table: "users".into(),
            operations: vec![],
            filters: vec![],
        };
        assert_eq!(
            request.build(&users_policy()),
            Err(QueryBuilderError::EmptyAggregates)
        );
    }

    #[test]
    fn rejects_aggregate_column_outside_policy() {
        let request = AggregateRequest {
            table: "users".into(),
            operations: vec![AggregateOperation {
                function: AggregateFunction::Sum,
                column: "password".into(),
                alias: None,
            }],
            filters: vec![],
        };
        assert!(matches!(
            request.build(&users_policy()),
            Err(QueryBuilderError::Policy(
                PolicyError::ColumnNotAllowed { .. }
            ))
        ));
    }

    #[test]
    fn rejects_search_with_no_searchable_columns() {
        let request = SearchRequest {
            table: "users".into(),
            columns: vec!["id".into()],
            searchable_columns: vec![],
            text: "alice".into(),
            limit: Some(5),
        };
        assert_eq!(
            request.build(&users_policy()),
            Err(QueryBuilderError::EmptySearchColumns)
        );
    }

    #[test]
    fn rejects_search_on_disallowed_column() {
        let request = SearchRequest {
            table: "users".into(),
            columns: vec!["id".into()],
            searchable_columns: vec!["password_hash".into()],
            text: "alice".into(),
            limit: Some(5),
        };
        assert!(matches!(
            request.build(&users_policy()),
            Err(QueryBuilderError::Policy(
                PolicyError::ColumnNotAllowed { .. }
            ))
        ));
    }

    #[test]
    fn builds_between_filter_with_two_binds() {
        let request = SelectRequest {
            table: "users".into(),
            columns: vec!["id".into(), "email".into()],
            filters: vec![Filter {
                column: "id".into(),
                operator: FilterOperator::Between,
                value: BindValue::Integer(10),
                value_to: Some(BindValue::Integer(20)),
            }],
            limit: Some(3),
        };

        let plan = request.build(&users_policy()).expect("valid query plan");
        assert_eq!(
            plan.sql,
            "SELECT \"id\", \"email\" FROM \"users\" WHERE \"id\" BETWEEN $1 AND $2 LIMIT $3"
        );
        assert_eq!(
            plan.binds,
            vec![
                BindValue::Integer(10),
                BindValue::Integer(20),
                BindValue::Integer(3)
            ]
        );
    }

    #[test]
    fn builds_full_text_filter_with_parameterized_tsquery() {
        let request = SelectRequest {
            table: "users".into(),
            columns: vec!["id".into(), "email".into()],
            filters: vec![Filter {
                column: "email".into(),
                operator: FilterOperator::FullText,
                value: BindValue::Text("alice bob".into()),
                value_to: None,
            }],
            limit: Some(3),
        };

        let plan = request.build(&users_policy()).expect("valid query plan");
        assert_eq!(
            plan.sql,
            "SELECT \"id\", \"email\" FROM \"users\" WHERE to_tsvector('simple', coalesce(\"email\"::text, '')) @@ plainto_tsquery('simple', $1) LIMIT $2"
        );
        assert_eq!(
            plan.binds,
            vec![BindValue::Text("alice bob".into()), BindValue::Integer(3)]
        );
    }

    #[test]
    fn rejects_between_without_secondary_value() {
        let request = SelectRequest {
            table: "users".into(),
            columns: vec!["id".into()],
            filters: vec![Filter {
                column: "id".into(),
                operator: FilterOperator::Between,
                value: BindValue::Integer(10),
                value_to: None,
            }],
            limit: Some(1),
        };

        assert_eq!(
            request.build(&users_policy()),
            Err(QueryBuilderError::MissingSecondaryFilterValue("id".into()))
        );
    }

    #[test]
    fn rejects_non_text_value_for_like_or_full_text() {
        let like_request = SelectRequest {
            table: "users".into(),
            columns: vec!["id".into()],
            filters: vec![Filter {
                column: "email".into(),
                operator: FilterOperator::Like,
                value: BindValue::Integer(10),
                value_to: None,
            }],
            limit: Some(1),
        };
        assert_eq!(
            like_request.build(&users_policy()),
            Err(QueryBuilderError::InvalidTextFilterValue("email".into()))
        );

        let full_text_request = SelectRequest {
            table: "users".into(),
            columns: vec!["id".into()],
            filters: vec![Filter {
                column: "email".into(),
                operator: FilterOperator::FullText,
                value: BindValue::Boolean(true),
                value_to: None,
            }],
            limit: Some(1),
        };
        assert_eq!(
            full_text_request.build(&users_policy()),
            Err(QueryBuilderError::InvalidTextFilterValue("email".into()))
        );
    }
}
