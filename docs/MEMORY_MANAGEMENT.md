# Memory management

Solx now collects unreachable cycles involving lists, objects, closures,
captured scopes, and module exports. Live values retain their identity and
contents. This fixes the leaks demonstrated by the earlier
[runtime audit](RUNTIME_AUDIT.md); it does not make every memory/resource
failure recoverable.

## Ownership contract

The existing `Value` variants and their `Rc` payload types are preserved. Ordinary
acyclic values still release when their last strong owner disappears. Their
destruction uses an explicit worklist, including closure environments and module
exports, instead of recursively dropping a value graph. Solx
allocations register weak references with a thread-local collector; the
registry itself does not keep payloads alive. There is no unsafe Rust, new
dependency, weak closure capture, moving heap, finalizer, or syntax change.

These are roots and retain everything they can reach:

- Active interpreter environments and successful module caches.
- Environments temporarily saved during calls/imports, argument and expression
  temporaries, and returned values being unwound through blocks.
- Rust-owned Values, Rc clones, and Environment clones, including values
  retained after their original Interpreter has been dropped.
- Containers whose RefCell is currently borrowed; they are retained
  conservatively until a later collection can inspect them.

Weak references are observations, not roots. Dropping an Interpreter releases
only that interpreter's owners; it does not clear any reachable captured scope.
Successful module caches remain intentional roots until their Interpreter is
dropped. Failed module initialization can leave live exported closures in a
successful dependency, and those closures must remain usable.

The `Rc` graph remains single-threaded. Collection on one thread considers all
registered graphs on that thread, including other live interpreters. It does
not operate on another thread's state.

## Iterative destruction (2026-10-24)

`Value::drop` checks whether it is the sole strong owner of a collection,
function, or module. If so, it creates a worklist owner and replaces itself with
`Nil`. The replacement invokes `Drop` once more, but that invocation sees the
additional owner and immediately returns. The maximum nesting caused by this
transfer is one extra call, independent of graph depth. No unsafe reads,
placeholder allocations, thread-local destruction queue, or leaked owners are
used.

The worklist consumes nodes with `Rc::try_unwrap`. A uniquely owned container
transfers each outgoing graph edge to the worklist before dropping the original
child Value. Functions transfer their captured scope owners; modules transfer
their export-map owner. Shared nodes are only decremented and retain their
identity and contents. If several pending edges own the same node, its last
pending owner releases it. Unreachable cycles still require the existing
collector. Weak registrations never prevent `try_unwrap` from succeeding.

Owned `RefCell::into_inner` needs no borrow, so even a forgotten host borrow guard
does not force recursive teardown. A live borrowed host root continues to hold
its graph; no shared container is cleared by this destructor. Dropping a raw
`Rc<RefCell<Vec<Value>>>`, `Rc<ModuleValue>`, or `Rc<UserFunction>` also reaches
the iterative destructor at its child Values. Environment release, assignment,
removal, error unwinding, and collection checkpoints use the same mechanism.

Destruction is synchronous and requires worklist storage proportional to the
pending graph edges. It does not impose a heap cap, guarantee pause latency, or
recover from allocation failure. The invariant applies to the runtime graph;
function AST destruction retains the separately documented syntax-depth limits.
Arbitrarily deep ASTs built directly by Rust callers remain outside this claim.

### Rust API migration

`Value` now implements `Drop`, so Rust no longer permits moving a non-`Copy`
payload out through a by-value enum pattern. Match `&value` and clone an `Rc` if
an independently owned host handle is needed:

```rust
# use solx::runtime::Value;
let value = Value::list(vec![]);
let Value::List(items) = &value else { unreachable!() };
let host_owner = items.clone();
drop(value);
assert!(host_owner.borrow().is_empty());
```

To extract an owned String without copying its bytes, match `&mut value` and
call `std::mem::take` on that String. Whole Values can still be moved or cloned,
raw variants can still be constructed, and scalar `Copy` payloads can still be
matched by value. Solx programs have no language-semantic migration for this fix.

### Destruction validation

The original watched CLI regression, `var x=nil for i=1,20000 do x=[x] end x=nil`,
aborted with Windows status `3221225725` (`0xC00000FD`, stack overflow). After the
fix it exits successfully. Tests use separate child processes with a 30-second
watchdog, and host graph tests explicitly request a 256 KiB Rust thread stack.
Weak probes verify immediate reclamation of every node, rather than merely
checking successful process exit.

`tests/deep_destruction.rs` covers 100,000-level raw lists/mixed objects, final raw
Rc ownership, duplicate/shared edges, live borrows, replacement/removal under
parent borrows, deep cycles and ordinary GC, Environment clones, type errors,
returns, runtime errors, execution-budget interruption, cancellation, and
forgotten borrow flags. A runtime unit test constructs 20,000 alternating
function/module layers with captured scopes, then drops the last raw function
handle and verifies that all weak function/module probes expire.

The first complete post-fix `cargo test --locked --quiet` run passed 407 tests
(including four independent semantic crosschecks). Strict all-target Clippy also
passed. One additional cancellation regression was then added and is included in
the integrated validation recorded in [OVERNIGHT_PROGRESS.md](OVERNIGHT_PROGRESS.md).
During test development, a fixture named `object` shadowed the standard-library
module; renaming that local restored the intended removal check. The first
cancellation fixture allowed a 100,000-node synchronous GC pass to consume its
controller delay before execution entered the deep local. Its failed assertion
then tried to format the graph and the watchdog terminated it at 30 seconds.
The fixture now completes the initial GC before scheduling cancellation and
uses an assertion that does not render deep values; the corrected targeted test
passes. These Windows observations do not establish arbitrary host stack sizes
or platform support.

## Collection algorithm

1. Remove expired registrations and upgrade each registered node to exactly
   one temporary strong snapshot reference. Iteratively discover reachable
   nodes, including previously unregistered descendants.
2. Record every ownership edge with its multiplicity. Two fields pointing to
   the same collection contribute two incoming references. Module exports
   and Object aliases use the same identity.
3. Subtract the snapshot reference and counted internal incoming edges from
   each node's strong count. Remaining references identify external roots.
   Borrowed nodes are also roots; their uninspected edges remain uncounted,
   conservatively rooting any discovered children they own.
4. Iteratively mark everything reachable from those roots. Clear only the
   unreachable lists, object maps, and scope maps. Every cycle in the current
   value model crosses one of these mutable containers. Functions and modules
   do not require interior mutation.
5. Drop snapshot references, remove expired registrations, and reduce excessive
   registry capacity. Snapshot owners prevent cascading graph destruction
   while unreachable edges are being cleared.

No Solx code runs between tracing and clearing. The closed set of runtime
values has no user-defined destructors or finalizers. Containers selected for
clearing were exclusively borrowable during tracing. If an edge-count
invariant is violated, the pass retains the graph instead of clearing it.

This is reference counting plus cycle collection. The earlier audit preferred
considering a generational-handle tracing heap for a future runtime. Keeping
Rc here avoids breaking public host ownership and borrow behavior while fixing
the demonstrated leaks. A future heap migration still needs explicit handle
lifetimes, allocation policies, and measurements.

## Checkpoints and host API

Allocation registers nodes but does not trigger collection while a constructor
or mutation guard is active. The interpreter checks allocation pressure after
statements and block/loop iterations. It also collects after executed programs,
including runtime errors, and after releasing its roots during destruction.

The initial threshold is 512 newly registered nodes. After collection, the
threshold is `max(512, retained_nodes)`, avoiding repeated scans of a large
live graph after only a small number of allocations. This is a tuning policy,
not a byte limit or an observable language scheduling promise.

Rust embedders have two public functions under `solx::runtime`:

- `collect_garbage() -> CollectionStats` performs a pass without invalidating
  strong host references. After dropping the last host root, use this function
  or a later interpreter checkpoint to release remaining cycles.
- `track_value(&Value)` adopts a raw host graph, revisiting already registered
  nodes to discover newly attached descendants. Release all relevant RefCell
  guards first so the traversal can see every child.

`Value::list` and `Value::object` register automatically, as do runtime
functions, scopes, and module namespaces. Constructors adopt visible new
descendants, and Environment binding registers incoming values. Solx code
needs no explicit collection calls.

The public Rc payloads also allow Rust code to construct raw variants, insert
new raw nodes directly, or replace an allocation through `Rc::make_mut`.
After such operations, call `track_value` before disconnecting or dropping the
raw graph. A registered ancestor still connected during a collection lets the
collector discover those descendants, but an entirely unregistered cycle that
is already unreachable cannot be found. This is a host integration contract,
not a claim that arbitrary unobserved Rc allocations are automatically managed.
Perform host cleanup before thread-local destruction; collector calls become
no-ops once that thread's registry has been destroyed.

`CollectionStats` reports examined, reclaimed, remaining, and borrowed node
counts for that pass, plus cumulative `collections` and `total_reclaimed`
counts on the thread. Reclaimed means the payload has no strong owners; Rust
Weak handles may still retain an allocation's control block. These counts are
not byte measurements, RSS, or guarantees that the OS receives memory back.

## Cycle-collection milestone validation (2026-10-23)

The pre-change baseline passed 209 Rust tests. After implementation:

| Check | Observed result on Windows |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --locked --quiet` | 229 passed |
| `cargo clippy --all-targets --locked -- -D warnings` | Passed |
| `npm run test:interpreter` in `vscode-solx` | 5 passed against the rebuilt interpreter |
| `cargo run --release --locked --example gc_profile` | Passed; all 25 batch retention assertions held |
| `cargo build --release --locked --bin solx` | Passed; refreshed `target/release/solx.exe` |
| Release CLI running `examples/error_handling.solx` | Passed; handled error and printed the continuation message |
| `git diff --check` | Passed |

Twenty new integration tests cover mixed cycles, duplicate edges, external
roots, weak handles, shared/mutable borrows, raw graph adoption, Environment
clones, automatic collections during loops, closure temporaries, mutual
recursion, try/catch unwinding, module initialization/cache/export aliases,
failed-module escapes, metadata pruning, and a 10,000-node cycle. A deterministic
reachability oracle checks twenty generated mixed graphs against expected live
nodes. The three earlier leak probes now assert successful reclamation.

The reproducible release smoke profile is [gc_profile.rs](../examples/gc_profile.rs):

```sh
cargo run --release --locked --example gc_profile
```

Observed in the 2026-10-23 run: 25 batches of 1,000 cyclic list/object/closure
graphs; 225 collection passes; 100,025 reclaimed nodes; five retained nodes
before and after every batch; 264 ms elapsed for the workload and collections,
excluding compilation. That is one local run, not a general performance result.
No comparison with Lua, Luau, or the pre-collector runtime was made, and process
memory and pause percentiles were not measured. Rustup emitted its existing
host-path canonicalization warning; checks exited successfully.

No editor GUI, installer, or package tests were rerun: editor/package sources
are unchanged by this milestone, and existing distribution artifacts were not
rebuilt. Their earlier receipts do not validate this new runtime.

## Remaining work

The full graph pass uses time and temporary storage proportional to nodes and
edges. It is synchronous; large live heaps can cause long pauses. Borrowed
graphs can remain retained indefinitely while a host keeps guards alive.
Host-only allocation loops need explicit checkpoints. Automatic collection
does not impose an allocation cap or recover from out-of-memory failures.

Parsing, expression evaluation, function calls, imports, and AST clone/drop
still use recursive paths. Runtime graph destruction is now iterative as
described above; arbitrary deep programs are not thereby stack-safe.
Execution/AST guards and cooperative cancellation are now implemented in
[EXECUTION_LIMITS.md](EXECUTION_LIMITS.md). They do not bound value-graph depth,
allocation size, or GC work. An explicit VM stack remains major work. Broader
memory/latency profiling should guide collector tuning and any heap redesign.

Files changed in this milestone: `src/runtime/gc.rs` (new),
`src/runtime/mod.rs`, `src/runtime/environment.rs`, `src/runtime/value.rs`,
`src/interpreter/eval.rs`, `src/modules.rs`, `src/stdlib/mod.rs`,
`tests/garbage_collection.rs` (new), `tests/garbage_collection_modules.rs` (new),
`examples/gc_profile.rs` (new), `README.md`, `docs/SPEC.md`,
`docs/RUNTIME_AUDIT.md`, `docs/LANGUAGE_REVIEW.md`, and this new document.
Existing work was preserved; no dependencies or versions changed. At the end
of implementation validation, nothing had been staged, committed, pushed, or
published; a later local commit is separate from these validation results.
The final preservation check found exactly the ten intended existing files
changed, five new source/documentation files, 426 unchanged baseline files,
and no missing files. Git's index and HEAD matched the pre-edit snapshot at
that validation checkpoint.
