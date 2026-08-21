use std::str::FromStr;

use crate::backend::mysql::{MySqlBackend, MySqlBackendError};
use crate::backend::postgres::{PostgresBackend, PostgresBackendError};
use crate::config::{MySqlConfig, PostgresConfig};
use crate::query::QueryPlan;

#[tokio::test]
async fn postgres_read_only_backend_executes_select_and_rejects_write() {
    let Some(url) = std::env::var("DATAGATE_TEST_POSTGRES_URL").ok() else {
        return;
    };

    let config = PostgresConfig {
        connect_options: sqlx::postgres::PgConnectOptions::from_str(&url)
            .expect("valid DATAGATE_TEST_POSTGRES_URL"),
        max_connections: 1,
        acquire_timeout_secs: 3,
    };

    let backend = PostgresBackend::connect(&config)
        .await
        .expect("postgres backend connection should succeed");

    let select_result = backend
        .execute_select(&QueryPlan {
            sql: "SELECT 1::bigint AS id, 'alpha'::text AS name".to_string(),
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
    assert!(matches!(
        write_result,
        Err(PostgresBackendError::Execute(_))
    ));
}

#[tokio::test]
async fn mysql_read_only_backend_executes_select_and_rejects_write() {
    let Some(url) = std::env::var("DATAGATE_TEST_MYSQL_URL").ok() else {
        return;
    };

    let config = MySqlConfig {
        connect_options: sqlx::mysql::MySqlConnectOptions::from_str(&url)
            .expect("valid DATAGATE_TEST_MYSQL_URL"),
        max_connections: 1,
        acquire_timeout_secs: 3,
    };

    let backend = MySqlBackend::connect(&config)
        .await
        .expect("mysql backend connection should succeed");

    let select_result = backend
        .execute_select(&QueryPlan {
            sql: "SELECT CAST(1 AS SIGNED) AS id, CAST('alpha' AS CHAR) AS name".to_string(),
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
    assert!(matches!(write_result, Err(MySqlBackendError::Execute(_))));
}
