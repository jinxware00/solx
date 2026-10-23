# Continuous integration and release provenance

[The CI workflow](../.github/workflows/ci.yml) runs on pushes, pull requests and
manual dispatch. It has read-only repository permissions and does not publish
releases or install into a developer's normal Solx or VS Code directories.

| Runner | Checks |
| --- | --- |
| Ubuntu 24.04, Windows 2022, macOS 14 | Rust formatting, locked Cargo tests, Clippy with warnings denied; extension unit tests, real interpreter integration tests, VSIX packaging |
| Windows 2022 | Packaged VS Code 1.85.2 Extension Host checks; installer helper, file/PATH and build regression checks; eight isolated terminal installation/uninstallation cases |

The workflow pins Rust 1.99.0, uses Node.js 24, and installs extension dependencies
with `npm ci`. The interpreter integration tests use Cargo offline after the
Rust checks have fetched the locked dependencies. VS Code 1.85.2 is the supported
baseline desktop build; the host harness downloads it into its test directory.
The Windows terminal harness builds the release binary and uses separate install,
extension and user-data directories with PATH changes disabled.

Only official GitHub Actions are used. Their commit IDs were verified against
the upstream tags with `git ls-remote`:

| Action | Tag | Pinned commit |
| --- | --- | --- |
| `actions/checkout` | `v4.2.2` | `11bd71901bbe5b1630ceea73d27597364c9af683` |
| `actions/setup-node` | `v4.4.0` | `49933ea5288caeca8642d1e84afbd3f7d6820020` |
| `actions/upload-artifact` | `v4.6.2` | `ea165f8d65b6e75b540449e92b4886f43607fa02` |

Windows validation reports are retained as workflow artifacts for 14 days,
including failed host/installer logs when available. The ZIP and its provenance
record are uploaded only after all Windows checks succeed. These are CI
artifacts, not a GitHub Release or Marketplace publication. Linux/macOS desktop
Extension Host tests and real Windows user-PATH changes remain outside this
workflow. Adding a workflow does not establish that a remote run has passed;
consult the actual Actions run before claiming platform coverage.

## Installer build validation

`installer/windows/build.ps1` requires a successful packaged host receipt whose
SHA-256 matches the selected VSIX. It also independently compares every packaged
extension file with current source, including all of `package.json`, recursive
source directories, documentation and binary assets. A receipt from an earlier
build cannot authorize an outdated VSIX after source changes, even when the
extension version is unchanged. Unexpected, missing and duplicate ZIP entries,
and a mismatched VSIX manifest identity, are rejected.

The file allowlist comes from `vscode-solx/package.json`. Literal files and
directory patterns ending in `/**` are supported; a new glob form requires an
explicit update to the validator. VSCE's README, changelog and license filename
normalization is accounted for. Packaging must preserve source bytes, as the
current `npm run package` command does.

From the repository root on Windows PowerShell 5.1:

```powershell
# Check the existing VSIX and receipt without compiling or creating a release.
powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/build.ps1 -ValidateOnly

# Isolated regression fixtures; no real compiler, installation or PATH changes.
powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/test-build.ps1
```

The full build takes hashes of runtime sources and manifests, extension package
files and lockfile, extension validation scripts/tests, installer scripts,
installation instructions, license and packaged PNG. It checks that these inputs
and the VSIX are unchanged after compilation and copying. The resulting
`dist/solx-<version>-windows-x64.zip.json` uses provenance schema version 2 and
records:

- Interpreter/extension versions, Git commit and separate local-change flags for
  runtime, extension and installer inputs. Without Git metadata, these fields
  are `null`.
- Rust, Cargo, Node and npm versions, target, UTC build time and relevant Rust
  flag environment variables.
- SHA-256 for recorded source files, every ZIP payload file, the VSIX and ZIP,
  together with the packaged host validation receipt.

The installed-file checksums inside `bundle.json` retain their existing format;
the external provenance additionally covers terminal entry scripts, the VS Code
helper, branding and `bundle.json` itself. Before replacing a ZIP's conventional
filename, the build copies and verifies the previous ZIP and its provenance in a
unique `dist/history/<id>/` directory. Existing historical copies are retained.

This records the build's inputs and detects stale content; it does not promise
byte-identical ZIPs between machines or runs. ZIP timestamps, compiler/linker
environment and toolchain versions can affect output. Hashes detect corruption
and mismatches, not publisher authenticity. Use a clean checkout and review the
provenance before distributing a release. Existing signing and fresh-machine
release checks still apply.
