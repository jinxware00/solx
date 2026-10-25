# Preparing and publishing releases

Solx runtime and editor versions are independent. The current release pairs
runtime **0.2.0** with **Sol Language Support**, extension identifier
`jinxware00.solx-ls`, version **0.1.0**. Runtime tags use `vMAJOR.MINOR.PATCH`,
so this release uses `v0.2.0`. A tag must already exist and resolve to the exact
commit whose sources are tested. Existing release assets are never overwritten
by the workflow.

## Published assets

| Asset | Contents |
| --- | --- |
| `solx-ls-0.1.0.vsix` | Standalone VS Code extension |
| `solx-0.2.0-windows-x64.zip` | Runtime, terminal installer, and the same VSIX |
| `solx-0.2.0-windows-x64.zip.json` | Public, sanitized build and validation provenance |
| `SHA256SUMS` | SHA-256 for the three files above |

GitHub additionally provides its automatic source-code archives. The workflow
does not publish to the VS Code Marketplace, sign binaries, change publisher
accounts, or install into a developer's normal profiles. See
[installation instructions](INSTALLING.md) for the ZIP and VSIX.

## Automated release

After committing and reviewing the release sources, push the intended runtime
tag. Alternatively, run **Release** from the Actions page with the name of an
existing tag. Manual dispatch is useful when a tag was created before the
workflow was available; it still validates the tagged commit, not the current
default-branch source.

[release.yml](../.github/workflows/release.yml) resolves the tag once to a commit
and calls the existing [CI workflow](../.github/workflows/ci.yml) with that exact
commit. CI runs Rust formatting, tests and strict Clippy on all three operating
systems; extension unit/integration/package checks; and the Windows packaged
desktop hosts (VS Code 1.85.2 and 1.143.1), installer fixtures, release-preparation fixtures, complete bundle
build and isolated terminal tests. No publishing job starts unless every CI
matrix job succeeds.

The Windows job prepares only the four public files and uploads one
`release-assets` artifact. The publishing job downloads those files directly
into `dist/release-assets`, verifies them again against a fresh checkout of the
same tagged source, and checks that the remote tag still resolves to that commit.
It creates a draft with all four assets, then publishes that draft. Only this
job has `contents: write`; validation jobs have read-only repository access.
The GitHub token is supplied only to the publishing step and is never included
in notes or provenance.

If creation or upload fails, inspect any retained draft before retrying. A retry
does not replace an existing draft or published release. Resolve the specific
failure and choose whether to finish that draft or use a new version; do not
move a published tag to different source. Workflow artifacts are retained for
14 days. An Actions outage, unavailable Rust/VS Code download, or a failed test
leaves the release unpublished.

## Local preparation and verification

The [preparation helper](../scripts/prepare-release.ps1) only validates and stages
files. It never builds, installs, accesses the network, or publishes. Finish the
same locally runnable checks described in [CI.md](CI.md), build the current VSIX,
run its packaged desktop test, build the installer and run the complete isolated
terminal tests. Commit the packaged and release-automation inputs before the
final bundle build so its provenance names the intended clean source commit.
On the Windows development host, keep those builds/tests serial inside the
[resource guard](../scripts/GUARDED_VALIDATION.md).

If local desktop or build execution is unavailable, push the reviewed source
and tag for hosted validation. CI creates the final bundle from that commit;
do not reuse a stale local installer or bypass the release checks. Local
preparation is optional when the complete hosted workflow is used.

With the intended tag already pointing to that commit, run from the repository
root in Windows PowerShell 5.1:

```powershell
$releaseCommit = git rev-parse HEAD
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-release.ps1 -Tag v0.2.0 -ExpectedCommit $releaseCommit
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-release.ps1 -Tag v0.2.0 -ExpectedCommit $releaseCommit -VerifyOnly
```

The helper writes a new `dist/release-assets` directory and refuses to overwrite
an existing one. Use `-AssetDirectory <new-directory>` to preserve a previous
candidate. It requires:

- Matching tag, checkout HEAD, Cargo and extension lockfile versions, build
  provenance commit, and extension identity.
- Tracked, unchanged packaged and automation inputs; unrelated local assets or
  experiments do not authorize changes to any packaged file.
- Exact source hashes, a current source-matching VSIX, successful packaged-host
  receipt, matching standalone/bundled VSIX, and every expected ZIP entry/hash.
- Exactly four public files with matching checksums, explicit public metadata
  fields, and no unknown fields or raw environment values.

The original installer provenance and archive remain untouched. Public
provenance contains relative source/payload paths, hashes, versions, validation
counts and structured toolchain versions. Arbitrary logs, absolute paths and raw
Rust/environment flags are omitted; boolean indicators note whether build flags
were configured. The original provenance SHA-256 links the sanitized record to
its local input. This is a record of tested inputs and bytes, not a guarantee of
byte-identical rebuilds or publisher authenticity.

`-VerifyOnly` works with a clean checkout and the four downloaded assets; it
does not require the original build caches or local host receipt. It is used by
the publishing job. Do not upload the unsanitized JSON beside the original ZIP
or a wildcard over `dist`; upload only the four files inside the verified asset
directory.

For an explicitly authorized manual publication when Actions is unavailable,
complete the local validation above, review its recorded platform limitations,
verify the remote tag points to the same commit, and use GitHub's release UI or
`gh release create --verify-tag` with those four explicit paths. Stage a draft
and verify all uploads before publishing. Local Windows checks do not substitute
for successful Linux/macOS CI, and any unavailable platform check must be stated
in the release notes. A manual release does not establish that Actions passed.

## Testing the preparation boundary

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-prepare-release.ps1
```

The fixture script uses a temporary local Git repository, a fake compiler result,
and small ZIP/VSIX files. It tests successful staging and independent verification,
privacy and input preservation, wrong tags/commits, dirty/untracked/stale inputs,
extra ZIP/metadata entries, and tampered downloads/checksums. It does not compile
Solx, install anything, touch normal profiles, or contact a remote repository.

Fresh-Windows-VM PATH changes, operating-system failure/reboot behavior, code
signing, and Linux/macOS desktop-host runs remain separate release checks. Their
absence must not be described as successful validation.
