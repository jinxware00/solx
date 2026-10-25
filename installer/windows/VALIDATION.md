# Windows installer validation

This record supplements the historical repository validation documents. Tests
use ignored `.artifacts` directories and never select a normal installation or
VS Code profile. Actual user PATH writes are disabled.

## Completed milestone - 2026-10-25

The current runtime (0.2.0) and extension (0.4.0) bundle passed all **18**
extracted installation/uninstallation scenarios in
`.artifacts/terminal-877f07fe4dc14cfd8f514d60275a0ca8/results.json`.
This includes extension selected/skipped, CLI failures, decline/quit/EOF,
upgrade backups, rollback, damaged payloads, unsafe uninstall metadata and
preservation of user files and the separately installed extension.

Fresh bounded checks also passed: 13 CLI helpers (including inherited pipes),
6 file/PATH helpers, 13 build/provenance fixtures, 11 resource-guard probes, and
Actionlint 1.7.7. Receipts are under `target/validation/continue-installer-*`,
`continue-actionlint`, and `guard-probes-497d00aa109049339ba92abe11abe7ea`.
The successful full run used a 1 GiB job cap, completed in 58.890 seconds,
recorded 232,583,168 peak committed job bytes, and left zero child processes.
The normal interpreter, user PATH, VS Code settings and extension registration
were verified unchanged.

The reviewed source build recorded commit `2d9f46d`, clean runtime/extension
inputs, and the then-uncommitted installer changes by hash. It contains 77
source hashes and 10 payload hashes. Its ZIP SHA-256 is
`22D764CC89E29297915D5FE17CA28AF31067D9FA9AB0733F7B43358482AC7040`;
the bundled VSIX SHA-256 is
`5BD6C016CB9AE88A8F8E13A6CD83573695C4D049D95D22C90714C40A32E375AB`.
Subsequent builds preserve these bytes under `dist/history`; final artifact
identity and source commit are recorded in
[the progress record](../../docs/OVERNIGHT_PROGRESS.md).

Two failed attempts are retained: `continue-installer-terminal` supplied a
relative CodeRoot, rejected before installation; this exposed a bare trap
`throw` that hid the error as ScriptHalted. The trap now preserves the original
ErrorRecord. `continue-installer-terminal-reviewed` passed provenance and the
skip path but Electron hit sandbox EPERM on home-path resolution. The approved
retry outside the command sandbox passed, retaining the same job cap and
disposable paths. These were invocation/environment failures, not successful
installer validations. CodeRoot must be an absolute path.

## 2026-10-24 continuation

Verified source versions: Solx 0.2.0, extension 0.4.0. At inspection, `dist`
contained only the historical Solx 0.1.0 archive; it is retained. The existing
CI workflow already covered Rust, extension and installer checks.

Fresh checks before the final package build:

| Command/check | Actual result |
| --- | --- |
| `powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/test.ps1` | 12 passed |
| `powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/test-files.ps1` | 6 passed |
| `powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/test-build.ps1` | 13 passed |
| PowerShell parser, `test-install.ps1` | No syntax errors |
| `actionlint .github/workflows/ci.yml` | Passed |

The extracted-bundle harness now verifies external provenance and all extracted
hashes, exact runtime/extension versions, the successful VSIX receipt, and source
bytes before running the scripts. It adds upgrade/backup preservation, actual
destination-conflict rollback, damaged-bundle rejection, all three prompt
cancellation forms, uninstall cancellation and unsafe-metadata rejection.
Runtime smoke checks require exact integers, storable nil and JSON support.
Results are persisted after each scenario; failures retain their logs.

The final current-source build and expanded extracted tests are pending runtime
and packaged extension validation. This record must be updated with actual
archive hashes and results after that run.

### Source review after the host interruption

The resumed review found two gaps in the test harness: direct interpreter smoke
commands had no process timeout, and exceptional terminal exits could skip
process-tree cleanup. All smoke commands now use a 30-second watchdog (plus a
five-second Solx execution deadline for the semantic check), terminal scenarios
retain their 180-second watchdog, and cleanup runs in `finally`. Output drain
waits are bounded too. These are harness protections, not a host memory cap.

No tests or builds were run during this source-only review. The coordinator must
run the three helper/file/build scripts, then build the bundle after the final
packaged-host receipt exists, then run `test-install.ps1 -CodeRoot <isolated
VS Code root> -SkipBuild`. The complete harness expects **18** successful
scenario records in `.artifacts/terminal-*/results.json`; a failure writes a
false record and retains logs. Use `CARGO_BUILD_JOBS=1` and an external process
tree memory/time limit, with no simultaneous stress or desktop validation.

### 2026-10-25 source review

The production VS Code CLI helper had a related deadline gap: after its parent
process exited, an inherited stdout/stderr pipe could keep `ReadToEndAsync`
pending indefinitely. Both output drains now share the existing command
deadline. A fixture starts a short-lived descendant that inherits those pipes;
the regression requires a useful timeout result before the descendant exits,
then cleans up that exact fixture using its PID and creation time. The helper
suite now contains 13 checks; its successful execution is recorded above.
PowerShell parsed both modified files without errors. The earlier 12-check
receipt remains historical evidence, not validation of the new helper bytes.

## Remaining environment-specific checks

No disposable Windows VM is currently available (`Get-VM`, `VBoxManage` and
`vmrun` were not available commands during discovery). Actual PATH registry writes
and environment broadcasts, reboot behavior, restricted-directory permissions,
and interruptions during genuine disk failure remain unverified. Helper tests
cover PATH text transformations; they are not evidence for operating-system
integration. The scripts remain unsigned. Windows checks do not establish Linux
or macOS desktop support. Archive hashes identify bytes and detect corruption;
build timestamps and compiler/linker inputs prevent a byte-for-byte reproducible
ZIP guarantee across runs or machines.

No push, publication, account changes, normal-profile extension installation,
or normal Solx installation is part of these checks.
