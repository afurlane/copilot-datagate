# Final Hardening Checklist

This checklist captures the 1.0 hardening bar for DataGate as an MCP security
boundary between AI tools and real databases.

## Protocol Boundary

- MCP stdio keeps stdout reserved for JSON-RPC frames.
- Application logs are routed to stderr.
- MCP stdio defaults to `warn` logging to avoid noisy client-side warnings.
- Unknown MCP tools are rejected by the transport without invoking backend paths.
- Public tool responses never expose raw SQL.

## Request Boundary

- `request_id` is bounded and restricted to stable ASCII identifier characters.
- Text payloads are capped before policy evaluation.
- Filter, column, searchable-column, and aggregate operation counts are bounded.
- Complex JSON filter values (`null`, arrays, objects) are rejected.
- Oversized output is rejected server-side after execution and before returning to MCP.

## Policy and Query Boundary

- Every tool request is converted to a typed internal request.
- Policy validation runs before query-plan construction.
- Query plans are built only by the controlled query builder.
- SQLx 0.9 dynamic SQL execution is explicitly audited with `AssertSqlSafe` only
  at the backend boundary that consumes controlled `QueryPlan` values.
- Unauthorized tables, columns, operators, and ambiguous table references are
  mapped to sanitized public errors.

## Backend Boundary

- Database credentials are read from environment variables only.
- Backend pools enforce read-only behavior as defense in depth.
- Backend failures are converted to `backend_unavailable` and do not expose SQL,
  schema names, database driver details, or stack traces.
- Backend integration tests remain environment-gated for PostgreSQL and MySQL.

## Validation Commands

Run before release candidates and before substantial PRs:

```bash
cargo fmt --check
cargo check -q
cargo test -q
cargo clippy --all-targets --all-features -- -D warnings
cargo audit
cargo deny check
cargo run --release --bin datagate-benchmark
```

## Remaining Release Work

Hardening does not include remote transport implementation or registry
publication. Those remain separate roadmap items:

- MCP HTTP transport for remote scenarios.
- Dual-mode transport selection for stdio and HTTP.
- MCP Registry publication and VS Code MCP Server Gallery evaluation.
