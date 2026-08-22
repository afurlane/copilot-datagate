# HTTP Transport Decision Record

## Status

Accepted for the 1.0 line: do not implement or publish MCP HTTP transport as a
registry/default capability.

## Context

DataGate is a security boundary between AI tools and real databases. The current
MCP transport is local stdio: the MCP client starts a local process and exchanges
JSON-RPC frames over stdin/stdout. That model keeps the exposed surface close to
the user's workspace and avoids network access by default.

HTTP transport changes the threat model. A network-facing DataGate instance must
handle authentication, authorization, TLS, replay risk, request size limits,
principal/IP rate limits, reverse-proxy headers, and remote audit identity. Those
requirements are broader than the 1.0 local registry goal.

## Decision

DataGate 1.0 targets local MCP stdio only.

Registry and editor-gallery publication should describe DataGate as a local stdio
server. HTTP examples and templates must not be presented as the default or as an
implemented configuration path.

## Requirements Before Reconsidering HTTP

A future HTTP transport requires a separate design and implementation plan that
answers at least:

- TLS termination and certificate requirements
- authentication mechanism
- authorization model for profiles and named connections
- default bind address and explicit opt-in for non-localhost exposure
- request size limits at HTTP and MCP layers
- rate limiting by principal/IP, not only `request_id`
- audit identity for remote callers
- reverse-proxy and forwarded-header trust boundaries
- replay and abuse protections
- operational guidance for deployment and secret handling

## Consequences

- The registry path stays cross-editor and local-process based.
- The MCP template remains stable: launch `copilot-datagate mcp stdio` from the
  workspace.
- Remote access is not promised for 1.0.
- Project-specific configuration remains local to the workspace or user account.
