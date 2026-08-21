# MCP Tools Internal Documentation

This document is the internal reference for DataGate MCP tool contracts and
security behavior.

## Scope

Implemented tools:

- `select`
- `search`
- `aggregate`

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

## Tool: select

### Request Contract

`SelectToolRequest`

- `request_id: String`
- `table: String`
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

### Execution Flow

1. `prepare` validates rate limit (if configured).
2. JSON request is converted to internal `SelectRequest`.
3. Query builder validates table/columns/filters and policy limits.
4. Backend executes a parameterized SELECT.
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
- `table: String`
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
- `table: String`
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
