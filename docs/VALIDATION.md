# Release validation

The release pairs runtime **0.2.0** with **Sol Language Support**,
`jinxware00.solx-ls` **0.1.0**. Test results below distinguish the current package
from earlier artifacts. Generated packages, profiles and logs remain ignored.

## Published v0.2.0 - 2026-10-25

The [v0.2.0 development release](https://github.com/jinxware00/solx/releases/tag/v0.2.0)
was published from commit `b438b67946d9c13e4ca6228e422a249d4b7e5d82`.
The [release workflow](https://github.com/jinxware00/solx/actions/runs/40222012232)
and [CI](https://github.com/jinxware00/solx/actions/runs/40222012114) passed.
Its four assets are the Windows ZIP, sanitized provenance JSON,
`solx-ls-0.1.0.vsix`, and `SHA256SUMS`.

The published VSIX is 113,271 bytes, SHA-256
`b84c17085de79a761866e38fba4f0bbca82ca63f4c23d8a70d61bf899e9f051e`.
It is distinct from the historical local package below. The `fs`, `path` and
`time` source additions are after this release; these release results do not
validate or distribute those additions.

## Prerelease hosted validation - 2026-10-25

[CI run 40221522191](https://github.com/jinxware00/solx/actions/runs/40221522191)
passed on **Windows 2022, Ubuntu 24.04 and macOS 14** for commit `4073713`.
All three jobs passed Rust formatting, tests and strict Clippy, extension units,
real-interpreter integration and packaging. Windows additionally passed all
18 packaged checks on each of VS Code 1.85.2 and 1.143.1, 11 resource-guard
assertions, 34 installer helper/file/build checks, 14 release-preparation checks,
the clean bundle build and all 18 extracted installation/uninstallation cases.

Hosted validation corrected a platform-dependent floating-point test boundary,
PowerShell exit-code propagation and npm lockfile parsing, Windows ZIP entry
separators, and redirected installer input with a byte-order marker. The runtime
implementation was unchanged. The later tagged release passed its workflow
and artifact verification as recorded above.

## Historical local evidence - 2026-10-25

| Check | Result | Evidence |
| --- | --- | --- |
| Extension packaging and unit tests | 77 passed; new VSIX created | `target/validation/release-ls-package/` |
| Packaged VS Code 1.85.2 | 18 passed; zero snapshots remaining | `target/validation/release-ls-host-1852/` |
| Packaged VS Code 1.143.1 | Interrupted; no completed result | Fresh check required in CI |
| Renamed installer and release fixtures | Passed on hosted Windows | CI run linked above |
| Final Windows bundle and extracted installation tests | Passed on hosted Windows | CI run linked above |

The new VSIX is `vscode-solx/solx-ls-0.1.0.vsix`, 113,047 bytes, SHA-256
`1e1b5e107621b7a05b3ded0bb0bfcce43d49f6a89b60595ad86bfd800e307460`.
See [editor validation](../vscode-solx/HOST_VALIDATION.md) for coverage and
[installer validation](../installer/windows/VALIDATION.md) for installer cases.

The local desktop attempt was interrupted by a host restart. Further executable
workloads were moved to GitHub runners. A new release must pass the full
[CI workflow](CI.md); local historical receipts do not substitute for those
checks. Publication requires Windows, Linux and macOS jobs, both pinned Windows
VS Code versions, and the extracted installer scenarios to succeed.

## Historical source and artifact evidence

The runtime source is unchanged by the extension rename and release setup.
On 2026-10-24 it passed **421 Rust tests**, formatting and strict Clippy.
The earlier extension identity passed **77 units**, **8 interpreter integrations**
and **18 packaged checks** on each of VS Code 1.85.2 and 1.143.1.
The previous Windows bundle passed **18 extracted installation/uninstallation
scenarios**, **32 helper/file/build checks**, and **11 resource-guard probes**.
Historical receipts remain under `target/validation/resume-*` and
`target/validation/continue-*`; earlier bundles are local build history.

Those extension and installer results apply to legacy extension 0.4.0. The
renamed package and updated installer require their own checks. Benchmark
results retain 621 validated launches and 21 phase samples; see
[BENCHMARKS.md](BENCHMARKS.md) for methodology and limits.

## Release boundary and limitations

[Release preparation](RELEASING.md) documents the four allowed public assets,
commit/tag checks, exact source and payload hashes, sanitized provenance and
independent download verification. No private notes, experimental work, logs,
profiles, credentials or generated binaries belong in Git history. The local
backup branch is not a release input.

Windows PATH registry writes and broadcasts, reboot behavior, permissions and
real disk failures require a disposable Windows VM. Installer scripts are
unsigned. Linux/macOS desktop and remote editor hosts, Marketplace publisher
ownership and Marketplace rendering are not established by these checks.
Actual remote results and the published assets determine release status;
adding a workflow or producing a local package does not establish publication.
