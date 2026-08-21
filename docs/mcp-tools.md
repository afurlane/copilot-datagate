# MCP Tools Internal Documentation

This document is the internal reference for DataGate MCP tool contracts and
security behavior.

## Scope

Implemented tools:

- `select`
- `search`
- `aggregate`

The stable API descriptor is exposed by `McpApiDescriptor::current()` and
currently reports API version `1` with the tools `select`, `search`, and
`aggregate`, in that order. Clients should use the descriptor rather than
assuming that new tools are added without a version change.

The first `aggregate` version returns one aggregate row and does not support
`GROUP BY`; grouped aggregation is reserved for the advanced query layer.

## Shared Security Invariants

All MCP tools must satisfy these invariants:

- No free SQL accepted from callers.
- Policy validation is mandatory before SQL plan execution.
- Only parameterized SQL is executed.
- Read-only backend path only.
- Output-size limits enforced server-side.
- Public errors are sanitized (`invalid_request`, `policy_denied`,
  `rate_limited`, `backend_unavailable`, `internal_error`).
- Audit events are recorded for accepted and rejected requests.
- Optional rate limiting is evaluated before query execution.
- Optional in-process metrics are recorded per request outcome.

Hardening input envelope (server-side):

- `request_id` format validation (`[A-Za-z0-9._:-]`, max length 128)
- max text payload length (4096 bytes) for search/filter string values
- max filters per request (64)
- max selected columns (128)
- max searchable columns (64)
- max aggregate operations (64)

Requests outside these bounds are rejected with sanitized `invalid_request`.

The active policy is held behind a shared reloadable handle. Calling
`Policy::reload` or `Policy::reload_from_file` atomically replaces the policy
used by subsequent tool requests; existing tool instances do not need to be
recreated. A failed file load leaves the currently active policy unchanged.

## MCP stdio Transport

The binary exposes a real MCP stdio server when configured explicitly:

```bash
DATAGATE_MCP_STDIO=1 \
DATAGATE_BACKEND=sqlite \
SQLITE_PATH=/path/to/database.db \
DATAGATE_CONFIG=/path/to/datagate.toml \
cargo run
```

The transport uses the `rmcp` SDK and exposes `initialize`, `tools/list`, and
`tools/call` for the policy-safe `select`, `search`, and `aggregate` tools.
The first transport integration supports SQLite explicitly; PostgreSQL and
MySQL/MariaDB wiring remains a follow-up in the bootstrap layer.

The transport returns the existing sanitized public error envelope and never
accepts caller-provided SQL.

Current backend implementations behind the read-only abstraction:

- PostgreSQL
- MySQL/MariaDB
- SQLite

Backend selection is configurable via `DATAGATE_BACKEND` (`auto`, `postgres`,
`mysql`/`mariadb`, `sqlite`). In `auto` mode, precedence is PostgreSQL,
MySQL/MariaDB, then SQLite.

## Tool: select

### Request Contract

`SelectToolRequest`

- `request_id: String`
- `table: String` (supports `table` and `schema.table`)
- `columns: Vec<String>`
- `filters: Vec<SelectToolFilter>` (default empty)
- `limit: Option<u32>`

`SelectToolFilter`

- `column: String`
- `operator: SelectToolOperator`
- `value: serde_json::Value`
- `value_to: Option<serde_json::Value>` (required for `between`, ignored otherwise)

`SelectToolOperator`

- `equals`
- `not_equals`
- `less_than`
- `less_than_or_equal`
- `greater_than`
- `greater_than_or_equal`
- `like`
- `ilike`
- `between`
- `full_text`

Advanced filter semantics:

- `between`: builds `column BETWEEN $n AND $n+1` with two parameterized binds.
- `like`/`ilike`: require text bind values.
- `full_text`: builds `to_tsvector('simple', coalesce(column::text, '')) @@ plainto_tsquery('simple', $n)` with a single text bind.

Policy constraints:

- `filter_operators` is optional per table/column in configuration.
- if a column declares `filter_operators`, only those operators are accepted.
- if not declared, all operators remain allowed (backward compatible behavior).
- policy table keys can be unqualified (`users`) or schema-qualified (`audit.users`).
- an unqualified request that matches multiple schema-qualified policy keys is rejected as ambiguous.

### Execution Flow

1. `prepare` validates rate limit (if configured).
2. JSON request is converted to internal `SelectRequest`.
3. Query builder validates table/columns/filters and policy limits.
4. Backend executes a parameterized SELECT (including advanced predicates for
    `between` and `full_text` when requested).
5. Response payload size is validated against `max_output_bytes`.

### Response Contract

`SelectToolResponse`

- `request_id: String`
- `columns: Vec<String>`
- `rows: Vec<serde_json::Value>`

## Tool: search

### Request Contract

`SearchToolRequest`

- `request_id: String`
- `table: String` (supports `table` and `schema.table`)
- `columns: Vec<String>`
- `searchable_columns: Vec<String>`
- `text: String`
- `limit: Option<u32>`

### Execution Flow

1. `prepare` validates rate limit (if configured).
2. Request is converted to internal `SearchRequest`.
3. Query builder validates table/columns/searchable columns via policy.
4. Query builder produces a parameterized `ILIKE` query using `%text%` bind.
5. Backend executes through controlled read-only path.
6. Response payload size is validated against `max_output_bytes`.

### Response Contract

`SearchToolResponse`

- `request_id: String`
- `columns: Vec<String>`
- `rows: Vec<serde_json::Value>`

## Tool: aggregate

### Request Contract

`AggregateToolRequest`

- `request_id: String`
- `table: String` (supports `table` and `schema.table`)
- `operations: Vec<AggregateToolOperation>`
- `filters: Vec<SelectToolFilter>` (default empty)

`AggregateToolOperation`

- `function: count | sum | avg | min | max`
- `column: String` (`*` is allowed only for `count`)
- `alias: Option<String>`

### Execution Flow

1. `prepare` validates rate limit (if configured).
2. Operations and filters are converted to internal typed values.
3. Query builder validates every table, column, alias and complexity budget.
4. Backend executes one parameterized aggregate SELECT through the read-only path.
5. Response payload size is validated against `max_output_bytes`.

### Response Contract

`AggregateToolResponse`

- `request_id: String`
- `columns: Vec<String>`
- `rows: Vec<serde_json::Value>`

## Error Mapping Rules

Internal errors are never returned as-is.

- Policy denials -> `policy_denied`
- Invalid input / unsupported value shape -> `invalid_request`
- Rate limit denial -> `rate_limited`
- Backend execution/connectivity failure -> `backend_unavailable`
- Serialization/internal conversion edge failures -> `internal_error`

## Coverage Notes

Current line coverage is constrained mostly by bootstrap and DB-bound code paths
that are not deterministic to unit test without integration infrastructure.

Sonar coverage scope should keep focus on unit-testable business logic while the
project incrementally adds integration testing.
