# PostgreSQL MCP Server Comparison

This document compares DataGate with selected PostgreSQL-focused MCP servers and
packages based on publicly available project pages and package metadata checked
on 2026-08-22.

The comparison is not a benchmark and does not claim feature parity. It focuses
on product positioning, security posture, transport/distribution model, and how
well each project matches DataGate's goal: a policy-driven, read-only MCP
security boundary between AI tools and real databases.

## Summary

DataGate is intentionally narrower than many PostgreSQL MCP servers. It does not
try to be a DBA assistant, SQL workbench, web UI, migration tool, or remote
multi-user service. Its core differentiator is that MCP tools never accept free
SQL: callers send structured requests (`select`, `search`, `aggregate`), DataGate
applies policy first, and only then builds parameterized SQL.

Most comparable projects expose more PostgreSQL capabilities. That is useful for
development, administration, tuning, or exploratory workflows, but it also
expands the threat model. DataGate is positioned as the conservative option for
teams that want AI-assisted data access without giving an agent a general SQL
execution surface.

## High-Level Matrix

| Project | Main Positioning | DB Scope | Writes / SQL Surface | Transport / UX | Security Posture |
| --- | --- | --- | --- | --- | --- |
| DataGate | Policy-first safe data gateway for AI tools | PostgreSQL, MySQL/MariaDB, SQLite | No free SQL; structured `select`, `search`, `aggregate` only | Local MCP stdio, CLI, project-aware config, MCP Registry published | Deny-by-default policy, read-only backend, sanitized errors, server-side limits |
| pgEdge `pgedge-postgres-mcp` | PostgreSQL MCP server plus Natural Language Agent CLI and Web UI | PostgreSQL 14+ | Read-only protection by default, rich query/tools surface | CLI stdio, CLI HTTP, Web UI, Docker, clients | Read-only enforcement, TLS/HTTPS, user/token auth, config for multiple DBs |
| `pg-mcp` / `stuzero/pg-mcp-server` | Multi-tenant PostgreSQL MCP bridge with resources and NL-to-SQL prompt | PostgreSQL, multiple DBs | Read-only SQL query tool using connection IDs | HTTP/SSE server mode, Python/FastMCP, web/client architecture | Read-only mode by default; credentials provided to connect tool and mapped to connection IDs |
| CrystalDBA `postgres-mcp` | Postgres MCP Pro: DBA/performance assistant | PostgreSQL | `execute_sql`; restricted and unrestricted modes | stdio and SSE; Docker, `uv`, `pipx` | Safe SQL execution, configurable access mode, SQL parsing, read-only transaction mode |
| `pgsql-mcp-server` (PyPI) | Async PostgreSQL MCP server with broad SQL operation classes | PostgreSQL | DQL, DML, DDL, DCL tools | Python package, `uvx`, `pip`; MCP Inspector examples | Transactional safety and rollback, but broad write/admin tool surface |
| `pgsql-mcp` (PyPI) | PostgreSQL MCP package | PostgreSQL | Summary says read-only default and super-code write access | Python package metadata only checked | Limited public detail from metadata checked |
| HenkDz PostgreSQL MCP Server | Comprehensive PostgreSQL database management MCP server | PostgreSQL | Readonly by default; write/admin/unsafe modes enable mutations/arbitrary SQL | npm/npx, Docker, Smithery, manual Node | Security modes, destructive opt-in, schema validation, audit notes |
| Microsoft PostgreSQL extension MCP server | MCP provider inside VS Code PostgreSQL extension | Azure/PostgreSQL extension context | Not enough detail in public reference page checked | VS Code extension-provided MCP server | Extension-managed; public reference page is brief |

## DataGate

Repository/package: `afurlane/copilot-datagate`, `@copilot-datagate/cli`, MCP
Registry name `io.github.afurlane/copilot-datagate`.

### Verified Capabilities

- MCP stdio transport.
- Tools: `select`, `search`, `aggregate`.
- PostgreSQL, MySQL/MariaDB, and SQLite backends.
- Project-aware config resolution:
  - `DATAGATE_CONFIG`
  - `.datagate/datagate.toml`
  - `datagate.toml`
  - user config
  - deny-all fallback
- One named active connection per selected profile.
- No credentials in project config or registry metadata.
- Server-side request limits, output limits, rate limiting, audit/metrics hooks.
- Published MCP Registry entry for version `0.4.0`.

### Differentiator

DataGate does not accept arbitrary SQL through MCP. This is the key difference.
Even read-only arbitrary SQL still lets an agent exfiltrate any table/column the
DB role can see, explore system catalogs, and generate expensive queries. DataGate
forces a smaller semantic contract and policy validation before SQL is built.

### Tradeoffs

- Less flexible than tools that expose SQL execution.
- No web UI, no natural-language-to-SQL agent, no DBA performance advisor.
- Multi-connection workflows are designed for later, not active in 1.0.

## pgEdge `pgedge-postgres-mcp`

Source checked: `https://github.com/pgEdge/pgedge-postgres-mcp`.

### Observed Positioning

pgEdge positions this as a PostgreSQL MCP Server and Natural Language Agent,
with a CLI, HTTP mode, and Web UI. Public docs describe installation/setup for
multiple clients and include Docker deployment.

### Observed Capabilities

- PostgreSQL 14+.
- CLI stdio for local single-user development.
- CLI HTTP and Web UI for multi-user/remote/browser usage.
- Natural Language Agent CLI and web client.
- Query execution, schema analysis, hybrid search, embedding generation,
  resources, prompts, and guided workflows.
- Multiple database configuration is documented.
- Docker images and Docker Compose deployment are documented.

### Security Notes

Public README states:

- read-only transaction enforcement by default
- TLS/HTTPS support
- user and token authentication
- file permission enforcement
- input validation/sanitization
- warning that it is not for public-facing applications and should be used for
  internal/trusted workflows

### Comparison With DataGate

pgEdge is broader and more product-complete for natural-language database
interaction, web usage, and multi-user/HTTP scenarios. DataGate is narrower and
more conservative: local stdio default, no HTTP default, no free SQL surface, and
policy-first table/column allow-listing.

## `pg-mcp` / `stuzero/pg-mcp-server`

Sources checked:

- `https://stuzero.github.io/pg-mcp/overview/`
- `https://github.com/stuzero/pg-mcp-server`

### Observed Positioning

`pg-mcp-server` is a multi-tenant MCP server for PostgreSQL databases. It uses a
resource-oriented architecture and exposes schema/data context through MCP
resources. The architecture includes `pg-mcp-client`, `pg-mcp-agent`, and
`pg-mcp-server` communicating over HTTP/SSE.

### Observed Capabilities

- Multi-database support.
- Connection management through a `connect` tool that registers a connection
  string and returns an opaque connection ID.
- `disconnect` tool.
- `pg_query` read-only query execution.
- `pg_explain` query-plan analysis.
- Resource templates for schemas, tables, columns, extensions, row counts, and
  database descriptions.
- Prompt for natural-language-to-SQL.
- Extension context for PostGIS and pgvector.

### Security Notes

The docs describe read-only mode by default and mention that credentials are
sent once during initial connection, then hidden behind connection IDs. The public
overview also says the browser client stores API keys and connection strings.

### Comparison With DataGate

`pg-mcp` is better suited to dynamic/multi-database exploration and resource
navigation. DataGate is better suited to a fixed project/workspace security
boundary: one named active connection, config outside chat history, no connection
strings through MCP tool calls, no free SQL.

## CrystalDBA `postgres-mcp` / Postgres MCP Pro

Source checked: `https://github.com/crystaldba/postgres-mcp`.

The user-provided list included this project twice; it is covered once here.

### Observed Positioning

Postgres MCP Pro is positioned as a development-to-production PostgreSQL
assistant. It combines database health checks, index tuning, query plans, schema
intelligence, and safe SQL execution.

### Observed Capabilities

- Database health checks: index health, connections, buffer cache, vacuum,
  sequences, replication lag, and more.
- Index tuning using workload analysis and hypothetical indexes.
- EXPLAIN plans and query optimization tools.
- Schema intelligence for SQL generation.
- Tools include `execute_sql`, `explain_query`, `get_top_queries`,
  workload/index analyzers, and database health analysis.
- Supports stdio and SSE transports.
- Installation via Docker, `pipx`, `uv`, or source.
- Optional PostgreSQL extensions such as `pg_stat_statements` and `hypopg` for
  full tuning capabilities.

### Security Notes

Docs describe access modes:

- unrestricted mode for development
- restricted mode for read-only production-like use

Protected SQL execution uses read-only transactions and SQL parsing to reject
`COMMIT`/`ROLLBACK` patterns that could bypass transaction boundaries. It still
exposes SQL execution as a core capability.

### Comparison With DataGate

CrystalDBA is much stronger as a DBA/performance assistant. DataGate is much
smaller and deliberately avoids SQL execution entirely at MCP boundary. For
performance tuning, CrystalDBA wins. For minimizing what an AI agent can ask the
DB to do, DataGate is stricter.

## `pgsql-mcp-server` (PyPI)

Sources checked:

- `https://pypi.org/project/pgsql-mcp-server/`
- PyPI JSON metadata for `pgsql-mcp-server`

### Observed Positioning

`pgsql-mcp-server` is a Python/FastMCP PostgreSQL MCP server using SQLAlchemy and
`asyncpg` for asynchronous database operations.

### Observed Capabilities

- Python package version checked: `1.4.6`.
- Requires Python `>=3.10,<3.15`.
- Installation via `uv tool install`, `uvx`, or `pip`.
- Tools listed publicly:
  - `get_schema_names`
  - `get_tables`
  - `get_columns`
  - `get_indexes`
  - `get_foreign_keys`
  - `run_dql_query`
  - `run_dml_query`
  - `run_ddl_query`
  - `run_dcl_query`
- MCP Inspector usage documented.

### Security Notes

The page highlights transactional safety and rollback for DDL/DML/DCL. It also
exposes DML, DDL, and DCL tool categories, so the scope is broader than
read-only data access.

### Comparison With DataGate

`pgsql-mcp-server` is a general database interaction server. DataGate is a
security boundary with no DML/DDL/DCL path and no free SQL path.

## `pgsql-mcp` (PyPI)

Source checked: PyPI JSON metadata for `pgsql-mcp`. The rendered PyPI page was
not fully accessible during collection due an anti-bot/CAPTCHA challenge.

### Observed Metadata

- Version checked: `0.1.0`.
- Summary: "A Model Context Protocol server for PostgreSQL with read-only
  default and super-code write access."
- Requires Python `>=3.12`.

### Comparison With DataGate

The available metadata suggests a read-only default with an elevated write mode.
That is closer to HenkDz or CrystalDBA-style mode switching than DataGate's
current stance. DataGate does not include an elevated MCP write mode.

Because only package metadata was reliably available, deeper feature comparison
should be revisited if the project documentation becomes accessible.

## HenkDz PostgreSQL MCP Server

Source checked: `https://mcpservers.org/servers/HenkDz/postgresql-mcp-server`.

### Observed Positioning

The page describes a comprehensive PostgreSQL management MCP server that was
redesigned from many individual tools into consolidated meta-tools.

### Observed Capabilities

- npm install / npx usage.
- Docker usage.
- Smithery installation.
- 18 tools organized into:
  - consolidated meta-tools
  - enhancement tools
  - specialized tools
- Query, mutation, arbitrary SQL, comments, schema/user/index/function/trigger/
  constraint/RLS management, export/import, copy, monitoring.

### Security Notes

Public docs describe:

- default `readonly` mode
- `write`, `admin`, and `unsafe` modes
- destructive operations requiring explicit opt-in
- per-tool connection strings disabled by default
- allowed connection target restrictions
- structured `where` predicates and raw escape hatches only in unsafe mode
- unknown fields rejected by tool schemas
- sanitized audit events

### Comparison With DataGate

HenkDz is a much broader PostgreSQL management surface with strong security
modes. DataGate is much narrower: it does not have write/admin/unsafe modes, does
not expose arbitrary SQL, and currently exposes only semantic read tools.

## Microsoft PostgreSQL Extension MCP Server

Source checked:
`https://learn.microsoft.com/en-us/azure/postgresql/development/vs-code-extension/reference/mcp-server`.

### Observed Positioning

Microsoft documents an MCP server definition provider registered by the
PostgreSQL extension for VS Code. The public reference page checked is brief.

### Observed Capabilities

- Provider ID: `pgsql-tools-mcp-server-provider`.
- Label: PostgreSQL Tools MCP Server Provider.
- The extension exposes database tools to AI assistants/language models through
  MCP.

### Comparison With DataGate

This is extension-integrated, rather than a standalone cross-editor server. Based
on the public reference checked, it is not possible to compare detailed tool
surface or security behavior. DataGate is standalone, registry-published, and
cross-editor local stdio-oriented.

## pgEdge vs DataGate vs DBA Tools

A useful grouping:

- DataGate: conservative gateway / security boundary.
- pgEdge: full product with natural-language agent, web UI, HTTP, auth, and
  broader database exploration.
- CrystalDBA: DBA/performance optimizer with explain/index/health tooling.
- HenkDz: broad PostgreSQL management server with security modes.
- stuzero pg-mcp: multi-database resource-oriented exploration with connection
  IDs and HTTP/SSE architecture.
- pgsql-mcp-server: broad Python database operations server.
- Microsoft extension MCP: IDE extension-provided server.

## Where DataGate Is Stronger

- No free SQL accepted through MCP.
- Policy-first table/column allow-listing.
- Deny-by-default behavior when config is absent or invalid.
- Structured operations only.
- Multi-backend support beyond PostgreSQL.
- Project-aware config without storing secrets in MCP client metadata.
- Local stdio default and explicit decision not to ship HTTP by default.

## Where Other Projects Are Stronger

- PostgreSQL-specific depth.
- Web UI / natural-language chat UX.
- Multiple live database connections.
- Rich schema resource navigation.
- Performance tuning, EXPLAIN, pg_stat_statements, hypopg, index advisor flows.
- Write/admin workflows under configurable modes.
- Docker/server deployment options.

## Recommendation

Keep DataGate's 1.0 positioning narrow and explicit:

> DataGate is a policy-driven, read-only, no-free-SQL MCP gateway for safe AI data
> access across configured database backends.

Do not compete head-on with Postgres DBA assistants. Instead, document the
tradeoff: DataGate intentionally gives agents less freedom so it can be safer by
default for real project databases.

## Sources Checked

- `https://github.com/pgEdge/pgedge-postgres-mcp`
- `https://stuzero.github.io/pg-mcp/overview/`
- `https://github.com/stuzero/pg-mcp-server`
- `https://github.com/crystaldba/postgres-mcp`
- `https://pypi.org/project/pgsql-mcp-server/`
- `https://pypi.org/pypi/pgsql-mcp-server/json`
- `https://pypi.org/pypi/pgsql-mcp/json`
- `https://mcpservers.org/servers/HenkDz/postgresql-mcp-server`
- `https://learn.microsoft.com/en-us/azure/postgresql/development/vs-code-extension/reference/mcp-server`
