//! MCP-facing request contracts. Transport wiring must call `SelectTool::prepare`.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audit::{AuditEvent, AuditLogger, AuditOutcome};
use crate::error::PublicError;
use crate::policy::Policy;
use crate::query::{
    BindValue, Filter, FilterOperator, QueryBuilderError, QueryPlan, SelectRequest,
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SelectToolRequest {
    pub request_id: String,
    pub table: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub filters: Vec<SelectToolFilter>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SelectToolFilter {
    pub column: String,
    pub operator: SelectToolOperator,
    pub value: Value,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectToolOperator {
    Equals,
    NotEquals,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Like,
    ILike,
}

#[derive(Debug, PartialEq)]
pub struct PreparedSelect {
    pub request_id: String,
    pub plan: QueryPlan,
}

pub struct SelectTool<'a> {
    policy: &'a Policy,
    audit: Option<&'a AuditLogger>,
}

impl<'a> SelectTool<'a> {
    pub fn new(policy: &'a Policy, audit: Option<&'a AuditLogger>) -> Self {
        Self { policy, audit }
    }

    /// Converts the MCP request into a policy-validated internal query plan.
    /// The plan remains internal and is never serialized as an MCP response.
    pub async fn prepare(&self, request: SelectToolRequest) -> Result<PreparedSelect, PublicError> {
        let started = Instant::now();
        let request_id = request.request_id.clone();
        let operation = "select";
        let internal = self.convert_request(request);
        let result = internal.and_then(|request| {
            request
                .build(self.policy)
                .map(|plan| PreparedSelect {
                    request_id: request_id.clone(),
                    plan,
                })
                .map_err(public_error_for_query)
        });

        match result {
            Ok(prepared) => {
                self.record_audit(
                    AuditEvent::new(&request_id, operation)
                        .outcome(AuditOutcome::Accepted)
                        .elapsed(started.elapsed()),
                )
                .await;
                Ok(prepared)
            }
            Err(error) => {
                self.record_audit(
                    AuditEvent::new(&request_id, operation)
                        .outcome(AuditOutcome::Rejected)
                        .elapsed(started.elapsed()),
                )
                .await;
                Err(error)
            }
        }
    }

    fn convert_request(&self, request: SelectToolRequest) -> Result<SelectRequest, PublicError> {
        let filters = request
            .filters
            .into_iter()
            .map(|filter| {
                Ok(Filter {
                    column: filter.column,
                    operator: filter.operator.into(),
                    value: bind_value(filter.value)?,
                })
            })
            .collect::<Result<Vec<_>, PublicError>>()?;

        Ok(SelectRequest {
            table: request.table,
            columns: request.columns,
            filters,
            limit: request.limit,
        })
    }

    async fn record_audit(&self, event: AuditEvent) {
        if let Some(audit) = self.audit {
            let _ = audit.record(&event).await;
        }
    }
}

fn public_error_for_query(error: QueryBuilderError) -> PublicError {
    match error {
        QueryBuilderError::Policy(_) => PublicError::policy_denied(),
        QueryBuilderError::InvalidIdentifier(_) | QueryBuilderError::EmptyColumns => {
            PublicError::invalid_request()
        }
    }
}

impl From<SelectToolOperator> for FilterOperator {
    fn from(operator: SelectToolOperator) -> Self {
        match operator {
            SelectToolOperator::Equals => Self::Equals,
            SelectToolOperator::NotEquals => Self::NotEquals,
            SelectToolOperator::LessThan => Self::LessThan,
            SelectToolOperator::LessThanOrEqual => Self::LessThanOrEqual,
            SelectToolOperator::GreaterThan => Self::GreaterThan,
            SelectToolOperator::GreaterThanOrEqual => Self::GreaterThanOrEqual,
            SelectToolOperator::Like => Self::Like,
            SelectToolOperator::ILike => Self::ILike,
        }
    }
}

fn bind_value(value: Value) -> Result<BindValue, PublicError> {
    match value {
        Value::String(value) => Ok(BindValue::Text(value)),
        Value::Number(value) if value.is_i64() => value
            .as_i64()
            .map(BindValue::Integer)
            .ok_or_else(PublicError::invalid_request),
        Value::Number(value) if value.is_f64() => value
            .as_f64()
            .map(BindValue::Decimal)
            .ok_or_else(PublicError::invalid_request),
        Value::Bool(value) => Ok(BindValue::Boolean(value)),
        Value::Null | Value::Array(_) | Value::Object(_) => Err(PublicError::invalid_request()),
        Value::Number(_) => Err(PublicError::invalid_request()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::{PolicyConfig, TableConfig};

    fn policy() -> Policy {
        let mut tables = HashMap::new();
        tables.insert(
            "users".into(),
            TableConfig {
                columns: vec!["id".into(), "email".into(), "active".into()],
            },
        );
        Policy::new(PolicyConfig {
            tables,
            default_row_limit: 20,
            max_row_limit: 100,
        })
    }

    #[tokio::test]
    async fn prepares_typed_select_without_exposing_sql_in_response() {
        let request = SelectToolRequest {
            request_id: "request-1".into(),
            table: "users".into(),
            columns: vec!["id".into(), "email".into()],
            filters: vec![SelectToolFilter {
                column: "active".into(),
                operator: SelectToolOperator::Equals,
                value: Value::Bool(true),
            }],
            limit: Some(10),
        };

        let prepared = SelectTool::new(&policy(), None)
            .prepare(request)
            .await
            .expect("valid select request");
        assert_eq!(prepared.request_id, "request-1");
        assert_eq!(
            prepared.plan.binds,
            vec![BindValue::Boolean(true), BindValue::Integer(10)]
        );
        let json = serde_json::to_string(&prepared.request_id).expect("serializable id");
        assert!(!json.contains("SELECT"));
    }

    #[tokio::test]
    async fn maps_policy_rejection_to_sanitized_public_error() {
        let request = SelectToolRequest {
            request_id: "request-2".into(),
            table: "secrets".into(),
            columns: vec!["value".into()],
            filters: vec![],
            limit: Some(1),
        };
        let error = SelectTool::new(&policy(), None)
            .prepare(request)
            .await
            .unwrap_err();
        assert_eq!(error, PublicError::policy_denied());
    }

    #[tokio::test]
    async fn rejects_complex_json_filter_values() {
        let request = SelectToolRequest {
            request_id: "request-3".into(),
            table: "users".into(),
            columns: vec!["id".into()],
            filters: vec![SelectToolFilter {
                column: "active".into(),
                operator: SelectToolOperator::Equals,
                value: serde_json::json!({"unexpected": "object"}),
            }],
            limit: Some(1),
        };
        let error = SelectTool::new(&policy(), None)
            .prepare(request)
            .await
            .unwrap_err();
        assert_eq!(error, PublicError::invalid_request());
    }
}
