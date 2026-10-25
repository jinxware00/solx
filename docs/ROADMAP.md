# Development roadmap

This tracks implemented milestones separately from unfinished work. The
specification and tests define supported behavior; a roadmap entry is not a
feature promise or a release announcement.

## Completed source milestones

- Windows extension checker startup workaround and normal-shutdown cleanup
  (extension 0.3.1, locally packaged and desktop tested).
- CI definition for Windows, Linux and macOS, plus installer source/VSIX
  consistency, provenance and archive preservation. Local fixture tests pass;
  the remote workflow has not run yet.
- Storable nil with lexical scope preservation and migration guidance.
- Exact i64 integers, checked arithmetic and exact mixed numeric comparisons.
- Bounded JSON parsing/serialization with deterministic keys and Unicode support.
- Collection iteration with snapshot membership, deterministic object ordering
  and fresh closure bindings.
- Optional monotonic execution deadlines and bounded runtime file/REPL source.
- Raw multiline strings, bounded literal replacement, UTF-8 byte length and
  Unicode scalar conversion helpers.
- Snapshot callback list map/filter/fold and stable custom sorting with shared
  execution budgets and normal function/error semantics.
- Conservative opt-in AST name checking and exact declaration symbols, retaining
  default syntax-only checks and the UTF-8 JSON protocol.
- Iterative destruction of runtime lists, objects, closures, scopes and modules,
  including shared roots, cycle collection and error/cancellation cleanup.
- Byte/work budgets for language formatting, with cancellation and deadline
  checkpoints and complete-line validation before print output.
- Deterministic generated-source tests and reproducible local benchmark tooling.
- Extension 0.4.0 support for current runtime APIs, raw strings, iteration,
  optional name diagnostics and Outline symbols. The matching VSIX passed 18
  checks on both VS Code 1.85.2 and 1.143.1, including configuration refresh and
  overlapping symbol requests; see the editor's HOST_VALIDATION.md.

## Validated local artifacts

- The 0.4.0 VSIX passed 77 unit tests, 8 real-interpreter integrations and 18
  packaged checks on each of VS Code 1.85.2 and 1.143.1.
- The 0.2.0 Windows installer bundles that VSIX and the validated runtime. Its
  final archive, built from committed inputs, passed all 18 isolated extracted
  scenarios plus 32 helper/file/build checks. No normal installation was changed.
- CI passes local lint and its Windows checks are validated; remote Linux/macOS
  jobs have not run. Final hashes and exact receipts are in
  [OVERNIGHT_PROGRESS.md](OVERNIGHT_PROGRESS.md). Earlier validation records
  apply only to the artifacts and sources they name.
- Nine workloads now have a stable-runtime baseline against Lua and LuaJIT,
  plus explicit GC phase samples; see [BENCHMARKS.md](BENCHMARKS.md). Solx is
  slower on these measured workloads. Luau and portable memory measurements
  remain unavailable, and no general speedup claim is supported.

## Remaining development

- Broader native-operation/allocation/output budgets; measure automatic GC
  pauses and retained process memory. Iterative runtime teardown is implemented
  but remains synchronous and requires temporary worklist storage.
- Explicit interpolation and its escaping rules; grapheme APIs if needed.
- Broader name/signature analysis, expression ranges, multiple error recovery,
  structured error causes/rethrow and deterministic resource cleanup.
- Function/return/optional annotations and explicit multiple-result rules.
- Capability-based file/path/environment/argument/time APIs and isolated seeded
  randomness; networking/process access needs a clear host access policy.
- Language-server features, navigation/rename/signature help,
  comment-preserving formatting, indentation/folding, and a debugger.
- Persistent checker transport and investigation of the Windows stdin stall;
  non-Windows/remote editor validation.
- Explicit VM execution stack/bytecode and tail calls, measured against the
  existing interpreter and equivalent Lua/LuaJIT/Luau workloads.
- Project manifest/lockfile/local package tooling, then offline verified caches
  and resolution. A registry and signing/trust design follow stable local tools.
  Start with bounded local manifest/entry validation before adding dependency
  resolution; this remains a proposed milestone, not an implemented command.
- Real fresh-Windows-VM PATH/upgrade/failure testing, release signing,
  Marketplace identity/ownership/rendering verification and publication.

Coroutines/async, classes, pattern matching, deep immutability, metaprogramming,
native interoperability and a JIT remain conditional on concrete use cases and
measurements. Publication and normal-profile installation are not part of the
isolated build/test workflow. Validation records must distinguish a local
automated test, a desktop host run, a VM test and an actual public release.
