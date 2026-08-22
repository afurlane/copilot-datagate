# OCI / Docker Evaluation

OCI packaging can be useful for automation and isolated environments, but it is
not the default path for local IDE stdio usage.

## Recommendation

Do not make Docker the primary 1.0 install path. Keep it as a follow-up for CI,
internal automation, or environments that already run MCP servers in containers.

## Tradeoffs

Pros:

- Reproducible runtime environment.
- Easy pinning by image digest.
- Useful for non-IDE automation.

Cons:

- More friction for local stdio MCP clients.
- Requires explicit mounts/env for project config and database access.
- Network and filesystem boundaries become Docker configuration concerns.
- Poor default experience for local database sockets or desktop IDE workflows.

## Minimum Requirements Before Publishing

- Read-only runtime image with non-root user.
- Documented mounts for `.datagate/datagate.toml` and optional user config.
- Environment variable pass-through for database secrets.
- No embedded credentials or example secrets.
- Image signing or digest pinning guidance.
