# DataGate Benchmarks

DataGate includes a local benchmark runner for the controlled query path. The
runner builds plans through the policy-aware query builder and executes them
through the read-only backend trait.

## Runner

```bash
cargo run --release --bin datagate-benchmark
```

The runner always creates a temporary SQLite fixture. PostgreSQL benchmarks run
when the standard PostgreSQL environment is present:

```bash
export DB_URL='postgresql://reader:password@127.0.0.1:5432/application'
export DB_OPTIONS='application_name=datagate&search_path=public'
cargo run --release --bin datagate-benchmark
```

Supported options:

- `DATAGATE_BENCH_ITERATIONS` (default `50`)
- `DATAGATE_BENCH_WARMUPS` (default `5`)
- `DATAGATE_BENCH_SQLITE_ROWS` (default `10000`)
- `DATAGATE_BENCH_INCLUDE_SEARCH=1` to include the optional text-search case

The PostgreSQL benchmark currently targets the Axon event table
`public.domainevententry`. It uses this allow-list:

- `aggregateidentifier`
- `sequencenumber`
- `type`
- `eventidentifier`
- `metadata`
- `payload`
- `payloadrevision`
- `payloadtype`
- `timestamp`
- `username`

## Current Local Result

Environment:

- date: 2026-08-21
- iterations: 20
- warmups: 3
- SQLite fixture rows: 10000
- PostgreSQL table: `public.domainevententry`
- PostgreSQL rows: 1796440

```text
sqlite.select_limit_100              n=20   min=   0.306ms mean=   0.381ms p50=   0.334ms p95=   0.735ms max=   0.735ms
sqlite.aggregate_count               n=20   min=   0.084ms mean=   0.092ms p50=   0.088ms p95=   0.111ms max=   0.111ms
postgres.select_limit_100            n=20   min=   0.246ms mean=   0.292ms p50=   0.292ms p95=   0.451ms max=   0.451ms
postgres.aggregate_count             n=20   min= 106.182ms mean= 113.689ms p50= 113.057ms p95= 124.974ms max= 124.974ms
```

## Notes

- The benchmark does not accept free SQL from users.
- The benchmark does not store database credentials in the repository.
- The PostgreSQL target is intentionally a real production-shaped table, useful
  for latency tracking beyond synthetic SQLite checks.
