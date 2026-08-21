//! SQLite backend with read-only connections and controlled query execution.

use std::time::Duration;

use serde_json::{Map, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Row, TypeInfo};
use thiserror::Error;

use crate::backend::{BackendError, ReadOnlyBackend, SelectResult};
use crate::config::SqliteConfig;
use crate::metrics::PoolMetrics;
use crate::query::{BindValue, QueryPlan};

#[derive(Debug, Error)]
pub enum SqliteBackendError {
    #[error("SQLite connection configuration is invalid")]
    InvalidConfiguration,
    #[error("unable to establish a SQLite read-only connection")]
    Connect(#[source] sqlx::Error),
    #[error("unable to execute the controlled SQLite select")]
    Execute(#[source] sqlx::Error),
    #[error("unable to serialize a SQLite result value")]
    ResultValue,
}

pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    pub async fn connect(config: &SqliteConfig) -> Result<Self, SqliteBackendError> {
        if config.max_connections == 0 || config.acquire_timeout_secs == 0 {
            return Err(SqliteBackendError::InvalidConfiguration);
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
            .after_connect(|connection, _metadata| {
                Box::pin(async move {
                    sqlx::query("PRAGMA query_only = ON")
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            })
            .connect_with(config.connect_options.clone())
            .await
            .map_err(SqliteBackendError::Connect)?;

        Ok(Self { pool })
    }

    pub(crate) fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            size: self.pool.size(),
            idle: u32::try_from(self.pool.num_idle()).unwrap_or(u32::MAX),
        }
    }

    pub(crate) async fn execute_select(
        &self,
        plan: &QueryPlan,
    ) -> Result<SelectResult, SqliteBackendError> {
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
            .map_err(SqliteBackendError::Execute)?;
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

fn row_to_json(row: SqliteRow) -> Result<Value, SqliteBackendError> {
    let mut object = Map::new();
    for column in row.columns() {
        let name = column.name().to_string();
        let value = match column.type_info().name() {
            "INTEGER" | "INT" => row
                .try_get::<Option<i64>, _>(column.ordinal())
                .map(json_or_null),
            "REAL" | "FLOAT" | "DOUBLE" => row
                .try_get::<Option<f64>, _>(column.ordinal())
                .map(json_or_null),
            "TEXT" => row
                .try_get::<Option<String>, _>(column.ordinal())
                .map(json_or_null),
            "BLOB" => row
                .try_get::<Option<Vec<u8>>, _>(column.ordinal())
                .map(|value| json_or_null(value.map(encode_hex))),
            _ => row
                .try_get::<Option<String>, _>(column.ordinal())
                .map(json_or_null),
        }
        .map_err(|_| SqliteBackendError::ResultValue)?;
        object.insert(name, value);
    }
    Ok(Value::Object(object))
}

impl ReadOnlyBackend for SqliteBackend {
    fn execute_select<'a>(
        &'a self,
        plan: &'a QueryPlan,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<SelectResult, BackendError>> + Send + 'a>,
    > {
        Box::pin(async move {
            SqliteBackend::execute_select(self, plan)
                .await
                .map_err(BackendError::from)
        })
    }

    fn pool_metrics(&self) -> PoolMetrics {
        SqliteBackend::pool_metrics(self)
    }
}

fn json_or_null<T>(value: Option<T>) -> Value
where
    T: Into<Value>,
{
    value.map_or(Value::Null, Into::into)
}

fn encode_hex(bytes: Vec<u8>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_zero_acquire_timeout_without_attempting_to_connect() {
        let config = SqliteConfig {
            connect_options: sqlx::sqlite::SqliteConnectOptions::new().in_memory(true),
            max_connections: 1,
            acquire_timeout_secs: 0,
        };
        let result = SqliteBackend::connect(&config).await;
        assert!(matches!(
            result,
            Err(SqliteBackendError::InvalidConfiguration)
        ));
    }

    #[tokio::test]
    async fn rejects_zero_pool_size_without_attempting_to_connect() {
        let config = SqliteConfig {
            connect_options: sqlx::sqlite::SqliteConnectOptions::new().in_memory(true),
            max_connections: 0,
            acquire_timeout_secs: 1,
        };
        let result = SqliteBackend::connect(&config).await;
        assert!(matches!(
            result,
            Err(SqliteBackendError::InvalidConfiguration)
        ));
    }
}
