# Native bug fixes: research and validation (2026-09-09)

## Reference checkouts

The requested `../almide-references` directory contains shallow checkouts:

- Gleam: `gleam-lang/gleam`, `19bf207ebb7d953ea1391f041da48c214ee1440a`.
- Rust: `rust-lang/rust`, `0d31508599a7814a7044e9a7a871e3dc5f037753`;
  sparse checkout of MIR construction and standard-library process internals.

## #2062: an outlined branch must preserve its assignment destinations

Rust's [`builder/expr/stmt.rs`](https://github.com/rust-lang/rust/blob/0d31508599a7814a7044e9a7a871e3dc5f037753/compiler/rustc_mir_build/src/builder/expr/stmt.rs)
lowers assignment by evaluating the RHS, resolving the LHS as a place, then
assigning to that place. Moving a branch into a value-parameter helper cannot
preserve an enclosing local's storage. Gleam's
[`javascript/expression.rs`](https://github.com/gleam-lang/gleam/blob/19bf207ebb7d953ea1391f041da48c214ee1440a/compiler-core/src/javascript/expression.rs)
dispatches case expressions and function literals separately; its immutable
binding semantics are not evidence that Almide's mutable captures can be copied.

Decision: decline branch outlining when the branch writes a variable that is
neither bound within the branch nor a module global. Reuse the existing mutation
collector, including write-only and collection-write targets. Both the dense
expression route and the let-bound route pass through this guard. Local mutation
and global access retain their existing behavior. Nested-lambda writes are
conservatively included. No reference-parameter ABI or new ownership rule is
introduced merely to preserve this optimization.

The original retry loop changed from `cost=0.0 n=2` to `cost=0.5 n=2` on native;
`almide run --target wasm` also prints the latter. The new spec covers retry-loop
state, a write-only capture outside a loop, list writes, and branch-local mutation.
The legacy explicit-WASM *test* runner skips this fixture at its rendering wall;
that skip is not counted as a successful WASM regression test.

## #2063: adapt the function-value ABI at the runtime boundary

Almide emits `Rc<dyn Fn(String)>`, while the HTTP streaming implementation
accepted `impl FnMut(String)`. The diagnostic demonstrates that `Rc` does not
satisfy that generic bound. The existing SSE callers need stack-borrowing mutable
closures, so replacing all callbacks with owned `Rc` closures would alter their
lifetime and mutation behavior.

Decision: expose the Almide function-value signature at
`almide_http_request_stream`, forwarding through a small closure to the existing
`FnMut` implementation. Native SSE callers call that implementation directly.
The loopback integration test checks expression, block and bound callbacks,
shared-list mutation, and both content-length and chunked responses.

## #2065: the deadline covers pipe EOF as well as child exit

Rust's [`sys/process/unix/unix.rs`](https://github.com/rust-lang/rust/blob/0d31508599a7814a7044e9a7a871e3dc5f037753/library/std/src/sys/process/unix/unix.rs)
implements the configured process group with `setpgid`. A descendant can keep
stdout/stderr open after the direct child exits, so an unconditional reader join
cannot be part of a bounded timeout path.

Decision: create a private Unix process group, poll child exit and both reader
completions under one deadline, and kill the group on timeout. Windows uses the
existing taskkill approach with `/T`. Join readers only after they have finished;
error paths never wait for pipe EOF. A descendant that deliberately escapes the
group can leave a detached reader alive until it closes its inherited pipe, but
cannot hold the caller past the deadline through that reader. Process-tree
termination is best effort; this is not a process containment mechanism.

The Unix integration test checks a live child, an already-exited child, inherited
stderr, and a successful nonzero exit. Windows tree cleanup requires Windows CI.

## Quality measurement and existing PRs

Use codopsy **2.2.0**, built from O6lvl4/codopsy
`fabfbdccac8bee5fc71055b6859c0684fd6fce5b`, with the unchanged `.codopsyrc.json`.
The installed PATH binary is 2.1.0 and reports different scores.

The commissioned CI gate measures `crates/almide-wasm`: **91/A**. The modified
optimizer crate measures **96/A**; `runtime/rs/src/process.rs` measures **100/A**.
The broader `crates` tree is **82/B**, codegen is **89/B**, and the runtime tree
is **83/B**. These are existing broader quality deficits; this batch does not
claim that the entire repository is A or relax rules to manufacture that result.

Existing work was checked before implementation: #2054 is addressed in PR #2053,
#2059 in PR #2064, and #2044 in PR #2055. At inspection, #2053, #2055 and #2064 conflicted
with develop. This batch does not duplicate those changes or mark their issues
resolved merely because a PR exists.

## Validation

- `almide test`: 438/438 files; 425 WASM, 13 native fallback.
- `almide test --target rust`: 438/438 files.
- Optimizer/codegen Rust tests: 78 passed.
- Interpreter regression: passed, including the loop's accumulated value before
  the subsequent write-only overwrite.
- HTTP streaming integration: passed (three callback forms, two encodings).
- Unix timeout integration: passed (three orphan-pipe shapes, normal exit 7).
  Only the timeout cases use 100ms; the normal-exit case has a generous 30s bound
  so machine load cannot turn its success assertion into a timing flake.
- AST corpus parity: 1 passed; diagnostics corpus parity: 3 passed. Added only
  the new fixture's measured rows; existing oracle hashes were not rewritten.
- File discipline, pass roster, layout discipline, libm determinism, fixture
  formatting, and `git diff --check`: passed.

### CI coverage follow-up

The legacy v1 `almide test` path declines the list-element mutation case,
although the structural `almide run --target wasm` path supports it. The
three scalar/record spec cases stay on the v1 WASM path. The list case is
pinned separately in `tests/branch_lift_heap_mutation_test.rs`, which runs
the native and structural WASM targets and checks the final element is 2.
The fallback baseline is unchanged.
