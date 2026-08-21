//! MySQL/MariaDB connection pool with read-only sessions enforced at connection time.

use std::time::Duration;

use serde_json::{Map, Value};
use sqlx::mysql::{MySqlPool, MySqlPoolOptions, MySqlRow};
use sqlx::{Column, Row, TypeInfo};
use thiserror::Error;

use crate::backend::{
    columns_from_rows, json_or_null, BackendError, ReadOnlyBackend, SelectResult,
};
use crate::config::MySqlConfig;
use crate::metrics::PoolMetrics;
use crate::query::QueryPlan;

#[derive(Debug, Error)]
pub enum MySqlBackendError {
    #[error("MySQL connection configuration is invalid")]
    InvalidConfiguration,
    #[error("unable to establish a MySQL read-only connection")]
    Connect(#[source] sqlx::Error),
    #[error("unable to execute the controlled MySQL select")]
    Execute(#[source] sqlx::Error),
    #[error("unable to serialize a MySQL result value")]
    ResultValue,
}

pub struct MySqlBackend {
    pool: MySqlPool,
}

impl MySqlBackend {
    pub async fn connect(config: &MySqlConfig) -> Result<Self, MySqlBackendError> {
        if config.max_connections == 0 || config.acquire_timeout_secs == 0 {
            return Err(MySqlBackendError::InvalidConfiguration);
        }

        let pool = MySqlPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
            .after_connect(|connection, _metadata| {
                Box::pin(async move {
                    // Fixed statement for defense in depth; app-level code still uses controlled SELECTs only.
                    sqlx::query("SET SESSION TRANSACTION READ ONLY")
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            })
            .connect_with(config.connect_options.clone())
            .await
            .map_err(MySqlBackendError::Connect)?;

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
    ) -> Result<SelectResult, MySqlBackendError> {
        let query = crate::backend::bind_query_values!(sqlx::query(&plan.sql), &plan.binds);

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(MySqlBackendError::Execute)?;
        let columns = columns_from_rows(&rows);
        let rows = rows
            .into_iter()
            .map(row_to_json)
            .collect::<Result<_, _>>()?;
        Ok(SelectResult { columns, rows })
    }
}

fn row_to_json(row: MySqlRow) -> Result<Value, MySqlBackendError> {
    let mut object = Map::new();
    for column in row.columns() {
        let name = column.name().to_string();
        let value = match column.type_info().name() {
            "BOOL" => row
                .try_get::<Option<bool>, _>(column.ordinal())
                .map(json_or_null),
            "TINYINT" => row
                .try_get::<Option<i8>, _>(column.ordinal())
                .map(|value| json_or_null(value.map(i64::from))),
            "SMALLINT" => row
                .try_get::<Option<i16>, _>(column.ordinal())
                .map(|value| json_or_null(value.map(i64::from))),
            "INT" | "MEDIUMINT" => row
                .try_get::<Option<i32>, _>(column.ordinal())
                .map(|value| json_or_null(value.map(i64::from))),
            "BIGINT" => row
                .try_get::<Option<i64>, _>(column.ordinal())
                .map(json_or_null),
            "FLOAT" => row
                .try_get::<Option<f32>, _>(column.ordinal())
                .map(|value| json_or_null(value.map(f64::from))),
            "DOUBLE" => row
                .try_get::<Option<f64>, _>(column.ordinal())
                .map(json_or_null),
            "JSON" => row
                .try_get::<Option<String>, _>(column.ordinal())
                .map(parse_mysql_json_text),
            _ => row
                .try_get::<Option<String>, _>(column.ordinal())
                .map(json_or_null),
        }
        .map_err(|_| MySqlBackendError::ResultValue)?;
        object.insert(name, value);
    }
    Ok(Value::Object(object))
}

fn parse_mysql_json_text(value: Option<String>) -> Value {
    json_or_null(value.and_then(|text| serde_json::from_str::<Value>(&text).ok()))
}

impl ReadOnlyBackend for MySqlBackend {
    fn execute_select<'a>(
        &'a self,
        plan: &'a QueryPlan,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<SelectResult, BackendError>> + Send + 'a>,
    > {
        Box::pin(async move {
            MySqlBackend::execute_select(self, plan)
                .await
                .map_err(BackendError::from)
        })
    }

    fn pool_metrics(&self) -> PoolMetrics {
        MySqlBackend::pool_metrics(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_zero_acquire_timeout_without_attempting_to_connect() {
        let config = MySqlConfig {
            connect_options: sqlx::mysql::MySqlConnectOptions::new(),
            max_connections: 1,
            acquire_timeout_secs: 0,
        };
        let result = MySqlBackend::connect(&config).await;
        assert!(matches!(
            result,
            Err(MySqlBackendError::InvalidConfiguration)
        ));
    }

    #[tokio::test]
    async fn rejects_zero_pool_size_without_attempting_to_connect() {
        let config = MySqlConfig {
            connect_options: sqlx::mysql::MySqlConnectOptions::new()
                .host("localhost")
                .database("application"),
            max_connections: 0,
            acquire_timeout_secs: 1,
        };
        let result = MySqlBackend::connect(&config).await;
        assert!(matches!(
            result,
            Err(MySqlBackendError::InvalidConfiguration)
        ));
    }

    #[tokio::test]
    async fn returns_connect_error_for_unreachable_server() {
        let config = MySqlConfig {
            connect_options: sqlx::mysql::MySqlConnectOptions::new()
                .host("127.0.0.1")
                .port(9)
                .username("reader")
                .database("application"),
            max_connections: 1,
            acquire_timeout_secs: 1,
        };
        let result = MySqlBackend::connect(&config).await;
        assert!(matches!(result, Err(MySqlBackendError::Connect(_))));
    }

    #[test]
    fn parses_mysql_json_text_value() {
        let value = parse_mysql_json_text(Some(r#"{"ok":true,"n":7}"#.to_string()));
        assert_eq!(value, serde_json::json!({"ok": true, "n": 7}));
    }

    #[test]
    fn returns_null_when_mysql_json_text_is_invalid_or_missing() {
        assert_eq!(
            parse_mysql_json_text(Some("not-json".to_string())),
            Value::Null
        );
        assert_eq!(parse_mysql_json_text(None), Value::Null);
    }
}
