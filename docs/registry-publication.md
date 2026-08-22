# MCP Registry Publication Strategy

DataGate should be published as a local MCP stdio server for the 1.0 line. The
registry entry must be static and cross-editor friendly; project-specific
database configuration stays outside the registry metadata.

## Goals

- Allow installation and use from MCP-capable clients such as VS Code, Visual
  Studio, Rider, and other editors that can launch local stdio servers.
- Avoid storing database credentials in registry manifests, editor config, or
  project files.
- Keep HTTP remote transport out of the default registry story unless a separate
  security decision approves it.
- Make the first configuration model single-connection per workspace while
  reserving a clear path to future multi-connection workflows.

## Distribution Model

Canonical release artifacts remain GitHub Releases:

- Linux x86_64/aarch64
- macOS x86_64/aarch64
- Windows x86_64/aarch64
- SHA-256 checksum files

A cross-editor registry entry should invoke an installed local command rather
than embedding platform-specific paths. The preferred command shape is:

```bash
copilot-datagate mcp stdio
```

A package wrapper or package-manager manifest can later provide this command by
downloading or dispatching to the matching GitHub Release binary for the host
platform. Candidate package channels:

- GitHub Release binary as the canonical artifact source
- Homebrew tap for macOS/Linux
- Scoop for Windows
- npm wrapper for clients that support `npx` command templates
- WinGet for Windows once the simpler Windows path is proven
- crates.io / `cargo install` for Rust users, if publishing the crate becomes desirable
- OCI/Docker for non-IDE automation scenarios, not as the default local IDE path

The intended shape is one canonical build artifact set with multiple installer
descriptors around it. npm is useful for MCP clients that prefer `npx`, but it is
not a requirement of DataGate itself.

## Registry Template

A registry template should be stable and avoid database-specific parameters:

Candidate metadata is tracked in [mcp-registry-metadata.json](mcp-registry-metadata.json)
and the MCP Registry manifest candidate is tracked in [server.json](../server.json).
The cross-editor compatibility assessment is tracked in
[editor-registry-evaluation.md](editor-registry-evaluation.md).
Packaging channel details are tracked in [packaging.md](packaging.md).

```json
{
  "servers": {
    "copilot-datagate": {
      "type": "stdio",
      "command": "copilot-datagate",
      "args": ["mcp", "stdio"],
      "cwd": "${workspaceFolder}",
      "env": {
        "DATAGATE_PROJECT_ROOT": "${workspaceFolder}"
      }
    }
  }
}
```

The template only starts DataGate. DataGate resolves the project configuration
from the workspace.

## Project Configuration Resolution

DataGate should resolve configuration in this order:

1. `DATAGATE_CONFIG`
2. `${DATAGATE_PROJECT_ROOT}/.datagate/datagate.toml`
3. `${DATAGATE_PROJECT_ROOT}/datagate.toml`
4. user-level config, for example `~/.config/datagate/config.toml`
5. deny-all fallback

This keeps the registry entry reusable across projects while allowing each
workspace to choose its own backend and policy.

## Named Connection Model

Even while DataGate supports only one active connection per workspace, the
configuration should give that connection a stable name. This avoids a later
breaking change when multi-connection workflows are introduced.

Example single-connection configuration:

```toml
profile = "dev"

[profiles.dev.connection]
name = "pippo"
backend = "postgres"
url_env = "DB_URL"
options_env = "DB_OPTIONS"

[profiles.dev.policy]
default_row_limit = 100
max_row_limit = 1000
max_query_complexity = 100
max_output_bytes = 131072

[profiles.dev.policy.tables.domainevententry]
columns = [
  "aggregateidentifier",
  "sequencenumber",
  "type",
  "eventidentifier",
  "payloadtype",
  "timestamp"
]
```

Rules for the 1.0 line:

- exactly one active connection per selected profile
- every connection has a required stable `name`
- MCP tools do not accept a connection selector yet
- audit and metrics should include the connection name once wired
- future multi-connection support can add request-level selection without
  changing the basic connection object shape

A future multi-connection request could ask the agent to use named databases,
for example: "usa il db pippo e il db pluto e cerca questo: xyz". That future
mode must still enforce policy independently for each named connection.

## Auto-Configuration CLI

The registry experience should rely on a local setup command:

```bash
copilot-datagate init
copilot-datagate doctor
copilot-datagate mcp stdio
```

`init` should:

- detect the workspace root
- ask for a connection name
- ask for backend type (`postgres`, `mysql`, `sqlite`)
- record environment-variable names, not secret values
- test the connection when the referenced environment variables are present
- inspect schema through read-only paths
- generate a deny-by-default policy skeleton

`doctor` should:

- report which config file was selected
- report the selected profile and connection name
- check whether required environment variables exist without printing values
- check read-only backend connectivity
- check that at least one policy table is allow-listed

For 0.4.0, `server.json` targets the optional npm wrapper package
`@copilot-datagate/cli@0.4.0`, which has been published to npm. Future npm
publishes should use the manual `Publish npm wrapper` GitHub Actions workflow
with npm Trusted Publishing (OIDC), not a long-lived npm token. MCP Registry
publication for `io.github.afurlane/copilot-datagate` version `0.4.0` was
completed with `mcp-publisher publish` and is active in the registry.

## Secret Handling

Never store secrets in:

- registry metadata
- `mcp.json`
- project TOML files intended for version control
- examples or documentation

For 1.0, secrets should come from environment variables or native database
mechanisms already configured by the user. OS keychain integration can be a later
feature.

## HTTP Transport Decision

HTTP is not part of the registry default. It requires a separate decision record
because it changes the threat model from local process execution to network
exposure.

The current 1.0 decision is documented in
[http-transport-decision.md](http-transport-decision.md): registry publication
targets local stdio only.

Any future HTTP transport must define at least:

- TLS requirements
- authentication and authorization model
- bind-address defaults
- reverse-proxy assumptions
- request size and rate limits by principal/IP
- audit identity model
- replay and abuse protections

Until that decision is approved, registry publication targets local stdio only.
