# Windows installer validation

## Current release inputs

Runtime **0.2.0** bundles **Sol Language Support**, extension identity
`jinxware00.solx-ls` **0.1.0**, as `solx-ls-0.1.0.vsix`. Bundle metadata and
schema-v2 provenance include the extension name, publisher and version.
Installer and test paths derive from that validated identity. Uninstall accepts
only current and legacy Solx package names for compatibility with older records.

The renamed VSIX passed all 18 packaged checks on VS Code 1.85.2. Current
installer fixture execution, the clean release bundle build and extracted
installation tests are deferred to hosted CI. Earlier results below do not
validate these updated installer bytes.

| Suite | Required checks |
| --- | ---: |
| CLI helper, quoting, timeout and inherited-pipe handling | 13 |
| File/PATH transformations, checksums, identity and rollback | 7 |
| Build inputs, identity, VSIX contents and provenance | 14 |
| Release preparation, privacy and tampering rejection | 14 |
| Extracted terminal installation/uninstallation | 18 |

The terminal suite covers selected/skipped editor installation, CLI failures,
decline/quit/EOF, upgrade backups, rollback, damaged payloads, unsafe uninstall
metadata and preservation of user files and separately installed extensions.
All test installations and editor profiles use ignored `.artifacts` directories;
actual user PATH writes are disabled. `-CodeRoot` must be absolute. The release
workflow builds once and runs the extracted suite with `-SkipBuild`.

## Historical evidence - 2026-10-25

The earlier runtime 0.2.0 / extension 0.4.0 bundle passed all 18 extracted
scenarios in `.artifacts/terminal-877f07fe4dc14cfd8f514d60275a0ca8/results.json`.
It also passed 13 CLI helper, 6 file/PATH and 13 build/provenance checks.
The successful terminal run completed in 58.890 seconds with a 1 GiB job cap,
232,583,168 peak committed job bytes and no remaining child processes. Normal
interpreter, PATH, editor settings and extension registration were unchanged.
The legacy ZIP SHA-256 was
`22d764cc89e29297915d5fe17ca28af31067d9fa9ab0733f7b43358482ac7040`.
Historical bundles are retained under `dist/history` and receipts under
`target/validation/continue-installer-*`.

## Limits

A successful source build must record the clean tagged commit and source hashes.
The [release helper](../../scripts/prepare-release.ps1) independently checks
all public payloads, sanitized metadata and checksums before publication.
See [RELEASING.md](../../docs/RELEASING.md) for the complete release process.

Actual PATH registry writes and environment broadcasts, reboot behavior,
restricted-directory permissions and genuine disk failures require a disposable
Windows VM and remain unverified locally. The scripts are unsigned. Hashes
identify inputs and bytes; timestamps and compiler/linker environments prevent
a general byte-for-byte rebuild guarantee. Resource limits cannot prevent driver
faults, so executable workloads remain on hosted runners for this release.
