//! Database backends. Backends own connections but never accept caller-provided SQL.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;
use sqlx::{Column, Row};
use thiserror::Error;

use crate::metrics::PoolMetrics;
use crate::query::QueryPlan;

macro_rules! bind_query_values {
    ($query:expr, $binds:expr) => {{
        let mut query = $query;
        for bind in $binds {
            query = match bind {
                crate::query::BindValue::Text(value) => query.bind(value),
                crate::query::BindValue::Integer(value) => query.bind(value),
                crate::query::BindValue::Decimal(value) => query.bind(value),
                crate::query::BindValue::Boolean(value) => query.bind(value),
            };
        }
        query
    }};
}

pub(crate) use bind_query_values;

pub(crate) fn columns_from_rows<R>(rows: &[R]) -> Vec<String>
where
    R: Row,
{
    rows.first()
        .map(|row| {
            row.columns()
                .iter()
                .map(|column| column.name().to_string())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn json_or_null<T>(value: Option<T>) -> Value
where
    T: Into<Value>,
{
    value.map_or(Value::Null, Into::into)
}

pub mod postgres;
pub mod sqlite;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("PostgreSQL backend failed")]
    Postgres(#[from] postgres::PostgresBackendError),
    #[error("SQLite backend failed")]
    Sqlite(#[from] sqlite::SqliteBackendError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectResult {
    pub columns: Vec<String>,
    pub rows: Vec<Value>,
}

pub trait ReadOnlyBackend {
    fn execute_select<'a>(
        &'a self,
        plan: &'a QueryPlan,
    ) -> Pin<Box<dyn Future<Output = Result<SelectResult, BackendError>> + Send + 'a>>;

    fn pool_metrics(&self) -> PoolMetrics;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyBackend;

    impl ReadOnlyBackend for DummyBackend {
        fn execute_select<'a>(
            &'a self,
            _plan: &'a QueryPlan,
        ) -> Pin<Box<dyn Future<Output = Result<SelectResult, BackendError>> + Send + 'a>> {
            Box::pin(async move {
                Ok(SelectResult {
                    columns: vec!["id".to_string()],
                    rows: vec![serde_json::json!({"id": 1})],
                })
            })
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics { size: 1, idle: 1 }
        }
    }

    #[tokio::test]
    async fn abstract_backend_trait_can_execute_select() {
        let backend = DummyBackend;
        let result = backend
            .execute_select(&QueryPlan {
                sql: "SELECT 1".to_string(),
                binds: vec![],
            })
            .await
            .expect("dummy backend should return data");
        assert_eq!(result.columns, vec!["id"]);
        assert_eq!(result.rows, vec![serde_json::json!({"id": 1})]);
        assert_eq!(backend.pool_metrics(), PoolMetrics { size: 1, idle: 1 });
    }
}
