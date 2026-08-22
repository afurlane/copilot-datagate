# Packaging Channels

DataGate uses GitHub Releases as the canonical source for built binaries. Other
channels are installer descriptors or thin wrappers around those release assets.

## Canonical Artifacts

Release assets are produced by the `Release` workflow for:

- `linux-x86_64`
- `linux-aarch64`
- `macos-x86_64`
- `macos-aarch64`
- `windows-x86_64`
- `windows-aarch64`

Each platform archive contains the `copilot-datagate` executable and is uploaded
with target-specific SHA-256 checksum files.

## Channel Status

| Channel | Status | Purpose |
| --- | --- | --- |
| GitHub Releases | Canonical | Official binary artifacts and checksums |
| Homebrew | Template ready | macOS/Linux install path |
| Scoop | Template ready | Windows install path |
| npm wrapper | Candidate ready | `npx`-friendly MCP client install path |
| WinGet | Evaluated | Follow-up after Scoop is proven |
| crates.io | Evaluated | Rust-user install path, not default UX |
| OCI/Docker | Evaluated | Automation/isolation scenarios, not default IDE UX |

The 0.4.0 Homebrew and Scoop templates include the real GitHub Release asset
URLs and SHA-256 values. The npm wrapper package is versioned at 0.4.0 and can
be dry-run packed locally, but actual npm publication requires an authenticated
npm account.

## Versioning

Release asset tags follow release-please tags such as:

```text
copilot-datagate-v0.4.0
```

Archive names follow the release workflow shape:

```text
copilot-datagate-<tag>-<platform>.tar.gz
copilot-datagate-<tag>-<platform>.zip
```

Example:

```text
copilot-datagate-copilot-datagate-v0.4.0-linux-x86_64.tar.gz
```

## Publication Order

1. Publish a GitHub Release through release-please.
2. Update Homebrew/Scoop templates with the release tag and checksums.
3. Publish optional npm wrapper if MCP Registry/client UX requires `npx`.
4. Generate registry `server.json` for the selected primary package channel.
5. Validate with `mcp-publisher validate`.
6. Publish with `mcp-publisher publish`.

## Security Notes

- Do not place database credentials in package metadata.
- Do not place database credentials in editor MCP templates.
- Keep DataGate project configuration local to `.datagate/datagate.toml`,
  `datagate.toml`, or user config.
- Keep GitHub Release checksums as the integrity baseline for package-manager
  descriptors.
