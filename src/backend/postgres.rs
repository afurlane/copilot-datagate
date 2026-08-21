//! PostgreSQL connection pool with read-only sessions enforced at connection time.

use std::time::Duration;

use serde_json::{Map, Value};
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::{Column, Row, TypeInfo};
use thiserror::Error;

use crate::backend::{BackendError, ReadOnlyBackend, SelectResult};
use crate::config::PostgresConfig;
use crate::metrics::PoolMetrics;
use crate::query::{BindValue, QueryPlan};
use crate::schema::{SchemaCatalog, SchemaLoaderError};

#[derive(Debug, Error)]
pub enum PostgresBackendError {
    #[error("PostgreSQL connection configuration is invalid")]
    InvalidConfiguration,
    #[error("unable to establish a PostgreSQL read-only connection")]
    Connect(#[source] sqlx::Error),
    #[error("unable to load PostgreSQL schema metadata")]
    Schema(#[source] SchemaLoaderError),
    #[error("unable to execute the controlled PostgreSQL select")]
    Execute(#[source] sqlx::Error),
    #[error("unable to serialize a PostgreSQL result value")]
    ResultValue,
}

/// Owns a pool whose connections default to read-only transactions.
///
/// The pool is deliberately private: only controlled query-builder operations may
/// use it once that layer is implemented. Database credentials must use a read-only
/// PostgreSQL role as defense in depth.
pub struct PostgresBackend {
    #[allow(dead_code)]
    pool: PgPool,
}

impl PostgresBackend {
    pub async fn connect(config: &PostgresConfig) -> Result<Self, PostgresBackendError> {
        if config.max_connections == 0 || config.acquire_timeout_secs == 0 {
            return Err(PostgresBackendError::InvalidConfiguration);
        }

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
            .after_connect(|connection, _metadata| {
                Box::pin(async move {
                    // This statement is fixed by DataGate; no caller data can enter it.
                    sqlx::query("SET default_transaction_read_only = on")
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            })
            .connect_with(config.connect_options.clone())
            .await
            .map_err(PostgresBackendError::Connect)?;

        Ok(Self { pool })
    }

    pub(crate) async fn load_schema(&self) -> Result<SchemaCatalog, PostgresBackendError> {
        SchemaCatalog::load(&self.pool)
            .await
            .map_err(PostgresBackendError::Schema)
    }

    pub(crate) fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            size: self.pool.size(),
            idle: u32::try_from(self.pool.num_idle()).unwrap_or(u32::MAX),
        }
    }

    /// Executes only a plan created by the controlled query builder.
    pub(crate) async fn execute_select(
        &self,
        plan: &QueryPlan,
    ) -> Result<SelectResult, PostgresBackendError> {
        let mut query = sqlx::query(&plan.sql);
        for bind in &plan.binds {
            query = match bind {
                BindValue::Text(value) => query.bind(value),
                BindValue::Integer(value) => query.bind(value),
                BindValue::Decimal(value) => query.bind(value),
                BindValue::Boolean(value) => query.bind(value),
            };
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(PostgresBackendError::Execute)?;
        let columns = rows
            .first()
            .map(|row| {
                row.columns()
                    .iter()
                    .map(|column| column.name().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let rows = rows
            .into_iter()
            .map(row_to_json)
            .collect::<Result<_, _>>()?;
        Ok(SelectResult { columns, rows })
    }
}

fn row_to_json(row: PgRow) -> Result<Value, PostgresBackendError> {
    let mut object = Map::new();
    for column in row.columns() {
        let name = column.name().to_string();
        let value = match column.type_info().name() {
            "BOOL" => row
                .try_get::<Option<bool>, _>(column.ordinal())
                .map(json_or_null),
            "INT2" => row
                .try_get::<Option<i16>, _>(column.ordinal())
                .map(|value| json_or_null(value.map(i64::from))),
            "INT4" => row
                .try_get::<Option<i32>, _>(column.ordinal())
                .map(|value| json_or_null(value.map(i64::from))),
            "INT8" => row
                .try_get::<Option<i64>, _>(column.ordinal())
                .map(json_or_null),
            "FLOAT4" => row
                .try_get::<Option<f32>, _>(column.ordinal())
                .map(|value| json_or_null(value.map(f64::from))),
            "FLOAT8" => row
                .try_get::<Option<f64>, _>(column.ordinal())
                .map(json_or_null),
            "JSON" | "JSONB" => row
                .try_get::<Option<Value>, _>(column.ordinal())
                .map(json_or_null),
            _ => row
                .try_get::<Option<String>, _>(column.ordinal())
                .map(json_or_null),
        }
        .map_err(|_| PostgresBackendError::ResultValue)?;
        object.insert(name, value);
    }
    Ok(Value::Object(object))
}

impl ReadOnlyBackend for PostgresBackend {
    fn execute_select<'a>(
        &'a self,
        plan: &'a QueryPlan,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<SelectResult, BackendError>> + Send + 'a>,
    > {
        Box::pin(async move {
            PostgresBackend::execute_select(self, plan)
                .await
                .map_err(BackendError::from)
        })
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PostgresBackend::pool_metrics(self)
    }
}

fn json_or_null<T>(value: Option<T>) -> Value
where
    T: Into<Value>,
{
    value.map_or(Value::Null, Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_zero_acquire_timeout_without_attempting_to_connect() {
        let config = PostgresConfig {
            connect_options: sqlx::postgres::PgConnectOptions::new(),
            max_connections: 1,
            acquire_timeout_secs: 0,
        };
        let result = PostgresBackend::connect(&config).await;
        assert!(matches!(
            result,
            Err(PostgresBackendError::InvalidConfiguration)
        ));
    }

    #[tokio::test]
    async fn rejects_zero_pool_size_without_attempting_to_connect() {
        let options = sqlx::postgres::PgConnectOptions::new()
            .host("localhost")
            .database("application");
        let config = PostgresConfig {
            connect_options: options,
            max_connections: 0,
            acquire_timeout_secs: 1,
        };
        let result = PostgresBackend::connect(&config).await;
        assert!(matches!(
            result,
            Err(PostgresBackendError::InvalidConfiguration)
        ));
    }
}
