<!-- description: spec/ is the trust spine's v0 corpus, not a test directory — the revived monkey suite grows two shrink-only ratchets and breaches the caps-soundness backstop -->

# Reviving the monkey regression suite: what `spec/` actually is

**Status**: open. Found 2026-08-08 while reviving the monkey regression suite
(`MONKEY_BUGS.md`, 41 files, written 2026-04-05 at v0.11.4) that had sat in the
repo root's `_scenarios/` and rotted unrun because CI only runs `almide test spec/`.

The suite is correct and passes: all 41 files run green on both targets
(`almide test`, 393 files 0 failed with them in). Moving them into `spec/` is
what fails — and the reason is the point of this note.

## `spec/` is the v0 corpus, not a test directory

`proofs/corpus-wall.sh` (CI job `Trust Spine (v1)`, `make verify-trust`) sweeps
`$ROOT/spec` — the WHOLE directory — through the real frontend → MIR and enforces
three properties. Two of them are **ratchets that may only shrink**. Adding files
to `spec/` therefore is not free: any new source shape that the v1 lowering does
not yet cover grows a ratchet, and the gate is explicit about the policy:

```
WALLED-REAL RATCHET FAIL: function(s) walled that are NOT in proofs/walled-real-baseline.txt —
ship the lowering in the same change (adding entries is a reviewed regression, not a fix)
```

The 41 revived files add **15 new walled-real functions** across 7 files:

| file | new walls |
|---|---|
| `scenario8_error_taxonomy_test.almd` | 3 |
| `scenario3_pipeline_test.almd` | 3 |
| `monkey24_variant_advanced_test.almd` | 3 |
| `monkey08_option_result_test.almd` | 2 |
| `monkey05_pattern_match_test.almd` | 2 |
| `monkey13_records_test.almd` | 1 |
| `monkey14_edge_cases_test.almd` | 1 |

They are ordinary Almide: a match-arm guard, a heap-result `match`/`if` tail, `??`
over an untracked Option operand, `list.filter_map` with a closure-list argument,
a `match` over an untracked subject with a call-bearing arm. Each is a real v1
lowering gap the current `spec/` corpus does not exercise.

## Plus one hard breach: `mir_calls > ir_calls`

Two of the files also breach the **caps-soundness backstop**, which has no
baseline and must be 0 — a `record_elided_calls` marker may only surface a
genuinely ELIDED call, so the MIR call count can rise at most TO the IR's; if it
EXCEEDS, a marker double-counted a lowered call, which could mask another elision
and falsely de-taint a Stdout-reaching function.

```
MIR>IR monkey28_nested_data_test.almd::profile_summary (mir 8 > ir 7)
MIR>IR scenario5_plugin_test.almd::merge_sources        (mir 3 > ir 2)
```

### Minimal repro

```almide
type DS = { fetch: fn(String) -> Result[List[String], String] }

effect fn merge(sources: List[DS]) -> List[String] = {
  var all: List[String] = []
  for src in sources {
    let rows = src.fetch("*")!
    all = all + rows
  }
  all
}

test "t" { assert(true) }
```

```
$ ALMIDE_DEBUG_CALL_OPS=1 cargo run -p almide-mir --example classify_corpus -- --out /tmp/o <dir>
[call-op] merge: CallIndirect              <- src.fetch("*")
[call-op] merge: CallFn __list_concat_rc   <- all + rows
[call-op] merge: CallFn __str_concat       <- SPURIOUS: the source has no string concat
[call-count] ir=2
MIR>IR <dir>/a_test.almd::merge (mir 3 > ir 2)
```

`ir=2` is correct (one `Call`, one `ConcatList` — `count_ir_calls` credits every
`ConcatList` node). The third MIR op is the defect: a `__str_concat` `Op::CallFn`
in a function whose source contains no `ConcatStr` node anywhere.

### Root cause (confirmed 2026-08-08)

`build_append_slot_subst`, `crates/almide-mir/src/lower/mod_p5_b.rs:513-527`.
Lowering an accumulator loop to the tail-recursive append-slot form synthesizes
a COPY of the accumulator — the function's own doc comment says so: *"a String
accumulator copies via `acc + ""`, a list via `acc + []`"*.

```rust
let (empty, concat_op) = if matches!(list_ty, Ty::String) {
    (tco_ir(IrExprKind::LitStr { value: String::new() }, Ty::String), BinOp::ConcatStr)
} else {
    (tco_ir(IrExprKind::List { elements: vec![] }, list_ty.clone()), BinOp::ConcatList)
};
let copy = tco_ir(IrExprKind::BinOp { op: concat_op, left: Var(acc_var), right: empty }, ..);
```

That `acc + ""` is a **`ConcatStr` node the source never had**.
`try_lower_concat_str` (`lower/calls_p2.rs:140`) lowers it to a real
`Op::CallFn { name: "__str_concat" }`, so `mir_calls` gains a call that
`count_ir_calls` — which walks the SOURCE body — has no node to credit. Hence
`mir 3 > ir 2`. Measured directly by instrumenting the emitter:

```
[sc] __str_concat  ty=String
     left  = Var { id: VarId(7) }
     right = LitStr { value: "" }
```

Ruled out on the way: `record_elided_calls` (`lower/calls_b.rs:170` — its push
prints nothing for this function) and `lower_interp_compound_wall`
(`lower/calls_p2.rs:592` — the repro has no string interpolation).

### The fix is a decision, not a patch

The injected copy is the same class as `$__mg_take`, which the gate already
excludes by name, reasoning that it is

> a COMPILER-INJECTED slot accessor … not a lowering of any IR call node: a
> mutable-global heap assign injects one with no IR counterpart, so counting it
> would false-breach `mir <= ir`.

But `__str_concat` cannot simply join that filter: a REAL source-level string
`+` also lowers to `__str_concat`, and the gate credits each source `ConcatStr`
node as one ir_call precisely so those stay counted. Excluding the name
wholesale would under-count real calls and weaken the backstop — the opposite
of its purpose. Three ways out, increasing in intrusiveness:

1. **Name the injected copy distinctly** (`__str_copy` / `__acc_copy`) so the
   gate excludes it exactly, the way `__mg_take` is excluded. Adds a self-host
   symbol to link.
2. **Credit it on the IR side** — teach `count_ir_calls` that a String
   append-slot accumulator implies one synthetic concat. Emitted program
   unchanged, but it duplicates the lowering's dispatch rule inside the gate,
   which is exactly the drift the `__mg_take` comment warns about.
3. **Copy without a `ConcatStr`** — give the append-slot lowering a dedicated
   copy operation instead of reusing the `x + ""` identity trick.

(3) is cleanest — the copy is not a concatenation and only looks like one
because `x + ""` was a convenient identity — but it changes the
ownership-certified lowering, so it wants its own review rather than a
drive-by. Whichever is chosen, the acceptance test is the same: restore
`spec/regression/` and watch `proofs/corpus-wall.sh` hold at 0 breaches.

## Current state and the way in

`spec/regression/` was reverted; the 41 files + this repro are in the working
archive (`../almide-docs-archive/blocked-on-mir-caps-gate/`) and in git history
(`git log --diff-filter=D -- spec/regression/`).

Landing the suite is a real piece of work, in this order:

1. **Close the caps accounting gap** (the 2 hard breaches). This is a soundness
   backstop — it must be fixed, not baselined.
2. **Close, or consciously baseline, the 15 lowering gaps.** The ratchet's own
   policy is "ship the lowering in the same change"; baselining 15 entries is a
   reviewed regression and needs a decision, not a drive-by.
3. Restore `spec/regression/` — the restoration is the acceptance test.

Until then the suite stays out of `spec/`, because putting it in makes
`Trust Spine (v1)` red and no amount of test value justifies a red soundness gate.
The value it already delivered is this document: 15 uncovered lowering shapes and
one accounting breach that four months of `spec/` growth never surfaced.
