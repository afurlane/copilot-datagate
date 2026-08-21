# DataGate 1.0 Release Candidate

This document defines the release-candidate baseline for DataGate. It is a
readiness contract, not a release announcement. The final 1.0 release still
requires the open items listed in the roadmap.

## Candidate Scope

The candidate includes:

- policy-first, parameterized `select`, `search`, and `aggregate` tool contracts
- read-only PostgreSQL, MySQL/MariaDB, and SQLite backends
- schema-qualified policy resolution and deny-by-default behavior
- server-side row, complexity, output, request-shape, and rate limits
- sanitized public errors, structured audit events, and in-process metrics
- atomic policy reload through `Policy::reload` and `Policy::reload_from_file`
- documented configuration and operational security boundaries

## Required Quality Gates

Run these commands from the repository root:

```bash
cargo fmt --all -- --check
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo audit
cargo deny check
```

The pull-request workflows must be green for:

- Build, Test & Lint
- cargo-audit & cargo-deny
- SonarQube Quality Scan and its external quality-gate check
- CodeQL (Rust)
- Conventional commit validation

Documentation-only pull requests still run the required pull-request checks.
Push filters on `master` may avoid rebuilding for documentation-only commits,
because the merge commit has already passed the pull-request checks.

## Security Acceptance Criteria

- No MCP path accepts caller-provided SQL.
- No MCP path reaches a write operation or a non-read-only backend connection.
- Policy validation happens before query-plan generation and backend execution.
- Public errors do not expose SQL, stack traces, secrets, filter values, or
  unauthorized object names.
- Credentials are supplied through runtime environment variables only.
- Integration database tests are opt-in through dedicated environment variables.

## Operational Notes

The current binary initializes configuration and the selected backend, while
MCP transport wiring remains a post-candidate item. The supported tool contracts
are exposed as typed Rust APIs and are documented in [mcp-tools.md](mcp-tools.md).
Use [configuration.md](configuration.md) for local setup, backend variables,
policy profiles, observability, and verification commands.

The release version and changelog are managed by release-please. Do not edit
`Cargo.toml` version or `CHANGELOG.md` manually as part of candidate work.

## Remaining 1.0 Release Work

- stabilize and publish the MCP transport/API contract
- add end-to-end tests with Copilot
- add reproducible benchmarks
- complete final hardening review
- publish the official release
