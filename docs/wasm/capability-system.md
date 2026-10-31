# Capability System

Compile-time capability checks and host authority are separate controls.
`[permissions].allow` bounds the categories inferred from a program; the host
and selected Wasm route determine which operations its artifact can perform.
Neither the category check nor import pruning is a general sandbox for
arbitrary native code or subprocesses.

## Capability Categories

The current vocabulary is the 14 leaves in
`crates/almide-ir/src/effect.rs` (`Effect::ALL`):

| Parent | Leaves |
|--------|--------|
| `FS` | `FS.read`, `FS.write` |
| `Net` | `Net.fetch`, `Net.listen` |
| `IO` | `IO.stdin`, `IO.stdout`, `IO.stderr` |
| `Env` | `Env.read`, `Env.write` |
| — | `Proc`, `Time`, `Rand`, `Fan`, `Abort` |

A parent grants its listed leaves. `IO` does **not** grant filesystem access;
`Proc` has no parent. `All` is not a grant name. Unknown names are rejected by
`src/project.rs::allowed_effects`. Inference and manifest checking use sets of
`Effect`, not the earlier proposed 13-bit `u16` representation.

Categories are assigned per operation. The registry and stdlib `@category`
annotations are authoritative; see `STDLIB_MODULE_EFFECTS` and
`stdlib_call_effect` in `crates/almide-ir/src/effect.rs`. For current function
signatures, use the [generated stdlib reference](../stdlib/README.md), rather
than a second API list here.

Output and abort builtins may occur in a plain `fn`, but still carry categories:
`println` uses `IO.stdout`, `eprintln` uses `IO.stderr`, and `panic`/failing
assertions use both `Abort` and `IO.stderr`. Returning `err(...)` is not `Abort`.
The effect bit, fallibility, and category set are distinct; see the
[effect specification](../specs/effect-system.md) and
[ADR-0026](../adr/0026-effect-categories-ride-the-transparent-fn-type.md).

## Declaration

```toml
# almide.toml
[permissions]
allow = ["FS.read", "IO.stdout"]
```

Dotted leaves and the four parents above are accepted. With no `allow` key,
the category restriction is not applied (backward compatibility). An explicit
`allow = []` grants no categories; it is not an alias for an implemented
`pure` profile. The absence of clock or random capabilities alone does not
prove determinism or eliminate timing side channels.

Other permission keys have separate purposes:

- `proc` bounds process commands; see below.
- `fs` bounds filesystem roots, relative to the project root when relative.
  An empty list grants no roots; the embedded host defaults are described below.
- `native` grants dependency packages permission to bring native code; it is
  not a category name or a general OS sandbox. See `src/native_grant.rs`.

## Inference and Diagnostics

`EffectInferencePass` records `IrProgram.effect_map`. Its `effect_flow` solver
propagates sets through direct calls and function values, including callbacks
passed to higher-order functions. This is implemented, not a future Phase 4.
The compiler's category input is per-operation stdlib metadata and builtin
metadata, not a blanket mapping from every function in a module to one effect.

Foreign `@extern` functions and non-stdlib `@inline_rust` templates are opaque
to inference and carry every leaf (top). They pass an explicit category bound
only if it grants every leaf. This does not prove what the foreign body does.
See `crates/almide-codegen/src/pass_effect_inference.rs` and `effect_flow/mod.rs`.

Use `almide check --effects` to inspect inferred sets. Permission checking
compares transitive requirements to the manifest and reports missing leaves
with a provenance path when available. The diagnostic begins with
`error: capability violation in ...`; it is **not E010** (E010 is a pattern
matching diagnostic). The renderers are `src/cli/check.rs` and `src/cli/mod.rs`.
Do not rely on the old mock diagnostics or a fixed 20-round inference limit.

## Process Commands

A `Proc` grant allows process operations, which can reach other resources
through the child program. The command allowlist is not a sandbox for the
child's effects: granting `cargo` or a shell can confer substantial authority.

### What the checker enforces today

`Proc` is its own leaf (ADR-0026): `[permissions] allow = ["Proc"]`
admits process operations, and no parent implies it. `[permissions] proc`
bounds commands separately (ADR-0025):

```toml
[permissions]
allow = ["IO", "Proc"]
proc = ["git", "cargo"]
```

- `exec` must name a command literal on the list. `output`, `run`, and
  `start` take a `ProcessCommand` record whose `cmd` must be such a literal.
  A computed command or a spawning function passed as a value is refused
  when it cannot be checked. See `src/cli/mod.rs::PROC_SPAWNING` and
  `proc_command_literal` for the current API, not the retired helper names.
- `process.kill` starts no child but signals a process. Its
  `@signals_process` annotation bounds it as the command `kill`; the embedded
  host checks this too (`stdlib/process.almd`, `host_process.rs`).
- No `proc` key leaves commands unbounded; `proc = []` grants none.
  Granting `Proc` without a command list warns, or is refused under
  `--profile critical`.

### On wasm: the private `almide:process/spawn` import

The private subprocess interface is defined in
[`spawn.wit`](../../crates/almide-wasm-run/wit/process/spawn.wit).
Its operation cases include legacy ABI slots; they are not all current
source-level functions. See the [process reference](../stdlib/README.md)
and `stdlib/process.almd` for the source API.

- The embedded core host serves supported synchronous operations through its
  process host dispatch, sharing native runtime operations.
- A stock-p1 artifact that needs a process host operation imports
  `almide:process/spawn::call` and exports `cabi_realloc`. A runtime that does
  not implement this private import refuses the artifact at instantiation.
- The child-handle family (`start`, `poll`, `wait`, `stop`, `read_new_stdout`,
  `read_new_stderr`, `child_pid`) is native-only pending a Wasm child resource
  (#4308). It is not covered by saying the embedded host serves process I/O.
- Component routes do not declare this private subprocess interface.

The per-function, per-route availability is recorded in
[`target-availability.toml`](../../proofs/target-availability.toml), not inferred
from the presence of a WIT case. ADR-0025 records the private-host decision.


## Wasm Imports and Host Scoping

Imports depend on reachable operations and the chosen artifact/host route,
not just the text of the allow list. A single import can serve several
operations: for example, the embedded core host dispatches filesystem calls
through `almide.fs_call`. The presence or absence of an import is therefore
not a one-to-one proof of the category set. The earlier per-capability import
and binary-size tables were design sketches, not current artifact guarantees.
See [Wasm output](WASM-OUTPUT.md), [the host overview](README.md), and the
[operation capability registry](../../proofs/target-availability.toml).

An external runtime must provide the imports an artifact declares; unresolved
imports fail instantiation. A stock WASI artifact receives the authority its
runtime grants, for example preopened directories. This does not make a grant
of `FS.read` alone equivalent to a runtime read-only descriptor policy.

For the embedded host, `[permissions] fs` is implemented:

- Core-module filesystem operations are checked and executed beneath the
  configured roots. Descriptor-relative containment is implemented on 64-bit
  Linux and macOS; other platforms use a path-based check that remains subject
  to check/use races (`fs_beneath_sys.rs`). Without the `fs` key, this control
  leaves paths unrestricted.
- The component host preopens configured roots read-write. Without the key, it
  preopens the working directory and temporary directory, not `/`.

The implementation is `crates/almide-wasm-run/src/host_fs_scope.rs` and
`component_host.rs`. These controls do not turn the embedded host into general
isolation for arbitrary untrusted code. In particular, process authority and
foreign native code require separate consideration.

## Per-Dependency Restriction (Future)

```toml
[dependencies.json_parser]
version = "1.0"
allow = []              # pure only -- no I/O, no net, nothing

[dependencies.http_client]
version = "2.0"
allow = ["Net.fetch"]   # can fetch, nothing else
deny = ["Proc"]         # explicitly deny even if parent allows

[dependencies.code_formatter]
version = "0.5"
allow = ["FS.read"]     # read-only access to source files
```

Proposed behavior: reject a dependency whose transitive call graph exceeds its
per-dependency grant. This syntax and enforcement are not implemented by the
current top-level `[permissions].allow` check.

The proposal would create a trust hierarchy: the top-level project declares its own capabilities, and each dependency is restricted to a subset. It is not a current containment guarantee.

## Proposed Profile Names (Not Implemented)

The earlier design named common permission sets `pure`, `readonly-agent`,
`readwrite-agent`, `network-agent`, and `full-agent`. `[permissions] profile`
is not an implemented manifest key; do not use it to request a restriction.
The CLI's `--profile critical` is a separate feature, not one of these aliases.

Write explicit `allow` lists instead, for example:

```toml
[permissions]
allow = ["FS.read", "IO.stdin", "IO.stdout"]
fs = ["workspace"]
```

This grants the listed categories and filesystem roots; it is not a proof
that all observable behavior is deterministic or that every host resource is
contained. Extend the list explicitly when another operation is required.
