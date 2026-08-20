//! MCP-facing request contracts. Transport wiring must call `SelectTool::prepare`.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audit::{AuditEvent, AuditLogger, AuditOutcome};
use crate::backend::postgres::{PostgresBackend, SelectResult};
use crate::error::PublicError;
use crate::metrics::MetricsRegistry;
use crate::policy::Policy;
use crate::query::{
    BindValue, Filter, FilterOperator, QueryBuilderError, QueryPlan, SearchRequest, SelectRequest,
};
use crate::rate_limit::RateLimiter;

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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SelectToolResponse {
    pub request_id: String,
    pub columns: Vec<String>,
    pub rows: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SearchToolRequest {
    pub request_id: String,
    pub table: String,
    pub columns: Vec<String>,
    pub searchable_columns: Vec<String>,
    pub text: String,
    pub limit: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub struct PreparedSearch {
    pub request_id: String,
    pub plan: QueryPlan,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchToolResponse {
    pub request_id: String,
    pub columns: Vec<String>,
    pub rows: Vec<Value>,
}

pub struct SelectTool<'a> {
    policy: &'a Policy,
    audit: Option<&'a AuditLogger>,
    rate_limiter: Option<&'a RateLimiter>,
    metrics: Option<&'a MetricsRegistry>,
}

pub struct SearchTool<'a> {
    policy: &'a Policy,
    audit: Option<&'a AuditLogger>,
    rate_limiter: Option<&'a RateLimiter>,
    metrics: Option<&'a MetricsRegistry>,
}

impl<'a> SelectTool<'a> {
    pub fn new(policy: &'a Policy, audit: Option<&'a AuditLogger>) -> Self {
        Self {
            policy,
            audit,
            rate_limiter: None,
            metrics: None,
        }
    }

    pub fn with_rate_limiter(
        policy: &'a Policy,
        audit: Option<&'a AuditLogger>,
        rate_limiter: &'a RateLimiter,
    ) -> Self {
        Self {
            policy,
            audit,
            rate_limiter: Some(rate_limiter),
            metrics: None,
        }
    }

    pub fn with_metrics(mut self, metrics: &'a MetricsRegistry) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Converts the MCP request into a policy-validated internal query plan.
    /// The plan remains internal and is never serialized as an MCP response.
    pub async fn prepare(&self, request: SelectToolRequest) -> Result<PreparedSelect, PublicError> {
        let started = Instant::now();
        let request_id = request.request_id.clone();
        let operation = "select";
        if let Some(rate_limiter) = self.rate_limiter {
            if !rate_limiter.allow(&request_id) {
                self.record_audit(
                    AuditEvent::new(&request_id, operation)
                        .outcome(AuditOutcome::Rejected)
                        .elapsed(started.elapsed()),
                )
                .await;
                return Err(PublicError::rate_limited());
            }
        }
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

    /// Executes a prepared select through the read-only backend and returns rows only.
    pub async fn execute(
        &self,
        backend: &PostgresBackend,
        request: SelectToolRequest,
    ) -> Result<SelectToolResponse, PublicError> {
        let started = Instant::now();
        let prepared = match self.prepare(request).await {
            Ok(prepared) => prepared,
            Err(error) => {
                self.record_rejected_metric(started.elapsed());
                return Err(error);
            }
        };
        let result = backend.execute_select(&prepared.plan).await;
        let result = match result {
            Ok(result) => result,
            Err(_) => {
                self.record_backend_error_metric(started.elapsed());
                return Err(PublicError::backend_unavailable());
            }
        };

        self.record_pool_metrics(backend);
        let response = response_from_result(prepared.request_id, result);
        if let Err(error) = enforce_output_limit(self.policy, &response) {
            self.record_rejected_metric(started.elapsed());
            return Err(error);
        }

        self.record_accepted_metric(started.elapsed());
        Ok(response)
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

    fn record_accepted_metric(&self, duration: std::time::Duration) {
        if let Some(metrics) = self.metrics {
            metrics.record_accepted(elapsed_ms(duration));
        }
    }

    fn record_rejected_metric(&self, duration: std::time::Duration) {
        if let Some(metrics) = self.metrics {
            metrics.record_rejected(elapsed_ms(duration));
        }
    }

    fn record_backend_error_metric(&self, duration: std::time::Duration) {
        if let Some(metrics) = self.metrics {
            metrics.record_backend_error();
            metrics.record_rejected(elapsed_ms(duration));
        }
    }

    fn record_pool_metrics(&self, backend: &PostgresBackend) {
        if let Some(metrics) = self.metrics {
            metrics.set_pool_metrics(backend.pool_metrics());
        }
    }
}

impl<'a> SearchTool<'a> {
    pub fn new(policy: &'a Policy, audit: Option<&'a AuditLogger>) -> Self {
        Self {
            policy,
            audit,
            rate_limiter: None,
            metrics: None,
        }
    }

    pub fn with_rate_limiter(
        policy: &'a Policy,
        audit: Option<&'a AuditLogger>,
        rate_limiter: &'a RateLimiter,
    ) -> Self {
        Self {
            policy,
            audit,
            rate_limiter: Some(rate_limiter),
            metrics: None,
        }
    }

    pub fn with_metrics(mut self, metrics: &'a MetricsRegistry) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub async fn prepare(&self, request: SearchToolRequest) -> Result<PreparedSearch, PublicError> {
        let started = Instant::now();
        let request_id = request.request_id.clone();
        let operation = "search";
        if let Some(rate_limiter) = self.rate_limiter {
            if !rate_limiter.allow(&request_id) {
                self.record_audit(
                    AuditEvent::new(&request_id, operation)
                        .outcome(AuditOutcome::Rejected)
                        .elapsed(started.elapsed()),
                )
                .await;
                return Err(PublicError::rate_limited());
            }
        }

        let internal = SearchRequest {
            table: request.table,
            columns: request.columns,
            searchable_columns: request.searchable_columns,
            text: request.text,
            limit: request.limit,
        };
        let result = internal
            .build(self.policy)
            .map(|plan| PreparedSearch {
                request_id: request_id.clone(),
                plan,
            })
            .map_err(public_error_for_query);

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

    pub async fn execute(
        &self,
        backend: &PostgresBackend,
        request: SearchToolRequest,
    ) -> Result<SearchToolResponse, PublicError> {
        let started = Instant::now();
        let prepared = match self.prepare(request).await {
            Ok(prepared) => prepared,
            Err(error) => {
                self.record_rejected_metric(started.elapsed());
                return Err(error);
            }
        };
        let result = backend.execute_select(&prepared.plan).await;
        let result = match result {
            Ok(result) => result,
            Err(_) => {
                self.record_backend_error_metric(started.elapsed());
                return Err(PublicError::backend_unavailable());
            }
        };

        self.record_pool_metrics(backend);
        let response = search_response_from_result(prepared.request_id, result);
        if let Err(error) = enforce_search_output_limit(self.policy, &response) {
            self.record_rejected_metric(started.elapsed());
            return Err(error);
        }

        self.record_accepted_metric(started.elapsed());
        Ok(response)
    }

    async fn record_audit(&self, event: AuditEvent) {
        if let Some(audit) = self.audit {
            let _ = audit.record(&event).await;
        }
    }

    fn record_accepted_metric(&self, duration: std::time::Duration) {
        if let Some(metrics) = self.metrics {
            metrics.record_accepted(elapsed_ms(duration));
        }
    }

    fn record_rejected_metric(&self, duration: std::time::Duration) {
        if let Some(metrics) = self.metrics {
            metrics.record_rejected(elapsed_ms(duration));
        }
    }

    fn record_backend_error_metric(&self, duration: std::time::Duration) {
        if let Some(metrics) = self.metrics {
            metrics.record_backend_error();
            metrics.record_rejected(elapsed_ms(duration));
        }
    }

    fn record_pool_metrics(&self, backend: &PostgresBackend) {
        if let Some(metrics) = self.metrics {
            metrics.set_pool_metrics(backend.pool_metrics());
        }
    }
}

fn elapsed_ms(duration: std::time::Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn response_from_result(request_id: String, result: SelectResult) -> SelectToolResponse {
    SelectToolResponse {
        request_id,
        columns: result.columns,
        rows: result.rows,
    }
}

fn search_response_from_result(request_id: String, result: SelectResult) -> SearchToolResponse {
    SearchToolResponse {
        request_id,
        columns: result.columns,
        rows: result.rows,
    }
}

fn enforce_output_limit(policy: &Policy, response: &SelectToolResponse) -> Result<(), PublicError> {
    let output_bytes = serde_json::to_vec(response)
        .map_err(|_| PublicError::internal())?
        .len();
    let output_bytes = u32::try_from(output_bytes).unwrap_or(u32::MAX);
    policy
        .check_output_bytes(output_bytes)
        .map_err(|_| PublicError::policy_denied())
}

fn enforce_search_output_limit(
    policy: &Policy,
    response: &SearchToolResponse,
) -> Result<(), PublicError> {
    let output_bytes = serde_json::to_vec(response)
        .map_err(|_| PublicError::internal())?
        .len();
    let output_bytes = u32::try_from(output_bytes).unwrap_or(u32::MAX);
    policy
        .check_output_bytes(output_bytes)
        .map_err(|_| PublicError::policy_denied())
}

fn public_error_for_query(error: QueryBuilderError) -> PublicError {
    match error {
        QueryBuilderError::Policy(_) => PublicError::policy_denied(),
        QueryBuilderError::InvalidIdentifier(_)
        | QueryBuilderError::EmptyColumns
        | QueryBuilderError::EmptySearchColumns => PublicError::invalid_request(),
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
            max_query_complexity: 100,
            max_output_bytes: 10_000,
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

    #[tokio::test]
    async fn rejects_requests_after_rate_limit_is_reached() {
        let limiter = RateLimiter::new(1, std::time::Duration::from_secs(60)).expect("limiter");
        let request = || SelectToolRequest {
            request_id: "client-1".into(),
            table: "users".into(),
            columns: vec!["id".into()],
            filters: vec![],
            limit: Some(1),
        };
        let request_policy = policy();
        let tool = SelectTool::with_rate_limiter(&request_policy, None, &limiter);
        tool.prepare(request()).await.expect("first request");
        assert_eq!(
            tool.prepare(request()).await.unwrap_err(),
            PublicError::rate_limited()
        );
    }

    #[test]
    fn rejects_response_that_exceeds_output_limit() {
        let mut tables = HashMap::new();
        tables.insert(
            "users".into(),
            TableConfig {
                columns: vec!["id".into(), "email".into()],
            },
        );
        let strict_policy = Policy::new(PolicyConfig {
            tables,
            default_row_limit: 20,
            max_row_limit: 100,
            max_query_complexity: 100,
            max_output_bytes: 40,
        });
        let response = SelectToolResponse {
            request_id: "request-4".into(),
            columns: vec!["id".into(), "email".into()],
            rows: vec![serde_json::json!({"id": 1, "email": "a@b.c"})],
        };

        assert_eq!(
            enforce_output_limit(&strict_policy, &response).unwrap_err(),
            PublicError::policy_denied()
        );
    }

    #[tokio::test]
    async fn prepares_search_request_without_exposing_sql_in_response() {
        let request = SearchToolRequest {
            request_id: "search-1".into(),
            table: "users".into(),
            columns: vec!["id".into(), "email".into()],
            searchable_columns: vec!["email".into()],
            text: "alice".into(),
            limit: Some(10),
        };

        let prepared = SearchTool::new(&policy(), None)
            .prepare(request)
            .await
            .expect("valid search request");
        assert_eq!(prepared.request_id, "search-1");
        assert_eq!(
            prepared.plan.binds,
            vec![BindValue::Text("%alice%".into()), BindValue::Integer(10)]
        );
    }

    #[tokio::test]
    async fn search_rejects_policy_denied_table() {
        let request = SearchToolRequest {
            request_id: "search-2".into(),
            table: "secrets".into(),
            columns: vec!["value".into()],
            searchable_columns: vec!["value".into()],
            text: "x".into(),
            limit: Some(1),
        };
        let error = SearchTool::new(&policy(), None)
            .prepare(request)
            .await
            .unwrap_err();
        assert_eq!(error, PublicError::policy_denied());
    }

    #[test]
    fn search_output_limit_rejection_is_sanitized() {
        let mut tables = HashMap::new();
        tables.insert(
            "users".into(),
            TableConfig {
                columns: vec!["id".into(), "email".into()],
            },
        );
        let strict_policy = Policy::new(PolicyConfig {
            tables,
            default_row_limit: 20,
            max_row_limit: 100,
            max_query_complexity: 100,
            max_output_bytes: 40,
        });
        let response = SearchToolResponse {
            request_id: "search-3".into(),
            columns: vec!["id".into(), "email".into()],
            rows: vec![serde_json::json!({"id": 1, "email": "a@b.c"})],
        };

        assert_eq!(
            enforce_search_output_limit(&strict_policy, &response).unwrap_err(),
            PublicError::policy_denied()
        );
    }

    #[test]
    fn accepts_response_within_output_limit() {
        let response = SelectToolResponse {
            request_id: "request-5".into(),
            columns: vec!["id".into()],
            rows: vec![serde_json::json!({"id": 1})],
        };
        assert!(enforce_output_limit(&policy(), &response).is_ok());
    }

    #[test]
    fn rejects_u64_json_numbers_that_do_not_fit_i64() {
        let too_large = serde_json::Number::from(u64::MAX);
        let err = bind_value(Value::Number(too_large)).unwrap_err();
        assert_eq!(err, PublicError::invalid_request());
    }

    #[test]
    fn maps_all_select_tool_operators() {
        assert_eq!(
            FilterOperator::from(SelectToolOperator::Equals),
            FilterOperator::Equals
        );
        assert_eq!(
            FilterOperator::from(SelectToolOperator::NotEquals),
            FilterOperator::NotEquals
        );
        assert_eq!(
            FilterOperator::from(SelectToolOperator::LessThan),
            FilterOperator::LessThan
        );
        assert_eq!(
            FilterOperator::from(SelectToolOperator::LessThanOrEqual),
            FilterOperator::LessThanOrEqual
        );
        assert_eq!(
            FilterOperator::from(SelectToolOperator::GreaterThan),
            FilterOperator::GreaterThan
        );
        assert_eq!(
            FilterOperator::from(SelectToolOperator::GreaterThanOrEqual),
            FilterOperator::GreaterThanOrEqual
        );
        assert_eq!(
            FilterOperator::from(SelectToolOperator::Like),
            FilterOperator::Like
        );
        assert_eq!(
            FilterOperator::from(SelectToolOperator::ILike),
            FilterOperator::ILike
        );
    }

    #[test]
    fn elapsed_ms_saturates_at_u64_max() {
        let duration = std::time::Duration::from_secs(u64::MAX);
        assert_eq!(elapsed_ms(duration), u64::MAX);
    }
}
