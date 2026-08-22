# @copilot-datagate/cli

Thin npm wrapper for DataGate. The canonical binaries are published in GitHub
Releases; this package exists for MCP clients and editors that prefer an `npx`
install path.

```bash
npx -y @copilot-datagate/cli mcp stdio
```

The wrapper downloads the matching GitHub Release binary for the host platform
and dispatches arguments to it.

## Environment

- `COPILOT_DATAGATE_BIN`: use an already installed binary instead of downloading.
- `COPILOT_DATAGATE_VERSION`: override the wrapper package version when selecting
  the release tag.
- `COPILOT_DATAGATE_CACHE`: override the binary cache directory.

Database credentials are not handled by this package. Configure DataGate through
project-local `.datagate/datagate.toml` and environment variables.
