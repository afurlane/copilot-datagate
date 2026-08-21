#![allow(dead_code)]

#[path = "../backend/mod.rs"]
mod backend;
#[path = "../config.rs"]
mod config;
#[path = "../metrics.rs"]
mod metrics;
#[path = "../policy.rs"]
mod policy;
#[path = "../query.rs"]
mod query;
#[path = "../schema.rs"]
mod schema;

use std::collections::HashMap;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use backend::postgres::PostgresBackend;
use backend::sqlite::SqliteBackend;
use backend::ReadOnlyBackend;
use config::{PolicyConfig, SqliteConfig, TableConfig};
use policy::Policy;
use query::{
    AggregateFunction, AggregateOperation, AggregateRequest, QueryPlan, SearchRequest,
    SelectRequest,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

const BENCH_TABLE: &str = "public.domainevententry";
const SQLITE_TABLE: &str = "domainevententry";
const DEFAULT_ITERATIONS: usize = 50;
const DEFAULT_WARMUPS: usize = 5;
const DEFAULT_SQLITE_ROWS: usize = 10_000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let iterations = env_usize("DATAGATE_BENCH_ITERATIONS", DEFAULT_ITERATIONS);
    let warmups = env_usize("DATAGATE_BENCH_WARMUPS", DEFAULT_WARMUPS);
    let include_search = std::env::var("DATAGATE_BENCH_INCLUDE_SEARCH")
        .ok()
        .as_deref()
        == Some("1");

    println!("DataGate benchmark");
    println!("iterations={iterations} warmups={warmups}");
    println!();

    run_sqlite_benchmarks(iterations, warmups, include_search).await?;

    if config::PostgresConfig::from_env()?.is_some() {
        run_postgres_benchmarks(iterations, warmups, include_search).await?;
    } else {
        println!("postgres: skipped (set DB_URL or DB_* environment variables)");
    }

    Ok(())
}

async fn run_sqlite_benchmarks(
    iterations: usize,
    warmups: usize,
    include_search: bool,
) -> anyhow::Result<()> {
    let rows = env_usize("DATAGATE_BENCH_SQLITE_ROWS", DEFAULT_SQLITE_ROWS);
    let path = sqlite_fixture_path();
    create_sqlite_fixture(&path, rows).await?;

    let config = SqliteConfig {
        connect_options: SqliteConnectOptions::new()
            .filename(&path)
            .read_only(true)
            .create_if_missing(false),
        max_connections: 5,
        acquire_timeout_secs: 5,
    };
    let backend = SqliteBackend::connect(&config).await?;
    let policy = axon_policy(SQLITE_TABLE);
    let plans = benchmark_plans(SQLITE_TABLE, &policy, include_search)?;

    println!("sqlite fixture rows={rows}");
    for (name, plan) in plans {
        print_stats(
            run_plan_benchmark(
                &backend,
                &format!("sqlite.{name}"),
                &plan,
                iterations,
                warmups,
            )
            .await?,
        );
    }
    println!();

    std::fs::remove_file(path)?;
    Ok(())
}

async fn run_postgres_benchmarks(
    iterations: usize,
    warmups: usize,
    include_search: bool,
) -> anyhow::Result<()> {
    let config = config::PostgresConfig::from_env()?.expect("checked above");
    let backend = PostgresBackend::connect(&config).await?;
    let policy = axon_policy(BENCH_TABLE);
    let plans = benchmark_plans(BENCH_TABLE, &policy, include_search)?;

    println!("postgres table={BENCH_TABLE}");
    for (name, plan) in plans {
        print_stats(
            run_plan_benchmark(
                &backend,
                &format!("postgres.{name}"),
                &plan,
                iterations,
                warmups,
            )
            .await?,
        );
    }
    println!();

    Ok(())
}

fn benchmark_plans(
    table: &str,
    policy: &Policy,
    include_search: bool,
) -> anyhow::Result<Vec<(String, QueryPlan)>> {
    let mut plans = vec![
        (
            "select_limit_100".to_string(),
            SelectRequest {
                table: table.to_string(),
                columns: vec![
                    "aggregateidentifier".into(),
                    "sequencenumber".into(),
                    "type".into(),
                    "payloadtype".into(),
                    "timestamp".into(),
                ],
                filters: vec![],
                limit: Some(100),
            }
            .build(policy)?,
        ),
        (
            "aggregate_count".to_string(),
            AggregateRequest {
                table: table.to_string(),
                operations: vec![AggregateOperation {
                    function: AggregateFunction::Count,
                    column: "*".into(),
                    alias: Some("total".into()),
                }],
                filters: vec![],
            }
            .build(policy)?,
        ),
    ];

    if include_search {
        plans.push((
            "search_event_limit_100".to_string(),
            SearchRequest {
                table: table.to_string(),
                columns: vec![
                    "aggregateidentifier".into(),
                    "sequencenumber".into(),
                    "type".into(),
                    "payloadtype".into(),
                ],
                searchable_columns: vec!["type".into(), "payloadtype".into()],
                text: "Event".into(),
                limit: Some(100),
            }
            .build(policy)?,
        ));
    }

    Ok(plans)
}

async fn run_plan_benchmark(
    backend: &impl ReadOnlyBackend,
    name: &str,
    plan: &QueryPlan,
    iterations: usize,
    warmups: usize,
) -> anyhow::Result<BenchmarkStats> {
    for _ in 0..warmups {
        let result = backend.execute_select(plan).await?;
        black_box(result.rows.len());
    }

    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let result = backend.execute_select(plan).await?;
        black_box(result.rows.len());
        durations.push(started.elapsed());
    }

    Ok(BenchmarkStats::new(name.to_string(), durations))
}

fn axon_policy(table: &str) -> Policy {
    let mut tables = HashMap::new();
    tables.insert(
        table.to_string(),
        TableConfig {
            columns: vec![
                "aggregateidentifier".into(),
                "sequencenumber".into(),
                "type".into(),
                "eventidentifier".into(),
                "metadata".into(),
                "payload".into(),
                "payloadrevision".into(),
                "payloadtype".into(),
                "timestamp".into(),
                "username".into(),
            ],
            filter_operators: HashMap::new(),
        },
    );

    Policy::new(PolicyConfig {
        tables,
        default_row_limit: 100,
        max_row_limit: 1_000,
        max_query_complexity: 100,
        max_output_bytes: 8 * 1024 * 1024,
    })
}

async fn create_sqlite_fixture(path: &PathBuf, rows: usize) -> anyhow::Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await?;

    sqlx::query(
        "CREATE TABLE domainevententry (
            aggregateidentifier TEXT NOT NULL,
            sequencenumber INTEGER NOT NULL,
            type TEXT NOT NULL,
            eventidentifier TEXT NOT NULL,
            metadata BLOB,
            payload BLOB,
            payloadrevision TEXT,
            payloadtype TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            username TEXT
        )",
    )
    .execute(&pool)
    .await?;

    sqlx::query("BEGIN").execute(&pool).await?;
    for index in 0..rows {
        sqlx::query(
            "INSERT INTO domainevententry (
                aggregateidentifier,
                sequencenumber,
                type,
                eventidentifier,
                metadata,
                payload,
                payloadrevision,
                payloadtype,
                timestamp,
                username
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("aggregate-{}", index % 1_000))
        .bind(i64::try_from(index).unwrap_or(i64::MAX))
        .bind("DomainEvent")
        .bind(format!("event-{index}"))
        .bind(Vec::<u8>::new())
        .bind(vec![1_u8, 2, 3, 4])
        .bind("1")
        .bind("com.example.DomainEvent")
        .bind(format!("2026-08-21T20:{:02}:00Z", index % 60))
        .bind("benchmark")
        .execute(&pool)
        .await?;
    }
    sqlx::query("COMMIT").execute(&pool).await?;
    pool.close().await;

    Ok(())
}

fn sqlite_fixture_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "datagate-benchmark-{}-{}.db",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ))
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[derive(Debug)]
struct BenchmarkStats {
    name: String,
    iterations: usize,
    min: Duration,
    mean: Duration,
    p50: Duration,
    p95: Duration,
    max: Duration,
}

impl BenchmarkStats {
    fn new(name: String, mut durations: Vec<Duration>) -> Self {
        durations.sort_unstable();
        let iterations = durations.len();
        let total_nanos = durations.iter().map(Duration::as_nanos).sum::<u128>();
        let mean = Duration::from_nanos((total_nanos / iterations as u128) as u64);
        let min = durations[0];
        let p50 = percentile(&durations, 50);
        let p95 = percentile(&durations, 95);
        let max = durations[iterations - 1];

        Self {
            name,
            iterations,
            min,
            mean,
            p50,
            p95,
            max,
        }
    }
}

fn percentile(durations: &[Duration], percentile: usize) -> Duration {
    let index = ((durations.len() - 1) * percentile).div_ceil(100);
    durations[index]
}

fn print_stats(stats: BenchmarkStats) {
    println!(
        "{:<36} n={:<4} min={:>8.3}ms mean={:>8.3}ms p50={:>8.3}ms p95={:>8.3}ms max={:>8.3}ms",
        stats.name,
        stats.iterations,
        millis(stats.min),
        millis(stats.mean),
        millis(stats.p50),
        millis(stats.p95),
        millis(stats.max),
    );
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
