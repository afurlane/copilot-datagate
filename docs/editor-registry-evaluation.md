# Editor and MCP Registry Compatibility Evaluation

This document evaluates whether DataGate can be published and installed as a
local MCP stdio server across MCP Registry, VS Code, Visual Studio, and JetBrains
IDEs such as Rider and IntelliJ IDEA.

## Verdict

DataGate is compatible with the local stdio server model used by the evaluated
clients. For the 1.0 line, the clean registry path is:

1. Publish an installable package that provides the `copilot-datagate` command.
2. Publish MCP Registry metadata for that package with stdio transport.
3. Keep project database settings in `.datagate/datagate.toml` or user config.
4. Keep secrets in environment variables or native database mechanisms, never in
   registry/editor metadata.

Direct registry publication from GitHub Release binaries alone might not be
sufficient because the MCP Registry is metadata-only and expects package
ownership verification for supported package types. DataGate should keep GitHub
Releases as the canonical binary source and support multiple package-manager
descriptors around those artifacts.

## MCP Registry

Observed requirements and implications:

- The registry hosts metadata, not artifacts.
- Publishing uses `mcp-publisher` and a `server.json` manifest.
- The server name must be ownership-verifiable. With GitHub authentication, use
  the `io.github.<owner>/<server>` namespace.
- A package must exist before registry publication. The documented package flows
  include npm and other package types.
- A package entry declares transport, for example stdio.
- Environment variables can be described in metadata, including secret flags, but
  DataGate should avoid requiring DB credentials in registry metadata.

DataGate impact:

- Recommended registry name: `io.github.afurlane/copilot-datagate`.
- Recommended package path: npm wrapper first, because VS Code, Visual Studio,
  JetBrains, Claude Desktop-style clients, and reference MCP examples commonly
  support local command launch via package-manager wrappers.
- The package should launch the GitHub Release binary for the host platform.
- Registry metadata should expose only `DATAGATE_PROJECT_ROOT` as an optional
  non-secret env var. Database credentials stay project/user-local.

## VS Code

Observed support:

- VS Code supports local stdio servers through `command` and `args` in MCP
  configuration.
- Users can configure MCP servers at workspace or user-profile level.
- VS Code has an MCP server gallery (`@mcp`) and can install servers into a user
  profile or workspace.
- VS Code warns users to avoid hardcoding sensitive values in MCP config.
- VS Code Agent Host portability is better with workspace `.mcp.json` or user
  `~/.copilot/mcp-config.json` than with interactive input variables.
- VS Code supports sandboxing for local stdio MCP servers on macOS/Linux, but not
  Windows.

DataGate impact:

- Compatible with local stdio.
- Use command template:

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

- For GitHub Copilot Agent Host portability, avoid `${input:...}` secrets in the
  MCP template. Use project config plus environment variables instead.
- Optional sandbox guidance can be added for macOS/Linux users once registry
  metadata format support is confirmed.

## Visual Studio

Observed support:

- Visual Studio 2022 17.14+ and Visual Studio 2026 support MCP servers for
  GitHub Copilot agent mode.
- Visual Studio supports installing MCP servers from the GitHub MCP server
  registry.
- Visual Studio supports local command-line MCP servers using command, args, and
  transport metadata.
- Visual Studio discovers MCP configuration from multiple locations, including
  user-level `.mcp.json`, solution `.vs/mcp.json`, solution `.mcp.json`,
  `.vscode/mcp.json`, and `.cursor/mcp.json`.
- Visual Studio restarts and reloads servers when valid MCP configuration files
  change.
- Tools are subject to trust and tool-approval workflows. Enterprise allow-list
  policies can restrict which MCP servers are available.

DataGate impact:

- Compatible with local stdio.
- Registry publication is valuable because Visual Studio has a registry install
  flow.
- DataGate should document both solution-level `.mcp.json` and workspace
  `.vscode/mcp.json` templates.
- The command template should avoid shell-specific wrappers when the binary is
  installed on `PATH`.

## Rider and IntelliJ IDEA

Observed support:

- JetBrains AI Assistant supports connecting to MCP servers.
- JetBrains MCP configuration supports STDIO and HTTP.
- STDIO configuration accepts JSON with `mcpServers`, `command`, and `args`.
- The UI supports working directory and global/project server levels.
- JetBrains can import Claude Desktop MCP server configuration.
- Organizations can centrally manage allowed MCP servers through JetBrains IDE
  Services or JetBrains Central.

DataGate impact:

- Compatible with local stdio through manual JSON configuration.
- Registry integration is not confirmed as equivalent to VS Code/Visual Studio;
  treat JetBrains support as manual/project-level configuration until proven
  otherwise.
- Use a JetBrains/Claude-style template:

```json
{
  "mcpServers": {
    "copilot-datagate": {
      "command": "copilot-datagate",
      "args": ["mcp", "stdio"],
      "env": {
        "DATAGATE_PROJECT_ROOT": "/path/to/project"
      }
    }
  }
}
```

- Users should set the working directory to the project root when configuring the
  server through the JetBrains UI.

## Packaging Recommendation

Use a staged publishing plan:

1. Keep GitHub Releases as canonical signed/checksummed binary artifacts.
2. Add package-manager descriptors in priority order: Homebrew, Scoop, optional
   npm wrapper for `npx`-oriented MCP clients, then WinGet / crates.io / OCI as
   follow-up channels.
3. Add `mcpName = "io.github.afurlane/copilot-datagate"` or the package-type
   equivalent required by each published package channel.
4. Generate a registry-valid `server.json` from
   [mcp-registry-metadata.json](mcp-registry-metadata.json) once package type and
   version are fixed.
5. Validate with `mcp-publisher validate`.
6. Publish with `mcp-publisher publish` only after ownership verification and
   package publication succeed.

## Open Decisions Before Publication

- Choose the first package channel. Recommendation: Homebrew + Scoop first,
  with npm wrapper added if registry/client UX requires `npx`.
- Decide whether to commit generated `server.json` or derive it from
  `docs/mcp-registry-metadata.json` during release.
- Decide whether to add Homebrew/Scoop/WinGet before or after registry
  publication.
- Verify final MCP Registry schema version and package-type requirements at the
  time of publication.
- Confirm Visual Studio registry install behavior with a real published package.
- Confirm whether JetBrains has direct registry consumption or should remain
  manual JSON setup for 1.0.

## Conclusion

The evaluated clients support DataGate's local stdio direction. Publication is
compatible if DataGate provides an installable local command and keeps database
configuration project-aware. HTTP transport is not required for cross-editor
registry readiness and remains intentionally out of scope for 1.0.

## Sources Checked

- MCP Registry quickstart and publisher flow:
  `https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/quickstart.mdx`
- MCP Registry repository and development status:
  `https://github.com/modelcontextprotocol/registry`
- MCP reference servers packaging examples:
  `https://github.com/modelcontextprotocol/servers`
- VS Code MCP server management and gallery documentation:
  `https://code.visualstudio.com/docs/copilot/chat/mcp-servers`
- Visual Studio MCP server documentation:
  `https://learn.microsoft.com/en-us/visualstudio/ide/mcp-servers`
- JetBrains AI Assistant MCP documentation:
  `https://www.jetbrains.com/help/ai-assistant/configure-an-mcp-server.html`
