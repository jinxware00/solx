# Reproducible benchmarks and generated source checks

The benchmark harness compares fixed programs using explicitly selected local
executables. It uses Node's standard library, runs each process sequentially,
verifies its checksum, and writes raw samples plus median and p95 wall times.
It never downloads or installs interpreters.

Build Solx in release mode, then pass the executable paths from the repository
root. For example, in PowerShell:

```powershell
cargo build --release --locked --bin solx
node scripts/benchmark.cjs --solx .\target\release\solx.exe --lua 'C:\path\to\lua.exe' --samples 7 --warmups 2 --build-profile release --build-command 'cargo build --release --locked --bin solx'
```

On other systems, pass the corresponding native paths, such as
`--solx ./target/release/solx --lua /usr/bin/lua`. Lua is optional; `--luajit PATH`
and `--luau PATH` also select installed executables. Every path is explicit;
the harness does not search for runtimes. LuaJIT uses the Lua fixtures. Luau uses
those fixtures except for the manifest's explicit `imports.luau` variant, whose
relative `require` path follows Luau's module-loading rules. That variant is
implemented but has no recorded local Luau validation; do not claim a Luau
comparison unless every selected fixture actually passes. The harness records
the result of each version probe;
if a Luau build does not implement `--version`, its version remains unknown
and its binary SHA-256 still identifies the tested executable.

`node scripts/benchmark.cjs --help` lists all options. `--out DIRECTORY`
selects a new output directory; existing directories are rejected to preserve
previous measurements. `--samples` accepts 1–100 and `--warmups` accepts 0–10.
`--timeout-ms` accepts 100–60,000 milliseconds and defaults to 10,000.
The build profile and build command are recorded labels supplied by the caller;
the harness does not build binaries or infer their compiler options.

## Workloads

The [manifest](../benchmarks/manifest.json) specifies loop counts, expected
checksums, and fixture paths. The paired sources perform the same operations
on values within the exact integer range shared by these runtimes.

| Case | Work | Expected output |
| --- | --- | --- |
| Startup | Start a process, load a one-line file, print, exit | `ready` |
| Arithmetic | 50,000 integer multiply/remainder/add iterations | `2400012` |
| Calls | 20,000 calls to a nonrecursive arithmetic function | `600050000` |
| Strings | 4,000 ASCII uppercase/conversion/concatenation/length iterations | `52000` |
| Collections | Append 5,000 values, then index them and update object fields | `243887` |
| Closures | Call one counter closure 10,000 times, mutating its captured binding | `50005000` |
| Control | 20,000 while-loop iterations, summing odd values through a conditional | `100000000` |
| Allocation | Create 10,000 two-element lists and wrapper objects, then read through each wrapper | `50015000` |
| Imports | Import/require a helper on each of 5,000 iterations and call its function | `12507500` |

ASCII string fixtures make Lua's byte length and Solx's Unicode scalar length
equivalent. Collections use Solx Lists/Objects and corresponding Lua tables;
their allocation, lookup, and garbage-collection costs remain part of each
runtime's measured behavior. Programs print only the checksum and stay within
Solx's default execution and call limits. The harness does not disable or raise
those limits. The imports case reads a local helper module once per process,
then exercises cached import resolution and calls; it is not a pure cache-lookup
microbenchmark. Its helper sources are recorded and hashed as support files.
Other cases perform no script-visible I/O beyond checksum output, and none
access the network. All cases still include the runtime's initial source reads.

The current manifest has nine cases. With `S` samples, `W` warmups, and `E`
engines, the workload portion runs `9 * (S + W) * E` processes, in addition to
version and repository probes. Seven samples and two warmups mean 81 workload
launches per engine. The October 25 baseline below covers all nine cases;
the historical October 23 table contains only the original five.

## Timing and interpretation

Every sample uses a new process. A monotonic Node clock measures launch through
exit, including runtime initialization, source reads and parsing/compilation,
program execution, checksum output, and shutdown. Startup is measured as its
own baseline. It is **not subtracted** from program times: subtracting noisy
process measurements would not reliably isolate execution time. Solx currently
has no common internal clock API for comparable timing inside all runtimes, so
the report's `internalWorkTiming` is `null`.

Warmup launches are recorded and excluded from summaries. They can warm OS
caches, but cannot preserve an interpreter or its JIT state between processes.
Engine order rotates for each round to reduce fixed-order bias. Median is the
middle sample, or the mean of the middle two. p95 uses the nearest-rank method:
`sorted[ceil(0.95 * N) - 1]`; with fewer than 20 samples it is the maximum.

For useful comparisons, build first, stop other builds/tests and heavy work,
and run all engines in the same session. Repeat on an otherwise idle machine
with more samples before drawing conclusions. These small workloads include
substantial process overhead and do not establish general language speed,
steady-state throughput, memory consumption, or startup-free execution speed.
No superiority claim follows from the presence of this harness.

For runtime launches and version probes, the harness clears `LUA_INIT`,
`LUA_PATH`, `LUA_CPATH`, and their version-suffixed variants. The receipt records
removed variable names without their potentially private values. The Lua import
fixture also sets its module search path to its own directory and disables C
module search, so ordinary `require` loads the hashed sibling fixture and then
uses its cache. Luau's variant uses a relative sibling path. Other environment
settings are inherited; record any additional configuration relevant to a
measurement. Matching compiler and interpreter versions alone do not establish
identical machines, libraries, environments, or timings.

On the Windows development host, run builds and benchmarks serially through the
[validation resource guard](../scripts/GUARDED_VALIDATION.md). Its process-tree
limits supplement the benchmark's per-process timeout and output limits. Do not
run desktop-host or deep-graph tests alongside measurement.

Each `results.json` includes:

- Raw warmup/measured trials, exact executable/argument arrays, elapsed
  nanoseconds and milliseconds, exit status, bounded output, and checksum result.
- Per-case/engine median, p95, minimum, maximum, and sample count.
- OS, architecture, CPU model/count, total RAM, Node/Rust/Cargo versions,
  interpreter version probes and binary hashes, and the invocation/build labels.
- Git commit and dirty status; SHA-256 hashes for Cargo files, every runtime
  Rust source, the harness, manifest, workloads, and helper support files. Dirty builds remain clearly
  identified rather than attributed to an unchanged commit.

The binary, runtime source, harness, manifest, and fixture hashes must remain
unchanged during measurement. A timeout, crash, unexpected stderr, incorrect
checksum, truncated output, or changed input marks the run `failed` and exits
nonzero while preserving its raw artifact. Output capture is bounded to 64 KiB
per pipe, retained diagnostic text to 4,096 characters, and every process has
a hard timeout. Only `status: "complete"` artifacts should be used as timings.
Match a release binary to its recorded sources by building immediately before
the run; source hashes cannot prove how an arbitrary supplied binary was built.
The harness atomically replaces `results.json` after each trial and workload,
outside the measured process interval. This preserves completed observations
if an external guard terminates the harness; `results.json.next` may contain an
unfinished next checkpoint. Final source, executable, and fixture checks must
still succeed before the artifact receives `status: "complete"`. A missing
result file or `status: "running"` is partial evidence, not a usable timing run.

## Development phase profiling

[examples/phase_profile.rs](../examples/phase_profile.rs) is an implemented
development-only runner for selected Solx phases. Build it from the same stable,
validated sources as the interpreter before recording results:

```text
cargo build --release --locked --example phase_profile -j 1
```

Run the resulting `target/release/examples/phase_profile` executable (`.exe` on
Windows), with the resource guard on the Windows development host. Its JSON
output contains 21 measured trials after two discarded warmups, raw nanosecond
samples, median, nearest-rank p95, and maximum for each phase:

| Phase | Included work | Excluded or unresolved |
| --- | --- | --- |
| `parse` | Parse a fixed program that sums 1 through 10,000 | File reads, process startup, interpreter creation, and later AST destruction |
| `preparsed_execution` | Run that parsed program, including AST validation, evaluation, scopes, and automatic collection inside the public run | Interpreter construction/destruction; this is not evaluator-only timing |
| `cyclic_graph_allocation` | Construct and register 2,000 independent self-cyclic list graphs, including the host loop and reference operations | Allocator call/byte counts, language-level allocation, and other graph shapes |
| `explicit_collection_pause` | One synchronous explicit collection of those unreachable graphs | Automatic GC pause distribution and collection latency for retained/shared graphs |

Each execution checks the sum `50005000`. Each explicit pass asserts exactly
2,000 reclaimed nodes and restoration of the pre-allocation retained-node count.
`graphs_per_sample` and `reclaimed_nodes` are graph/node counts, not allocation
counts or bytes freed. The fixed 21-sample median is the middle value; p95 is
the twentieth sorted sample. These timings are separate from the cross-runtime
process harness and do not populate its `internalWorkTiming` field.

The example emits phase results rather than a complete provenance record. Keep
the command, toolchain/build profile, Git revision and source state, example
source hash, executable hash, OS details, and resource-guard receipt with its
JSON output. Preserve the exact code and constants for repeated measurements;
changing graph size or source creates a different workload. The October 25
phase results below retain their provenance and all measured samples.

The Windows guard's `PeakJobMemoryBytes` is the OS **peak committed job memory**
counter. It is not process RSS, allocator-requested bytes, a measurement of
retained Solx heap, or system-wide peak memory. When one job runs the Node
harness and multiple runtimes, its peak also includes the harness and whichever
descendants were alive; it cannot be attributed to a particular engine or
phase. Inspect the configured cap and receipt status too: on this host the
counter also increased for an allocation rejected by the cap. The phase
runner's node counts do not convert that counter into a portable memory metric.

## Deterministic adversarial source coverage

Run the generated checks with:

```text
cargo test --test generated_sources
```

Four fixed xorshift seeds generate 384 valid programs and 384 programs with
known lexical or syntax faults. A further 512 truncation/Unicode mutation
cases exercise parser recovery and partial-input handling. The tests call the
real lexer, parser, and syntax-check pipeline, check agreement and valid UTF-8
diagnostic ranges, and catch unexpected Rust panics. Generation depth and source
length are bounded; generated code is never executed.

Eight additional cases combine thousands of nested containers or iterative
operator/member/call chains. These run through `solx check --json --stdin` in
separate processes, with a ten-second watchdog started before pipe I/O and
bounded output readers. A stack abort, hang, unexpected acceptance, or anything
other than the expected syntax-limit diagnostic fails the test. Fixed seeds
make failures reproducible without adding a fuzzing dependency; this coverage
does not replace sustained fuzzing or establish parser correctness for all input.

## Recorded local result

The valid [2026-10-23 release baseline](../benchmarks/baselines/2026-10-23-release-stable.json)
ran from 22:30:36 through 22:30:58 UTC on Windows 11 Home x64, build 26200,
using an Intel Core i5-1135G7 with eight logical processors. It used Node
24.19.0, Rust/Cargo 1.99.0, a release build of Solx 0.2.0, Lua 5.4.6, and
LuaJIT 2.1.1720049189. Luau was unavailable and was not measured.

Each engine/case had 21 measured launches and two warmups. All 345 launches
passed checksum/output validation. Runtime source, interpreter binary, harness,
manifest, and workload hashes remained unchanged during measurement. The
recorded Git commit is `4e2b0bb80d7ac59c15b027f6c2965e36f144556f` with dirty
runtime sources recorded by hash; the result identifies that working build,
not an unmodified commit. Development builds/tests were held during the run.

Values below are **median / p95 process wall time in milliseconds**:

| Case | Solx 0.2.0 release | Lua 5.4.6 | LuaJIT 2.1.1720049189 |
| --- | --- | --- | --- |
| Startup | 68.89 / 80.97 | 34.58 / 41.88 | 39.72 / 44.89 |
| Arithmetic | 154.83 / 180.67 | 35.63 / 41.29 | 38.32 / 46.29 |
| Calls | 130.22 / 153.39 | 41.53 / 45.28 | 42.70 / 48.04 |
| Strings | 91.01 / 108.59 | 38.99 / 46.95 | 40.54 / 53.22 |
| Collections | 90.05 / 102.17 | 37.41 / 44.48 | 39.14 / 45.45 |

Solx had higher full-process medians in every case. Startup overhead is a large
part of these short runs, so these data do not isolate interpreter execution
or establish a general Lua-versus-LuaJIT ranking. Raw samples retain outliers,
including one 208.68 ms LuaJIT string launch. Memory use, GC pause percentiles,
and longer-running JIT steady state were not measured. An exploratory attempt
was rejected when source hashes changed; only the valid final artifact is
archived. The committed baseline replaces absolute repository and user-profile
paths with `<repository>` and `<user-profile>`, including paths inside logs.
Its `pathRedaction` metadata records the rules and the unchanged raw artifact's
SHA-256. Timings and source/binary/fixture hashes are preserved. Raw local runs
under `benchmarks/results/` are ignored by Git; substitute local executable paths
for the baseline's placeholders when reproducing its invocation.

## Stable runtime baseline - 2026-10-25

The [process baseline](../benchmarks/baselines/2026-10-25-release-stable.json)
records commit `2d9f46d99aca946af60608c8b085de8dfbd78a8b`, with committed runtime
and benchmark sources. Separate installer, documentation and preserved asset
work was present; the full worktree status is retained. Runtime, binary, harness,
manifest and fixture hashes were unchanged. The run lasted from 17:02:01 to
17:02:58 UTC on the same Windows/i5-1135G7 host, using Rust/Cargo 1.99.0, Node
24.19.0, Lua 5.4.6 and LuaJIT 2.1.1720049189. Luau was not available.

All 621 launches passed output/checksum validation: nine cases, three engines,
21 measured launches and two warmups per case/engine. Builds and desktop tests
were stopped during measurement. The guard used a 512 MiB committed job cap,
180-second timeout, and at most 12 concurrent descendants. Its peak job counter
was 76,734,464 bytes, including the harness; this is not per-engine RSS. Raw
trials, source/binary hashes and the guard receipt are retained, with private
path prefixes redacted and the original local report's SHA-256 recorded.

Values are **median / p95 process wall time in milliseconds**:

| Case | Solx 0.2.0 release | Lua 5.4.6 | LuaJIT 2.1.1720049189 |
| --- | --- | --- | --- |
| Startup | 65.25 / 76.34 | 36.97 / 44.00 | 40.09 / 55.28 |
| Arithmetic | 143.78 / 164.84 | 35.62 / 46.19 | 38.78 / 49.98 |
| Calls | 128.88 / 238.67 | 39.83 / 65.42 | 42.88 / 50.75 |
| Strings | 89.80 / 111.32 | 39.59 / 56.36 | 42.38 / 60.84 |
| Collections | 99.99 / 131.12 | 48.53 / 53.72 | 49.99 / 60.14 |
| Closures | 128.38 / 173.28 | 43.56 / 102.90 | 44.77 / 86.36 |
| Control | 104.36 / 135.10 | 36.18 / 43.72 | 42.62 / 49.56 |
| Allocation | 115.22 / 146.35 | 40.06 / 44.35 | 39.19 / 46.61 |
| Imports | 579.28 / 732.93 | 40.59 / 68.93 | 47.02 / 51.26 |

Solx has higher medians in all nine cases. Imports have the largest measured
Solx time and gap, making path resolution, cache lookup and module-call scope
work useful next profiling targets. This combined workload does not determine
which component dominates. Startup remains included and is never subtracted;
these short process measurements do not isolate steady-state execution or JIT
behavior. The two dated runs are not a controlled optimization comparison.

The [phase record](../benchmarks/baselines/2026-10-25-phase-profile.json) retains
21 samples after two warmups from the same runtime revision, with example and
binary hashes, build receipt, machine/toolchain data and a separate 512 MiB
guard receipt. All execution sums matched and every explicit pass reclaimed
exactly 2,000 nodes, restoring the initial retained count.

| Phase | Median ms | p95 ms |
| --- | --- | --- |
| Parse sum-loop source | 0.033 | 0.047 |
| Preparsed execution | 16.698 | 18.996 |
| Construct 2,000 self-cyclic lists | 1.057 | 1.155 |
| Explicit collection of those lists | 1.197 | 1.736 |

Execution dominates parsing in this particular sum-loop fixture. Its timing
includes AST validation, scopes and automatic collection; it does not isolate
evaluator instructions. The explicit collection distribution covers only this
graph shape, not all automatic pauses or retained/shared graphs. Node counts
are not allocator byte measurements, and neither result establishes a general
memory or latency bound.

## Remaining measurement work

A read-only code audit identifies work to measure before changing the runtime:

- `Environment::begin_scope` allocates and registers a scope for every loop
  iteration and function call; callback-heavy programs exercise the same path.
  `call_function` also clones its captured Environment and creates a fresh
  binding map. Measure scope/binding allocations separately from evaluation.
- `Registry::register` creates traversal containers even when a value is already
  registered. `Node::children` creates temporary child vectors. These are
  candidate allocation costs; the current timings do not establish their share.
- Collection reconstructs node/edge maps and traces the full registered graph
  synchronously. The retained-node threshold reduces scan frequency but does
  not impose a pause bound. Existing `CollectionStats` counts nodes and passes,
  rather than bytes, allocation counts, or pause duration.
- Every executed program performs a collection, and interpreter destruction
  performs another. The process harness includes both; it cannot identify their
  contribution separately from source parsing, environment work, or OS startup.

The phase runner above supplies measured parsing, pre-parsed execution, isolated
cyclic allocation, and explicit collection timings alongside the expanded
process workloads. Broader allocation/scope counters and automatic collection
pause distributions remain unimplemented; add those only with clear counting
rules and bounded fixtures. Extend phase coverage to callback-heavy loops and
retained/shared graphs before generalizing from isolated self-cycles. Reuse the
existing cycle-retention example and ownership regression tests. Keep the
current external-root and borrow contracts, then optimize whichever measured
phase dominates. A faster registry fast path or scope representation would need
before/after evidence and full closure/module/GC checks; these observations do
not by themselves justify replacing the collector or declaring a bottleneck.
