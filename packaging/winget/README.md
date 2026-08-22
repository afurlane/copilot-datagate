# WinGet Evaluation

WinGet is a suitable follow-up Windows package channel, but it should not block
MCP Registry readiness.

## Recommendation

Use Scoop first for Windows because the manifest is simple and can directly track
GitHub Release assets. Add WinGet after a stable 1.0 release is available and the
package identity is final.

## Requirements to Complete

- Choose package identifier, for example `AlessandroFurlan.CopilotDataGate`.
- Publish a stable GitHub Release with Windows x86_64 and ARM64 archives.
- Generate WinGet manifests with installer URLs and SHA-256 hashes.
- Submit to the WinGet package repository and pass validation.

## DataGate Constraints

- No database credentials in manifests.
- Package installs only the `copilot-datagate` executable.
- Project setup remains `copilot-datagate init` plus environment variables.
