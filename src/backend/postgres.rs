//! PostgreSQL connection pool with read-only sessions enforced at connection time.

use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;

use crate::config::PostgresConfig;

#[derive(Debug, Error)]
pub enum PostgresBackendError {
    #[error("PostgreSQL connection configuration is invalid")]
    InvalidConfiguration,
    #[error("unable to establish a PostgreSQL read-only connection")]
    Connect(#[source] sqlx::Error),
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
        if !config.validate() {
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
            .connect(&config.url)
            .await
            .map_err(PostgresBackendError::Connect)?;

        Ok(Self { pool })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_empty_connection_url_without_attempting_to_connect() {
        let config = PostgresConfig {
            url: String::new(),
            max_connections: 1,
            acquire_timeout_secs: 1,
        };
        let result = PostgresBackend::connect(&config).await;
        assert!(matches!(
            result,
            Err(PostgresBackendError::InvalidConfiguration)
        ));
    }

    #[tokio::test]
    async fn rejects_zero_pool_size_without_attempting_to_connect() {
        let config = PostgresConfig {
            url: "postgres://readonly:secret@localhost/application".to_string(),
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
