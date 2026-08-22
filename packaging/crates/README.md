# crates.io Evaluation

Publishing DataGate to crates.io would enable `cargo install copilot-datagate`,
but it is not the default user experience for MCP Registry and editor users.

## Recommendation

Defer crates.io publication until after 1.0 unless Rust users explicitly need it.
Most VS Code, Visual Studio, Rider, and IntelliJ users should not need a Rust
toolchain just to install an MCP server.

## Required Changes Before Publishing

- Remove `publish = false` from `Cargo.toml`.
- Confirm package metadata is complete and stable.
- Ensure generated release/changelog behavior remains release-please-managed.
- Decide whether benchmark binary should be included in the crate package.
- Run `cargo package --list` and `cargo publish --dry-run`.

## Registry Impact

If MCP Registry supports crates.io as a package type with ownership verification,
DataGate can add the required package metadata and derive `server.json` from the
same canonical registry metadata.
