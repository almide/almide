# GOAL PROMPT — finish ADT brick 5 (heap-field bind + recursive drop) to the #1 lever

You are continuing the v1 trust-spine ADT value-model work on branch **`develop-v1`**. Bricks
1–4 + 5a are DONE and pushed (see `git log`). Your job: **land bricks 5c (heap-field match
bind) and 5b (recursive drop), so the recursive `Expr` to_string byte-matches v0 — the #1
cross-repo wall lever (~41 walls).** Read `docs/roadmap/active/v1-adt-value-model.md` first; it
is the authoritative design + status.

## The goal, concretely

This program must byte-match v0 (`almide run`) on the v1 trust spine
(`render_program` → wat → `wasmtime`):

```almide
type Expr = Lit(Int) | Add(Expr, Expr) | Neg(Expr)
fn tos(e: Expr) -> String = match e {
  Lit(n)    => int.to_string(n),
  Add(l, r) => "(" + tos(l) + " + " + tos(r) + ")",
  Neg(x)    => "-" + tos(x),
}
fn main() -> Unit = { println(tos(Add(Lit(1), Neg(Lit(2))))) }   // v0: (1 + -2)
```

It currently WALLS (honest). Done = v0==v1 byte-match for this AND a create+drop leak loop
over `Expr` AND `bash proofs/corpus-wall.sh` ACCEPT (all 3 properties) AND `cargo test -p
almide-mir` shows no NEW failures (2 pre-existing `self_hosted_json_*` `$__drop_value` fails
are unrelated).

## NON-NEGOTIABLE discipline (the ② cardinal rule — already re-proven twice this arc)

**Never ship MIR the kernel-proven `[ownership]` checker rejects.** A heap-field variant
lowering can byte-match on `wasmtime` yet be a LATENT DOUBLE-FREE — only `corpus-wall.sh`'s
proven checker catches it. Two naive 5c attempts (borrow `LoadHandle`+`param_values` + the
Option-style move-out auto-Dup in `lower_heap_result_arm`) BOTH got `[ownership] REJECT`
(ownership would rise 16085→16162). So: corpus-wall ACCEPT is the gate for EVERY step; if it
REJECTs, REVERT (do not ship), don't reason your way past it. Work in small increments, run
corpus-wall after each.

## Where the code is

- Construct: `try_lower_variant_ctor` in `crates/almide-mir/src/lower/binds.rs` (5a = scalar +
  leaf `String` fields already land; the `String` field is moved in + masked via `record_masks`,
  freed by the existing DropListStr). Heap NON-String / nested-variant fields WALL.
- Match: `crates/almide-mir/src/lower/control.rs` —
  `try_lower_custom_variant_match` (value), `lower_custom_variant_unit_match` (unit-stmt),
  `parse_variant_arms` (shared arm parser — currently rejects heap-field binds),
  `bind_variant_arm` (scalar binds only), `emit_variant_arm_chain` / `emit_variant_unit_chain`.
  Wired at: tail.rs (scalar+heap), binds.rs (let-bind), calls.rs (operand/arg + ctor construct
  in arg position), control.rs:~122 (unit statement).
- v0 reference layout: `crates/almide-codegen/src/emit_wasm/collections.rs::emit_record`,
  `equality.rs::{variant_alloc_size,find_variant_tag_by_ctor}`, `mod.rs` variant registration.
  (v1 uses its OWN uniform-i64-slot block — tag@slot0, fields@slot1.., padded to slot_count —
  NOT v0's byte-packed layout. Only the observable stdout must match v0.)

## Step 5c — heap-field match bind (the blocker; do this FIRST, cert-driven)

1. **Diagnose the reject precisely** (don't guess). Temporarily re-enable a heap-field bind in
   `parse_variant_arms` + `bind_variant_arm` (borrow model), build classify_corpus, run
   `cargo run -q -p almide-mir --example classify_corpus -- --out /tmp/cw spec`, then read
   `/tmp/cw/ownership.cert`. Each line is one heap object's `i/a/d/m` lifecycle; find the
   UNBALANCED one the checker rejects (the corpus pattern that breaks). Understand the exact
   shape before redesigning. Then REVERT the scratch change.
2. **Redesign the bind to be cert-balanced.** Hypothesis (verify, don't trust): bind a heap
   field as a **`Dup`'d OWNED copy** (`LoadHandle` then `Op::Dup`, rc+1, push to
   `live_heap_handles`) so a read-only use drops it at arm end (balanced) and a move-out is a
   clean owned hand-off — rather than a borrow. Study how `let x = r.field` heap extraction
   (`lower_heap_extraction`) and the Option heap-payload path coordinate move-out vs read-only;
   the right model must make BOTH `Add(l,r) => …tos(l)…` (borrow-passed to a recursive call) and
   `Text(s) => s` (move-out) cert-balanced. Gate corpus-wall ACCEPT.
3. Byte-match a String-field bind fixture (`Msg = Text(String) | …; match m { Text(s) =>
   string.len(s), … }` and `Text(s) => s`) + a leak loop. Commit only when corpus-wall ACCEPT.

## Step 5b — recursive drop (after 5c)

A nested-variant ctor field (`Add(Expr, Expr)`) cannot be freed by a flat `rc_dec` of the child
slot — that leaks the grandchildren. You need a **tag-driven recursive free**, the shape of the
existing `Op::DropValue` / `$__drop_value` (the dynamic-Value recursive drop). Likely design:

- Allow nested-variant heap ctor fields in `try_lower_variant_ctor` (move child handle in, mask).
- Emit, per recursive variant type, a `$__drop_<Type>(ptr)` runtime fn: read tag@slot0, switch,
  per ctor recursively `$__drop_<FieldType>` each variant field + `rc_dec` each leaf field, then
  free the block. (Or a generic `$__drop_variant` driven by an emitted per-type layout table.)
- A new `Op::DropVariant { v, type }` (or reuse the DropValue mechanism) that the render lowers
  to a call to that fn; the proven checker accepts it as ONE trusted `d` (like `$__drop_value`).
- Track a constructed recursive-variant value so its scope-end drop emits `Op::DropVariant`.
- Verify: the `Expr` to_string byte-match AND a create+drop leak loop AND corpus-wall ACCEPT
  (ownership) — the checker must accept the recursive `d` as leak/double-free-free.

## Verification commands (run after every increment)

```
cargo build -q -p almide-mir --example render_program
# byte-match: almide run <f>  vs  render_program <f> | wasmtime
bash proofs/corpus-wall.sh        # MUST stay ACCEPT (all 3) — the soundness gate
bash proofs/diff-fuzz.sh 40       # generative v0==v1 (variant templates already present)
cargo test -q -p almide-mir       # no NEW failures (2 pre-existing json fails are unrelated)
```

When the `Expr` lever byte-matches with corpus-wall ACCEPT: add it to a deterministic wasmtime
cargo test (`crates/almide-mir/src/render_wasm/tests_part3.rs`, near
`custom_variant_*`) + a diff-fuzz recursive-variant template, mark bricks 5b/5c DONE in
`docs/roadmap/active/v1-adt-value-model.md`, commit (English, one line,
`Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`), and push `develop-v1` only when asked.

## Guardrails

- Branch `develop-v1`. NEVER `git checkout`/`restore`/`stash` files you didn't modify (other
  agents work concurrently); manual-Edit reverts instead.
- The `?? p03*.*` / other untracked files are not yours — don't touch them.
- Don't grow the §4.1 hand-written WAT runtime.
- If a step can't be made cert-sound, WALL it honestly and document — a wall beats a latent
  double-free. Partial-but-sound (e.g. 5a construct) is a real win; checker-rejected is not.
