use std::str::FromStr;

use crate::backend::mysql::{MySqlBackend, MySqlBackendError};
use crate::backend::postgres::{PostgresBackend, PostgresBackendError};
use crate::config::{MySqlConfig, PostgresConfig};
use crate::query::QueryPlan;

macro_rules! read_only_backend_integration_test {
    (
        $name:ident,
        env = $env:literal,
        builder = $builder:expr,
        select_sql = $select_sql:literal,
        execute_error = $error:path
    ) => {
        #[tokio::test]
        async fn $name() {
            let Some(url) = std::env::var($env).ok() else {
                return;
            };

            let backend = ($builder)(&url)
                .await
                .expect("backend connection should succeed");

            let select_result = backend
                .execute_select(&QueryPlan {
                    sql: $select_sql.to_string(),
                    binds: vec![],
                })
                .await
                .expect("read-only backend should allow controlled selects");
            assert_eq!(select_result.columns, vec!["id", "name"]);
            assert_eq!(select_result.rows.len(), 1);

            let write_result = backend
                .execute_select(&QueryPlan {
                    sql: "CREATE TABLE datagate_read_only_probe(id INT)".to_string(),
                    binds: vec![],
                })
                .await;
            assert!(matches!(write_result, Err($error(_))));
        }
    };
}

async fn build_postgres_backend(url: &str) -> Result<PostgresBackend, PostgresBackendError> {
    let config = PostgresConfig {
        connect_options: sqlx::postgres::PgConnectOptions::from_str(url)
            .expect("valid DATAGATE_TEST_POSTGRES_URL"),
        max_connections: 1,
        acquire_timeout_secs: 3,
    };
    PostgresBackend::connect(&config).await
}

async fn build_mysql_backend(url: &str) -> Result<MySqlBackend, MySqlBackendError> {
    let config = MySqlConfig {
        connect_options: sqlx::mysql::MySqlConnectOptions::from_str(url)
            .expect("valid DATAGATE_TEST_MYSQL_URL"),
        max_connections: 1,
        acquire_timeout_secs: 3,
    };
    MySqlBackend::connect(&config).await
}

read_only_backend_integration_test!(
    postgres_read_only_backend_executes_select_and_rejects_write,
    env = "DATAGATE_TEST_POSTGRES_URL",
    builder = build_postgres_backend,
    select_sql = "SELECT 1::bigint AS id, 'alpha'::text AS name",
    execute_error = PostgresBackendError::Execute
);

read_only_backend_integration_test!(
    mysql_read_only_backend_executes_select_and_rejects_write,
    env = "DATAGATE_TEST_MYSQL_URL",
    builder = build_mysql_backend,
    select_sql = "SELECT CAST(1 AS SIGNED) AS id, CAST('alpha' AS CHAR) AS name",
    execute_error = MySqlBackendError::Execute
);
