# Desktop host follow-up (2026-10-24)

Starting repository HEAD: `abb9e87`. Existing, uncommitted extension 0.4.0
work was preserved, including Outline, name analysis, raw strings, collection
iteration, and library documentation. The package and lockfile already agree
on 0.4.0. Logo and experimental work was not changed.

## Reproduction and diagnosis

- Historical `host-H8V0nU/results.json`: cached Outline still contained three
  declarations after disabling symbols. Later source added provider
  re-registration to invalidate VS Code's document-version/provider cache.
- Historical `host-YI0JnF` and latest old `host-QWpoIt` instead failed the
  existing warm-check threshold (15,854 ms and 40,930 ms respectively), before
  reaching Outline. They did not establish whether re-registration worked.
- `npm.cmd run test:package -- 1.85.2` initially failed in Electron's CLI with
  sandbox `EPERM` during path resolution. The authorized, isolated-profile
  rerun installed the current VSIX and passed all preceding checks. It failed
  in `host-I1zZo1` because the disabled-symbol result was `undefined` and the
  test accessed `.length`. Warm requests took 6,094 / 1,165 / 1,123 ms.
- Inspection of the bundled VS Code 1.85.2 command adapter confirms that
  `vscode.executeDocumentSymbolProvider` converts an empty symbol array into
  `undefined`. This is a valid empty result, not evidence of stale symbols.

The host regression now waits for the actual configuration event, accepts the
command's two empty representations, checks repeated emptiness after disable,
then requires all names and source selections to return after re-enable
without changing the document version. A unit regression verifies replacement
provider identities, obsolete registration disposal, local cache invalidation,
disabled launch suppression, and executable configuration refresh. The prior
provider re-registration fix remains in place.

## Current validation

The first corrected package passed 73 unit tests, all 8 real-interpreter
integration tests, packaging, and all 18 VS Code 1.85.2 host checks in
`host-F9mbCT`, with no temporary snapshots. Warm checks were 6,668 / 1,149 /
1,147 ms.

The additional VS Code 1.143.1 run (`host-HDsGp0`) exposed an initial empty
Outline result, before the disable/enable test. A source trace in
`host-eFNNZh` showed overlapping requests for the same document/version:
the second request killed the first request's checker. That trace run also
hit the unchanged ten-second warm-check threshold (11,227 ms); explicit
commands were waiting behind all queued background documents.

The provider now shares one checker per document/version/configuration,
tracks each consumer's cancellation separately, and stops work when all
consumers cancel. It also refreshes on workspace-trust grant. Explicit
syntax checks run ahead of queued background checks while preserving the
active check. Four new regressions cover shared results, individual/all
cancellation, trust refresh, and queue priority. All 77 unit tests pass.
Temporary tracing was removed. The harness also compares the complete
installed manifest, excluding only VS Code's added installation metadata.

The next 1.143.1 run (`host-vBaXdx`) passed Outline and the warm threshold
(2,292 / 2,604 / 2,666 ms), then timed out in the test's configuration helper
while VS Code was still saving settings. The event deadline now starts after
the awaited settings update, rather than counting file-save participants as
event-delivery delay. The setting still must produce its real change event.

At this point in the earlier session, both desktop versions still required
validation after these fixes. The final results appear below.
No normal profile or installed interpreter was modified; no publication or
push occurred. Test artifacts remain ignored under `.test-artifacts/` and
are not committed.

## Resumed source review

The resumed session reviewed shared Outline jobs, per-consumer cancellation,
provider replacement, shutdown cleanup, and manual-check queue priority without
launching another desktop process. Existing unit regressions cover those paths;
the review did not establish a new production defect.

The host harness now retains at most two million characters of editor output,
accepts `SOLX_HOST_TIMEOUT_MS` from 1,000 to 600,000 (default 600,000), and treats
any timeout as failure even if a success report was already written. On Unix,
the isolated editor uses a process group so timeout cleanup targets its
descendants; Windows continues to target only the launched editor's process
tree, with a ten-second timeout for the termination command. These controls do
not impose a memory bound on Electron, Cargo, or their descendants. Run this
harness inside the coordinating session's process-tree memory/time limits.

Required serial validation after source changes settle:

1. `node --test --test-concurrency=1 test/*.test.cjs`.
2. `npm.cmd run test:interpreter` (rebuilds the CLI in the isolated target).
3. `npm.cmd run package` (also runs unit tests during prepublish).
4. `npm.cmd run test:package -- 1.85.2` (the official build is cached).
5. `npm.cmd run test:package` against the locally discovered second VS Code
   installation; confirm its actual version in `results.json`.

Host runs create separate `host-*` profiles and preserve `results.json`,
`host.log`, `install.log`, and packaged-byte verification in `package.json`.
Only a successful packaged run writes `package-validation.json`; its SHA-256
must match the final VSIX. Prior receipts remain historical until the final
source/package checks succeed. No validation was run as part of this source
review.

## Guarded desktop validation

The final packaged bytes passed all 18 checks on VS Code 1.85.2 in
`host-QgpObQ`, including unchanged-buffer Outline disable/re-enable and warm
syntax checks at 1,608 / 1,581 / 1,800 ms. The guard receipt
`target/validation/resume-host-1852/result.json` records exit 0 and a peak job
memory counter of 721,915,904 bytes under a 1,536 MiB configured cap.

The first guarded VS Code 1.143.1 run, `host-c1RoZn`, failed after three checks
during negative completion/hover contexts. This was a renderer memory failure,
not an Outline or completion assertion: `main.log` at 19:50:09.557 UTC records
`renderer process gone (reason: oom, code: -536870904)`, immediately followed by
the test window exiting with code 1. The extension host then shut down normally
with code 0; the test's `Canceled` exception came from that teardown. The Node
harness exited 1 after approximately 11 seconds, without reaching its timeout.
Its guard receipt records peak job memory 1,610,846,208 bytes against the
1,610,612,736-byte cap; the peak counter can include rejected allocation attempts.

The modern editor started built-in agent/chat and Git services despite
`--disable-extensions`. The harness now seeds only its newly created test
profile with public settings disabling AI features, Git/background fetch,
telemetry, and updates. It changes no Solx setting, assertion, or normal editor
profile. The VSIX is unchanged. The initial 1.143.1 failure is retained as
evidence; the separate successful retry is recorded below.

## Final receipt review (2026-10-25)

The serial retry completed successfully on 2026-10-24 at 21:28:17 UTC, just
after the earlier chat was interrupted. `host-z2sCHN/results.json` records
all 18 checks passing on VS Code 1.143.1, including Outline disable/re-enable
on the unchanged document. Its three warm checks took 1,593 / 1,629 / 1,562 ms.
No Solx temporary snapshots remained.

`target/validation/resume-host-current-bounded/result.json` confirms exit 0
in 31,751 ms, a peak job memory counter of 1,387,900,928 bytes under the
2,147,483,648-byte configured cap, and zero remaining child processes when
the selected command exited. This was a fresh disposable editor profile;
the normal profile and normal Solx installation were not used.

The following existing results were read and verified during recovery; no
tests, builds, or desktop launches were repeated as part of this review:

| Validation | Passed | Receipt under `target/validation/` |
| --- | --- | --- |
| Extension unit tests | 77 | `resume-extension-unit/result.json` |
| Real-interpreter integration tests | 8 | `resume-extension-interpreter/result.json` |
| VSIX packaging, including prepublish unit tests | Yes | `resume-vsix-build/result.json` |
| Packaged VS Code 1.85.2 | 18 | `resume-host-1852/result.json` |
| Packaged VS Code 1.143.1 | 18 | `resume-host-current-bounded/result.json` |

Both successful desktop runs tested the same extension 0.4.0 VSIX, with
113,951 bytes and SHA-256
`5bd6c016cb9ae88a8f8e13a6cd83573695c4d049d95d22c90714c40a32e375ab`.
The archive's current hash matches both per-host `package.json` records and
`.test-artifacts/package-validation.json`. Read-only comparisons on 2026-10-25
also confirmed that all 16 shipped content files match the current checkout
byte for byte, and the complete installed manifest matches after removing
VS Code's installation metadata. The 18 installed files include that manifest
and VS Code's generated `.vsixmanifest`.

Source review covered shared Outline jobs and independent cancellation,
provider identity/cache refresh, diagnostic queue priority, snapshot cleanup,
raw-string/iteration support, and package verification. It found no additional
production defect requiring a source change. These receipts establish the
tested Windows desktop behavior only; Linux/macOS desktop and remote hosts
remain unverified. No Marketplace publication, account change, normal-profile
installation, or push occurred.
