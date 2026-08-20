//! PostgreSQL connection pool with read-only sessions enforced at connection time.

use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;

use crate::config::PostgresConfig;
use crate::schema::{SchemaCatalog, SchemaLoaderError};

#[derive(Debug, Error)]
pub enum PostgresBackendError {
    #[error("PostgreSQL connection configuration is invalid")]
    InvalidConfiguration,
    #[error("unable to establish a PostgreSQL read-only connection")]
    Connect(#[source] sqlx::Error),
    #[error("unable to load PostgreSQL schema metadata")]
    Schema(#[source] SchemaLoaderError),
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
