//! SQLite backend with read-only connections and controlled query execution.

use std::time::Duration;

use serde_json::{Map, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Row, TypeInfo};
use thiserror::Error;

use crate::backend::{
    columns_from_rows, json_or_null, BackendError, ReadOnlyBackend, SelectResult,
};
use crate::config::SqliteConfig;
use crate::metrics::PoolMetrics;
use crate::query::QueryPlan;

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
        let sql = plan.sql.replace(" ILIKE ", " LIKE ");
        let query =
            crate::backend::bind_query_values!(sqlx::query(sqlx::AssertSqlSafe(sql)), &plan.binds);

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(SqliteBackendError::Execute)?;
        let columns = columns_from_rows(&rows);
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
            "INTEGER" | "INT" | "INT64" | "NULL" => row
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
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::query::BindValue;
    use sqlx::ConnectOptions;

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

    #[tokio::test]
    async fn executes_parameterized_select_and_serializes_blob_as_hex() {
        let db_path = temp_db_path();

        let setup_options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .read_only(false)
            .disable_statement_logging();
        let setup_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(setup_options)
            .await
            .expect("sqlite setup pool");

        sqlx::query(
            "CREATE TABLE metrics (id INTEGER PRIMARY KEY, name TEXT, score REAL, payload BLOB)",
        )
        .execute(&setup_pool)
        .await
        .expect("create table");
        sqlx::query("INSERT INTO metrics (id, name, score, payload) VALUES (?, ?, ?, ?)")
            .bind(1_i64)
            .bind("alpha")
            .bind(12.5_f64)
            .bind(vec![0x0a_u8, 0x0b_u8])
            .execute(&setup_pool)
            .await
            .expect("insert row");
        drop(setup_pool);

        let config = SqliteConfig {
            connect_options: sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_path)
                .read_only(true)
                .create_if_missing(false)
                .disable_statement_logging(),
            max_connections: 1,
            acquire_timeout_secs: 2,
        };
        let backend = SqliteBackend::connect(&config)
            .await
            .expect("read-only backend");

        let result = backend
            .execute_select(&QueryPlan {
                sql: "SELECT id, name, score, payload FROM metrics WHERE id = ?".to_string(),
                binds: vec![BindValue::Integer(1)],
            })
            .await
            .expect("select from sqlite");

        assert_eq!(result.columns, vec!["id", "name", "score", "payload"]);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0]["id"], serde_json::json!(1));
        assert_eq!(result.rows[0]["name"], serde_json::json!("alpha"));
        assert_eq!(result.rows[0]["score"], serde_json::json!(12.5));
        assert_eq!(result.rows[0]["payload"], serde_json::json!("0a0b"));

        drop(backend);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn read_only_connection_rejects_write_statements() {
        let db_path = temp_db_path();

        let setup_options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .read_only(false)
            .disable_statement_logging();
        let setup_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(setup_options)
            .await
            .expect("sqlite setup pool");
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)")
            .execute(&setup_pool)
            .await
            .expect("create table");
        drop(setup_pool);

        let config = SqliteConfig {
            connect_options: sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_path)
                .read_only(true)
                .create_if_missing(false)
                .disable_statement_logging(),
            max_connections: 1,
            acquire_timeout_secs: 2,
        };
        let backend = SqliteBackend::connect(&config)
            .await
            .expect("read-only backend");

        let result = backend
            .execute_select(&QueryPlan {
                sql: "INSERT INTO t (name) VALUES ('blocked')".to_string(),
                binds: vec![],
            })
            .await;
        assert!(matches!(result, Err(SqliteBackendError::Execute(_))));

        drop(backend);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn encode_hex_uses_lowercase_pairs() {
        assert_eq!(encode_hex(vec![0x00, 0x1f, 0xa0, 0xff]), "001fa0ff");
    }

    fn temp_db_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "copilot-datagate-sqlite-{}-{}.db",
            std::process::id(),
            nanos
        ))
    }
}
