# DataGate Configuration and Operations

This guide describes the supported local setup and the runtime configuration
used by DataGate. The service is deny-by-default: a table and its columns must
be explicitly listed in the policy before a tool can access them.

## Quick Start

1. Copy `datagate.example.toml` to `datagate.toml`.
2. Adjust the selected profile and its table/column allow-list.
3. Set database credentials through environment variables.
4. Select a backend with `DATAGATE_BACKEND`, or leave it unset for `auto`.
5. Run the service:

```bash
cargo run
```

The current binary initializes configuration, policy, observability, and the
selected read-only backend. MCP stdio transport is available when explicitly
enabled via environment variables.

## Backend Selection

`DATAGATE_BACKEND` accepts `auto`, `postgres`, `mysql`/`mariadb`, or `sqlite`.
The default `auto` order is PostgreSQL, MySQL/MariaDB, then SQLite. Explicit
selection fails when the matching environment configuration is missing.

### PostgreSQL

Use either `DB_URL` or component variables:

```bash
export DATAGATE_BACKEND=postgres
export DB_URL='postgres://reader@localhost:5432/app'
```

Component variables are `DB_HOST`, `DB_PORT` (default `5432`), `DB_USER`,
`DB_PASSWORD`, and `DB_NAME`. `DB_OPTIONS` accepts URL-style options such as
`statement_timeout=5s&application_name=datagate`. Pool settings are
`DB_MAX_CONNECTIONS` and `DB_ACQUIRE_TIMEOUT_SECS`.

### MySQL/MariaDB

Use either `MYSQL_URL` or `MYSQL_HOST`, `MYSQL_PORT` (default `3306`),
`MYSQL_USER`, `MYSQL_PASSWORD`, and `MYSQL_DATABASE`. Pool settings are
`MYSQL_MAX_CONNECTIONS` and `MYSQL_ACQUIRE_TIMEOUT_SECS`.

### SQLite

Use `SQLITE_URL` or `SQLITE_PATH`. SQLite connections are opened read-only and
are never created automatically. Pool settings are `SQLITE_MAX_CONNECTIONS`
and `SQLITE_ACQUIRE_TIMEOUT_SECS`.

## Policy File

`DATAGATE_CONFIG` selects the TOML path; the default is `datagate.toml`.
Credentials must not be placed in this file.

A named profile is selected with `profile`:

```toml
profile = "production"

[profiles.production.policy]
default_row_limit = 50
max_row_limit = 500
max_query_complexity = 100
max_output_bytes = 131072

[profiles.production.policy.tables.customers]
columns = ["id", "email"]
```

If no named profiles are configured, the legacy `[policy]` section remains
supported. An unknown selected profile produces a deny-all policy during
application startup. Table keys may be unqualified (`customers`) or
schema-qualified (`audit.customers`).

The policy limits are server-side:

- `default_row_limit`: applied when a request omits `limit`.
- `max_row_limit`: upper bound for requested rows.
- `max_query_complexity`: query budget; columns cost 1 and filters cost 2.
- `max_output_bytes`: maximum serialized JSON response size.

Filter operators can be restricted per column with `filter_operators`. When a
column has no operator list, all supported operators remain allowed.

## Request Hardening and Observability

MCP input is bounded before policy evaluation and query generation. The current
limits are documented in [mcp-tools.md](mcp-tools.md): request IDs are
validated, text values are capped at 4096 bytes, and request list cardinality
is bounded.

Optional rate limiting uses `RATE_LIMIT_REQUESTS` and
`RATE_LIMIT_WINDOW_SECS`, keyed by `request_id`. Logging supports
`LOG_FORMAT=pretty` (default) or `LOG_FORMAT=json`, with `LOG_LEVEL` or
`RUST_LOG` controlling verbosity. In MCP stdio mode, logs are written to stderr
and default to `warn`, because stdout is reserved for JSON-RPC protocol frames.
`AUDIT_LOG_PATH` enables JSONL audit events.

## VS Code MCP Startup Templates

For VS Code, use `stdio` as the default transport model for local runs.

### Template A: Local stdio (supported today)

```json
{
  "servers": {
    "copilot-datagate": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "--quiet"],
      "cwd": "${workspaceFolder}",
      "env": {
        "DATAGATE_MCP_STDIO": "1",
        "DATAGATE_CONFIG": "${workspaceFolder}/datagate.toml",
        "DATAGATE_BACKEND": "postgres",
        "DB_URL": "postgresql://reader:password@127.0.0.1:5432/application",
        "DB_OPTIONS": "application_name=datagate&search_path=public",
        "DB_MAX_CONNECTIONS": "10",
        "DB_ACQUIRE_TIMEOUT_SECS": "10"
      }
    }
  }
}
```

### Remote HTTP

Remote HTTP transport is not part of the default 1.0 configuration. It requires
a separate security decision because it changes DataGate from a local stdio
process into a network-facing service. Registry publication targets local stdio
only; see [registry-publication.md](registry-publication.md).

## Verification

Run local benchmarks with:

```bash
cargo run --release --bin datagate-benchmark
```

Details and the latest local PostgreSQL run are documented in
[benchmarks.md](benchmarks.md).

Run the local quality checks before submitting changes:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo audit
cargo deny check
```

Backend integration tests are environment-gated. Set
`DATAGATE_TEST_POSTGRES_URL` or `DATAGATE_TEST_MYSQL_URL` to run the matching
checks; otherwise those tests are skipped.

## Security Boundaries

- Never put credentials in TOML, source code, logs, or pull requests.
- Use a database role with read-only permissions.
- Every query passes through policy validation and the controlled query builder.
- SQL text, filter values, stack traces, and unauthorized object names are not
  part of public MCP responses.
- Report suspected vulnerabilities through the private process in
  [SECURITY.md](../SECURITY.md).
