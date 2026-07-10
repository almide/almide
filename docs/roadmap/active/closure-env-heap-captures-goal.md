<!-- description: GOAL PROMPT — closure env full mode: heap/Float/Fn captures with masked recursive drop -->
# GOAL PROMPT — closure env full mode: heap, Float and Fn captures

> **Read first**: `crates/almide-mir/src/lower/binds.rs::lift_lambda` (the
> shipped scalar-capture closure block — the code you extend),
> [certificate-format-v1](certificate-format-v1.md) item 5's follow-up
> paragraph (the shipped scope + this exact ratchet named), `proofs/CowSafety.v`
> (why Dup-share capture preserves value semantics), the record-mask machinery
> (`record_masks` / `heap_slot_masks` / the masked variant drops — scout
> `DropWrapperRec` and `drop_op_for`).

## Context (2026-07-10, commit `7b91dcac`)

- Capturing closures SHIPPED with **i64-scalar captures only** (Int/Bool):
  closure block = DynList `[rc][len][cap][fnidx][captured…]`, env passed as the
  leading BORROWED arg, prologue `Load{8}` reads captures back. v0-byte-
  identical (`closure_capturing_wasm.almd`), corpus 4,745 in-profile.
- The gate in `lift_lambda`: `if !matches!(ty, Ty::Int | Ty::Bool) { return
  None; }` — a String/List/Value/variant/Float/Fn capture still defers, and a
  lambda body OUTSIDE the C1 inline path that captures one WALLS (the
  `greeter`-class: `fn greeter(name: String) -> (String) -> String =
  (x) => name + x`).
- The v1 brick ladder is complete; this is the recorded ratchet that also
  resolves `b`'s "closure-env consumer" claim honestly.

## The goal (one line)

> **Close the capture-kind gate: a closure may capture HEAP values (String /
> List / Value / variant / record), Fn values (closures capturing closures —
> `compose`), and Float — with the env OWNING its heap captures (masked
> recursive drop, never touching the fnidx slot) and the lambda reading them
> back as borrowed handles — v0-byte-identical, every witness proven.**

## Non-negotiable invariants

1. **Honest wall discipline**: any capture kind you cannot yet materialize
   faithfully keeps returning `None` (deferred) — never an empty/garbage env.
   Byte-parity vs `almide run` (v0) on every opened shape before commit.
2. **The fnidx slot is SCALAR**: slot 0 must NEVER be rc_dec'd by the block's
   drop. A flat `DropListStr` on a closure block would interpret the table
   index as a pointer — memory corruption. The masked-drop selection is the
   soundness core of this ratchet; write an adversarial test proving slot 0 is
   untouched by the drop.
3. **Ownership certs stay proven-ACCEPT**: capture = the established
   Var-element pattern (`Dup` the still-live var → store the fresh handle →
   `Consume` it into the block: cert `a` + `m`, original var's scope-end drop
   unchanged); block drop = one `d` on the block stream with the masked
   recursive free at render. corpus-wall (incl. the kernel oracle) green.
4. **Zero new checker rules**: everything rides `i/a/d/m` + the existing
   masked-drop render machinery — elaboration, not new trusted surface.
5. Commit style: English, one line, no prefix. Push at all-green only.

## Sub-tasks

**1 — heap captures (the core).**
- `lift_lambda` capture gate: admit heap tys. Creation side: for each heap
  capture, `Dup` (the closure co-owns — CowSafety makes share safe under value
  semantics: any later in-place mutation goes through MakeUnique's
  clone-on-shared, so neither side observes the other), store the handle into
  its slot, `Consume` the fresh handle (mirrors `try_lower_str_list_literal`'s
  Var element). Register the block's HEAP-SLOT MASK (slot 0 scalar, capture
  slots per-kind) so `drop_op_for(blk)` selects the masked recursive drop —
  scout how record blocks register `record_masks`/`heap_slot_masks` and reuse
  that exact channel; String/List/Value/variant captures each need their
  recursive free class (the `DropListStr`/`$__drop_value`/`DropVariant`
  vocabulary already exists).
- Lambda side: prologue `LoadHandle` (not `Load{8}`) for heap capture slots;
  the loaded handle joins `param_values` (BORROWED — the env owns it; a body
  that returns/consumes it must Dup first, exactly the param discipline) and
  gets `seed_variant_param` for variant/aggregate captures so a `match`/field
  read inside the closure executes.
- **`b` resolution (record it)**: decide whether env-slot reads emit `b` on
  the block's stream. The 5b discipline says borrowed zero-seeded streams stay
  event-free — if that holds here too, RECORD in certificate-format-v1.md that
  `b`'s load-bearing consumer is `MakeUnique` (and retire the "closure-env
  borrow" framing) rather than leaving the claim dangling.

**2 — Fn captures (closures capturing closures).**
A captured Fn value is a heap capture of a CLOSURE BLOCK: the loaded handle
must ALSO join `closure_values` in the sub-context so `g(x)` inside the body
dispatches via `emit_closure_call`. Target fixture: `fn compose(f: (Int) ->
Int, g: (Int) -> Int) -> (Int) -> Int = (x) => g(f(x))` — the killer shape.
Mind the drop mask: a captured closure block frees via the closure-block drop
(recursive if IT holds heap captures — scout whether the mask vocabulary
nests; if not, gate nested-heap-env captures honestly and record).

**3 — Float captures.**
`Load{8}`/`Store{8}` move raw bits, but a Float local is wasm f64 — scout how
the renderer types lambda locals and whether an i64↔f64 reinterpret exists in
the prim vocabulary. If it does, wire it; if not, keep Float deferred with the
reason recorded (do not emit type-punned invalid wasm).

**4 — fixtures + gates.**
- spec/lang: `closure_capturing_heap_wasm.almd` (String capture `greeter`,
  List capture, two instances with independent envs, `// wasm:enabled`),
  compose fixture; both byte-matched vs v0.
- proofs/fixtures: a heap-capture closure for gate rows (ownership ACCEPT +
  modes ACCEPT through the kernel oracle).
- almide-mir unit tests: cert shape for the capture (`…a…m` + block `i…d`),
  the adversarial slot-0 mask test, verify_ownership agreement.
- corpus-wall BEFORE/AFTER: in-profile must rise (the `greeter`/HOF-capture
  walls open); record the delta in certificate-format-v1.md.

## Verification ladder

```
cargo test -q -p almide-mir            # unit + differential + render
make install && almide test            # full spec, both targets
proofs/gate.sh                         # rows + kernel twins + tamper drill
proofs/corpus-wall.sh                  # walls ↓, PCC + kernel oracle ACCEPT
cargo test -q                          # workspace
```

## Exit criteria

- [ ] String/List/Value/variant and Fn captures execute v0-byte-identically;
      Float wired or honestly recorded as deferred.
- [ ] Slot-0 adversarial mask test in almide-mir; no drop ever touches fnidx.
- [ ] `b`'s consumer question RESOLVED in writing (certificate-format-v1.md).
- [ ] corpus in-profile strictly ↑, walls strictly ↓, all witnesses ACCEPT
      (binary + kernel oracle).
- [ ] Pushed at all-green; Trust Spine green on the push.

## What NOT to do

- No deep-copy capture "to be safe" — Dup + COW IS the value-semantics answer
  (CowSafety); a deep copy would silently change perf class and duplicate the
  cow machinery.
- No flat drop on closure blocks with heap captures (invariant 2).
- Do not start the regex/histogram goal here; do not touch the checker.
