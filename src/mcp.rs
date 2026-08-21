//! MCP-facing request contracts. Transport wiring must call `SelectTool::prepare`.

use std::future::Future;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audit::{AuditEvent, AuditLogger, AuditOutcome};
use crate::backend::postgres::{PostgresBackend, SelectResult};
use crate::error::PublicError;
use crate::metrics::MetricsRegistry;
use crate::policy::Policy;
use crate::query::{
    AggregateFunction, AggregateOperation, AggregateRequest, BindValue, Filter, FilterOperator,
    QueryBuilderError, QueryPlan, SearchRequest, SelectRequest,
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
    #[serde(default)]
    pub value_to: Option<Value>,
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
    Between,
    FullText,
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AggregateToolRequest {
    pub request_id: String,
    pub table: String,
    pub operations: Vec<AggregateToolOperation>,
    #[serde(default)]
    pub filters: Vec<SelectToolFilter>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AggregateToolFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AggregateToolOperation {
    pub function: AggregateToolFunction,
    pub column: String,
    pub alias: Option<String>,
}

pub type PreparedSearch = PreparedSelect;

pub type PreparedAggregate = PreparedSelect;

pub type SearchToolResponse = SelectToolResponse;

pub type AggregateToolResponse = SelectToolResponse;

pub struct SelectTool<'a> {
    runtime: ToolRuntime<'a>,
}

pub struct SearchTool<'a> {
    runtime: ToolRuntime<'a>,
}

pub struct AggregateTool<'a> {
    runtime: ToolRuntime<'a>,
}

#[derive(Clone, Copy)]
struct ToolRuntime<'a> {
    policy: &'a Policy,
    audit: Option<&'a AuditLogger>,
    rate_limiter: Option<&'a RateLimiter>,
    metrics: Option<&'a MetricsRegistry>,
}

impl<'a> SelectTool<'a> {
    pub fn new(policy: &'a Policy, audit: Option<&'a AuditLogger>) -> Self {
        Self {
            runtime: ToolRuntime::new(policy, audit),
        }
    }

    pub fn with_rate_limiter(
        policy: &'a Policy,
        audit: Option<&'a AuditLogger>,
        rate_limiter: &'a RateLimiter,
    ) -> Self {
        Self {
            runtime: ToolRuntime::with_rate_limiter(policy, audit, rate_limiter),
        }
    }

    pub fn with_metrics(mut self, metrics: &'a MetricsRegistry) -> Self {
        self.runtime = self.runtime.with_metrics(metrics);
        self
    }

    /// Converts the MCP request into a policy-validated internal query plan.
    /// The plan remains internal and is never serialized as an MCP response.
    pub async fn prepare(&self, request: SelectToolRequest) -> Result<PreparedSelect, PublicError> {
        let request_id = request.request_id.clone();
        let internal = self.convert_request(request)?;
        let prepared_request_id = request_id.clone();

        let plan = self
            .runtime
            .prepare_plan(&request_id, "select", move |policy| internal.build(policy))
            .await?;
        Ok(PreparedSelect {
            request_id: prepared_request_id,
            plan,
        })
    }

    /// Executes a prepared select through the read-only backend and returns rows only.
    pub async fn execute(
        &self,
        backend: &PostgresBackend,
        request: SelectToolRequest,
    ) -> Result<SelectToolResponse, PublicError> {
        self.runtime
            .execute(
                backend,
                self.prepare(request),
                |prepared| (prepared.request_id, prepared.plan),
                response_from_result,
                |response| enforce_serialized_output_limit(self.runtime.policy, response),
            )
            .await
    }

    fn convert_request(&self, request: SelectToolRequest) -> Result<SelectRequest, PublicError> {
        Ok(SelectRequest {
            table: request.table,
            columns: request.columns,
            filters: convert_filters(request.filters)?,
            limit: request.limit,
        })
    }
}

impl<'a> SearchTool<'a> {
    pub fn new(policy: &'a Policy, audit: Option<&'a AuditLogger>) -> Self {
        Self {
            runtime: ToolRuntime::new(policy, audit),
        }
    }

    pub fn with_rate_limiter(
        policy: &'a Policy,
        audit: Option<&'a AuditLogger>,
        rate_limiter: &'a RateLimiter,
    ) -> Self {
        Self {
            runtime: ToolRuntime::with_rate_limiter(policy, audit, rate_limiter),
        }
    }

    pub fn with_metrics(mut self, metrics: &'a MetricsRegistry) -> Self {
        self.runtime = self.runtime.with_metrics(metrics);
        self
    }

    pub async fn prepare(&self, request: SearchToolRequest) -> Result<PreparedSearch, PublicError> {
        let request_id = request.request_id.clone();
        let internal = SearchRequest {
            table: request.table,
            columns: request.columns,
            searchable_columns: request.searchable_columns,
            text: request.text,
            limit: request.limit,
        };
        let prepared_request_id = request_id.clone();

        let plan = self
            .runtime
            .prepare_plan(&request_id, "search", move |policy| internal.build(policy))
            .await?;
        Ok(PreparedSearch {
            request_id: prepared_request_id,
            plan,
        })
    }

    pub async fn execute(
        &self,
        backend: &PostgresBackend,
        request: SearchToolRequest,
    ) -> Result<SearchToolResponse, PublicError> {
        self.runtime
            .execute(
                backend,
                self.prepare(request),
                |prepared| (prepared.request_id, prepared.plan),
                response_from_result,
                |response| enforce_serialized_output_limit(self.runtime.policy, response),
            )
            .await
    }
}

impl<'a> AggregateTool<'a> {
    pub fn new(policy: &'a Policy, audit: Option<&'a AuditLogger>) -> Self {
        Self {
            runtime: ToolRuntime::new(policy, audit),
        }
    }

    pub fn with_rate_limiter(
        policy: &'a Policy,
        audit: Option<&'a AuditLogger>,
        rate_limiter: &'a RateLimiter,
    ) -> Self {
        Self {
            runtime: ToolRuntime::with_rate_limiter(policy, audit, rate_limiter),
        }
    }

    pub fn with_metrics(mut self, metrics: &'a MetricsRegistry) -> Self {
        self.runtime = self.runtime.with_metrics(metrics);
        self
    }

    pub async fn prepare(
        &self,
        request: AggregateToolRequest,
    ) -> Result<PreparedAggregate, PublicError> {
        let request_id = request.request_id.clone();
        let internal = AggregateRequest {
            table: request.table,
            operations: request
                .operations
                .into_iter()
                .map(|operation| AggregateOperation {
                    function: operation.function.into(),
                    column: operation.column,
                    alias: operation.alias,
                })
                .collect(),
            filters: convert_filters(request.filters)?,
        };
        let prepared_request_id = request_id.clone();

        let plan = self
            .runtime
            .prepare_plan(&request_id, "aggregate", move |policy| {
                internal.build(policy)
            })
            .await?;
        Ok(PreparedAggregate {
            request_id: prepared_request_id,
            plan,
        })
    }

    pub async fn execute(
        &self,
        backend: &PostgresBackend,
        request: AggregateToolRequest,
    ) -> Result<AggregateToolResponse, PublicError> {
        self.runtime
            .execute(
                backend,
                self.prepare(request),
                |prepared| (prepared.request_id, prepared.plan),
                response_from_result,
                |response| enforce_serialized_output_limit(self.runtime.policy, response),
            )
            .await
    }
}

fn convert_filters(filters: Vec<SelectToolFilter>) -> Result<Vec<Filter>, PublicError> {
    filters
        .into_iter()
        .map(|filter| {
            let value_to = filter.value_to.map(bind_value).transpose()?;
            Ok(Filter {
                column: filter.column,
                operator: filter.operator.into(),
                value: bind_value(filter.value)?,
                value_to,
            })
        })
        .collect()
}

impl From<AggregateToolFunction> for AggregateFunction {
    fn from(function: AggregateToolFunction) -> Self {
        match function {
            AggregateToolFunction::Count => Self::Count,
            AggregateToolFunction::Sum => Self::Sum,
            AggregateToolFunction::Avg => Self::Avg,
            AggregateToolFunction::Min => Self::Min,
            AggregateToolFunction::Max => Self::Max,
        }
    }
}

impl<'a> ToolRuntime<'a> {
    fn new(policy: &'a Policy, audit: Option<&'a AuditLogger>) -> Self {
        Self {
            policy,
            audit,
            rate_limiter: None,
            metrics: None,
        }
    }

    fn with_rate_limiter(
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

    fn with_metrics(mut self, metrics: &'a MetricsRegistry) -> Self {
        self.metrics = Some(metrics);
        self
    }

    async fn prepare<T, F>(
        &self,
        request_id: &str,
        operation: &str,
        build: F,
    ) -> Result<T, PublicError>
    where
        F: FnOnce(&Policy) -> Result<T, PublicError>,
    {
        let started = Instant::now();
        if let Some(rate_limiter) = self.rate_limiter {
            if !rate_limiter.allow(request_id) {
                self.record_audit(
                    AuditEvent::new(request_id, operation)
                        .outcome(AuditOutcome::Rejected)
                        .elapsed(started.elapsed()),
                )
                .await;
                return Err(PublicError::rate_limited());
            }
        }

        let result = build(self.policy);
        let outcome = if result.is_ok() {
            AuditOutcome::Accepted
        } else {
            AuditOutcome::Rejected
        };
        self.record_audit(
            AuditEvent::new(request_id, operation)
                .outcome(outcome)
                .elapsed(started.elapsed()),
        )
        .await;

        result
    }

    async fn record_audit(&self, event: AuditEvent) {
        if let Some(audit) = self.audit {
            let _ = audit.record(&event).await;
        }
    }

    async fn prepare_plan<F>(
        &self,
        request_id: &str,
        operation: &str,
        build: F,
    ) -> Result<QueryPlan, PublicError>
    where
        F: FnOnce(&Policy) -> Result<QueryPlan, QueryBuilderError>,
    {
        self.prepare(request_id, operation, |policy| {
            build(policy).map_err(public_error_for_query)
        })
        .await
    }

    async fn execute<P, T, Prepare, Split, BuildResponse, CheckOutput>(
        &self,
        backend: &PostgresBackend,
        prepare: Prepare,
        split: Split,
        build_response: BuildResponse,
        check_output: CheckOutput,
    ) -> Result<T, PublicError>
    where
        Prepare: Future<Output = Result<P, PublicError>>,
        Split: FnOnce(P) -> (String, QueryPlan),
        BuildResponse: FnOnce(String, SelectResult) -> T,
        CheckOutput: FnOnce(&T) -> Result<(), PublicError>,
    {
        let started = Instant::now();
        let prepared = match prepare.await {
            Ok(prepared) => prepared,
            Err(error) => {
                self.record_rejected_metric(started.elapsed());
                return Err(error);
            }
        };
        let (request_id, plan) = split(prepared);
        let result = match backend.execute_select(&plan).await {
            Ok(result) => result,
            Err(_) => {
                self.record_backend_error_metric(started.elapsed());
                return Err(PublicError::backend_unavailable());
            }
        };

        self.record_pool_metrics(backend);
        let response = build_response(request_id, result);
        if let Err(error) = check_output(&response) {
            self.record_rejected_metric(started.elapsed());
            return Err(error);
        }

        self.record_accepted_metric(started.elapsed());
        Ok(response)
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

fn enforce_serialized_output_limit<T: Serialize>(
    policy: &Policy,
    response: &T,
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
        | QueryBuilderError::EmptySearchColumns
        | QueryBuilderError::EmptyAggregates
        | QueryBuilderError::MissingSecondaryFilterValue(_)
        | QueryBuilderError::UnexpectedSecondaryFilterValue(_)
        | QueryBuilderError::InvalidTextFilterValue(_) => PublicError::invalid_request(),
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
            SelectToolOperator::Between => Self::Between,
            SelectToolOperator::FullText => Self::FullText,
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
                filter_operators: HashMap::new(),
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
                value_to: None,
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
                value_to: None,
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
                filter_operators: HashMap::new(),
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
            enforce_serialized_output_limit(&strict_policy, &response).unwrap_err(),
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
    async fn prepares_aggregate_request_with_policy_checked_operations() {
        let request = AggregateToolRequest {
            request_id: "aggregate-1".into(),
            table: "users".into(),
            operations: vec![
                AggregateToolOperation {
                    function: AggregateToolFunction::Count,
                    column: "*".into(),
                    alias: Some("total".into()),
                },
                AggregateToolOperation {
                    function: AggregateToolFunction::Avg,
                    column: "id".into(),
                    alias: Some("average_id".into()),
                },
            ],
            filters: vec![SelectToolFilter {
                column: "active".into(),
                operator: SelectToolOperator::Equals,
                value: Value::Bool(true),
                value_to: None,
            }],
        };

        let prepared = AggregateTool::new(&policy(), None)
            .prepare(request)
            .await
            .expect("valid aggregate request");
        assert_eq!(prepared.request_id, "aggregate-1");
        assert_eq!(
            prepared.plan.sql,
            "SELECT COUNT(*) AS \"total\", AVG(\"id\") AS \"average_id\" FROM \"users\" WHERE \"active\" = $1"
        );
        assert_eq!(prepared.plan.binds, vec![BindValue::Boolean(true)]);
    }

    #[tokio::test]
    async fn aggregate_rejects_empty_operations() {
        let request = AggregateToolRequest {
            request_id: "aggregate-2".into(),
            table: "users".into(),
            operations: vec![],
            filters: vec![],
        };
        assert_eq!(
            AggregateTool::new(&policy(), None)
                .prepare(request)
                .await
                .unwrap_err(),
            PublicError::invalid_request()
        );
    }

    #[tokio::test]
    async fn aggregate_rejects_disallowed_operation_column() {
        let request = AggregateToolRequest {
            request_id: "aggregate-3".into(),
            table: "users".into(),
            operations: vec![AggregateToolOperation {
                function: AggregateToolFunction::Sum,
                column: "password".into(),
                alias: None,
            }],
            filters: vec![],
        };
        assert_eq!(
            AggregateTool::new(&policy(), None)
                .prepare(request)
                .await
                .unwrap_err(),
            PublicError::policy_denied()
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
                filter_operators: HashMap::new(),
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
            enforce_serialized_output_limit(&strict_policy, &response).unwrap_err(),
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
        assert!(enforce_serialized_output_limit(&policy(), &response).is_ok());
    }

    #[test]
    fn rejects_u64_json_numbers_that_do_not_fit_i64() {
        let too_large = serde_json::Number::from(u64::MAX);
        let err = bind_value(Value::Number(too_large)).unwrap_err();
        assert_eq!(err, PublicError::invalid_request());
    }

    #[test]
    fn converts_supported_json_filter_values() {
        assert_eq!(
            bind_value(Value::String("alice".into())).unwrap(),
            BindValue::Text("alice".into())
        );
        assert_eq!(
            bind_value(serde_json::json!(42)).unwrap(),
            BindValue::Integer(42)
        );
        assert_eq!(
            bind_value(serde_json::json!(4.2)).unwrap(),
            BindValue::Decimal(4.2)
        );
        assert_eq!(
            bind_value(Value::Bool(true)).unwrap(),
            BindValue::Boolean(true)
        );
    }

    #[test]
    fn rejects_unsupported_json_filter_values() {
        for value in [
            Value::Null,
            serde_json::json!([1, 2]),
            serde_json::json!({"key": "value"}),
        ] {
            assert_eq!(
                bind_value(value).unwrap_err(),
                PublicError::invalid_request()
            );
        }
    }

    #[tokio::test]
    async fn rejects_empty_select_columns() {
        let request = SelectToolRequest {
            request_id: "request-6".into(),
            table: "users".into(),
            columns: vec![],
            filters: vec![],
            limit: Some(1),
        };
        assert_eq!(
            SelectTool::new(&policy(), None)
                .prepare(request)
                .await
                .unwrap_err(),
            PublicError::invalid_request()
        );
    }

    #[tokio::test]
    async fn rejects_empty_search_columns() {
        let request = SearchToolRequest {
            request_id: "search-4".into(),
            table: "users".into(),
            columns: vec!["id".into()],
            searchable_columns: vec![],
            text: "alice".into(),
            limit: Some(1),
        };
        assert_eq!(
            SearchTool::new(&policy(), None)
                .prepare(request)
                .await
                .unwrap_err(),
            PublicError::invalid_request()
        );
    }

    #[tokio::test]
    async fn rejects_search_with_unauthorized_searchable_column() {
        let request = SearchToolRequest {
            request_id: "search-5".into(),
            table: "users".into(),
            columns: vec!["id".into()],
            searchable_columns: vec!["password".into()],
            text: "alice".into(),
            limit: Some(1),
        };
        assert_eq!(
            SearchTool::new(&policy(), None)
                .prepare(request)
                .await
                .unwrap_err(),
            PublicError::policy_denied()
        );
    }

    #[test]
    fn maps_query_builder_errors_to_public_errors() {
        assert_eq!(
            public_error_for_query(QueryBuilderError::Policy(
                crate::policy::PolicyError::TableNotAllowed("users".into())
            )),
            PublicError::policy_denied()
        );
        assert_eq!(
            public_error_for_query(QueryBuilderError::InvalidIdentifier("bad".into())),
            PublicError::invalid_request()
        );
        assert_eq!(
            public_error_for_query(QueryBuilderError::EmptyColumns),
            PublicError::invalid_request()
        );
        assert_eq!(
            public_error_for_query(QueryBuilderError::EmptySearchColumns),
            PublicError::invalid_request()
        );
    }

    #[test]
    fn records_runtime_metrics_when_configured() {
        let metrics = MetricsRegistry::new();
        let request_policy = policy();
        let runtime = ToolRuntime::new(&request_policy, None).with_metrics(&metrics);
        runtime.record_accepted_metric(std::time::Duration::from_millis(3));
        runtime.record_rejected_metric(std::time::Duration::from_millis(4));
        runtime.record_backend_error_metric(std::time::Duration::from_millis(5));
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests_accepted, 1);
        assert_eq!(snapshot.requests_rejected, 2);
        assert_eq!(snapshot.backend_errors, 1);
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
        assert_eq!(
            FilterOperator::from(SelectToolOperator::Between),
            FilterOperator::Between
        );
        assert_eq!(
            FilterOperator::from(SelectToolOperator::FullText),
            FilterOperator::FullText
        );
    }

    #[tokio::test]
    async fn prepares_select_with_between_filter() {
        let request = SelectToolRequest {
            request_id: "request-between".into(),
            table: "users".into(),
            columns: vec!["id".into()],
            filters: vec![SelectToolFilter {
                column: "id".into(),
                operator: SelectToolOperator::Between,
                value: serde_json::json!(10),
                value_to: Some(serde_json::json!(20)),
            }],
            limit: Some(5),
        };

        let prepared = SelectTool::new(&policy(), None)
            .prepare(request)
            .await
            .expect("valid between request");
        assert_eq!(
            prepared.plan.sql,
            "SELECT \"id\" FROM \"users\" WHERE \"id\" BETWEEN $1 AND $2 LIMIT $3"
        );
        assert_eq!(
            prepared.plan.binds,
            vec![
                BindValue::Integer(10),
                BindValue::Integer(20),
                BindValue::Integer(5)
            ]
        );
    }

    #[tokio::test]
    async fn prepares_select_with_full_text_filter() {
        let request = SelectToolRequest {
            request_id: "request-full-text".into(),
            table: "users".into(),
            columns: vec!["id".into()],
            filters: vec![SelectToolFilter {
                column: "email".into(),
                operator: SelectToolOperator::FullText,
                value: serde_json::json!("alice"),
                value_to: None,
            }],
            limit: Some(5),
        };

        let prepared = SelectTool::new(&policy(), None)
            .prepare(request)
            .await
            .expect("valid full text request");
        assert!(prepared.plan.sql.contains(
            "to_tsvector('simple', coalesce(\"email\"::text, '')) @@ plainto_tsquery('simple', $1)"
        ));
        assert_eq!(
            prepared.plan.binds,
            vec![BindValue::Text("alice".into()), BindValue::Integer(5)]
        );
    }

    #[test]
    fn elapsed_ms_saturates_at_u64_max() {
        let duration = std::time::Duration::from_secs(u64::MAX);
        assert_eq!(elapsed_ms(duration), u64::MAX);
    }
}
