# Blueprint — making the ~27 effectful / raw-pointer stdlib fns FUNCTIONAL in v1

Design-only (the PLAN, not a build). Produced by a 6-agent design fan-out + adversarial
review, grounded in the tree (2026-06-17). **Verdict: SOUND-with-fixes — safe to build
FROM, not verbatim.** The §7 review corrections are part of the spec.

> **Honesty rule (load-bearing):** the ~27 are NOT one problem and must never be a single
> "done" number. Three tiers, three verification ceilings. Only tier-1 may be called "proven".
> See [[../../../../.claude .../feedback_done_means_three_tiers]] (memory).

## 1. Three mechanisms (each its own ceiling)

| | adds | sandbox exit | ceiling |
|---|---|---|---|
| **(A) WASI host-import floor** | new `PrimKind` rendered as `(call $wasi_fn)` vs new `(import "wasi_snapshot_preview1" …)` in `preamble()`, mirroring the sole existing `fd_write` (`render_wasm.rs:890`); effectful bodies then self-hosted in Almide over them | YES | **declared-caps-safe** (runs + caps ACCEPT; output unprovable by nature) |
| **(B) Capability vocabulary** | grow `enum Capability` (`lib.rs:471`, today only `Stdout=0`) + `Capability::id` + Coq ids to `{Stdout,Fs,Net,Clock,Random,Env,Process}`; make `cap_witness`+`declared_caps` attribute honestly | n/a (it is the accounting for A) | proof-foundation |
| **(C) raw-pointer / mem** | one `repr_of` arm (`Ty::RawPtr => Word`); `bytes.*` ptr fns = `prim.handle(b)+12`; non-cap `prim.heap_mark/heap_reset` over `$bump` | NO (linear-mem address math) | **offset-functional** (round-trip; byte-equality inapplicable) |

**Empirical anchor (A):** v0's `emit_wasm/runtime.rs:14-145` ALREADY registers + runs the full
preview1 set (`fd_write, clock_time_get, random_get, proc_exit, fd_read, path_open, …`) under
the SAME wasmtime the v1 tests use → adopting them is *promotion of proven calls into the prim
floor*, not discovery.

## 2. Reachability reality (the hard honest limit)

The v1 runner is bare `wasmtime run <file>` — default WASI ctx, no `--dir`, no `--env`, stdio
inherited (`tests_part3.rs:525/:547`). So:

- **Reachable NOW:** stdout (done), `clock_time_get`, `random_get`, `proc_exit`, `fd_read` (stdin).
- **Reachable only after a runner change** (`--dir .` / `--env`): all `fs.*`, `env.get/cwd`,
  `process.env`. Blocked by *invocation*, not by wasm (mechanism proven by v0 `calls_fs.rs`).
- **BLOCKED under preview1 (the CURRENT target) — a 4th metric column, NOT "unimplemented":** all
  `http.*` (24), `net.*` raw TCP (12), SSE/LLM streaming (no `sock_connect`/outbound socket),
  `env.set` (`environ` read-only). ~36 fns. Under preview1 they are native-only. **NOT permanent:**
  [`wasm-platform-frontier.md`](wasm-platform-frontier.md) plans the WASI 0.2/0.3 + Component Model
  migration (`wasi-http` has a wasmtime host impl; `wasi:sockets` = capability-scoped network, which
  ties DIRECTLY to this blueprint's §1(B) capability vocabulary — see
  [`effect-system-capability.md`](effect-system-capability.md)). Under THAT target they become
  **declared-caps-safe** (tier-2). So they are "blocked under preview1, unblocked by the frontier
  roadmap" — and **must be surfaced as their own metric column (§5), never folded into the
  unimplemented count** (else "78 left" misreads as "78 doable" when ~36 need a target migration).
  This shrinks the preview1 "27→functional" reality: clock/random/exit/stdin now, fs/env after a
  runner flag, http/net only after WASI 0.2/p2.

## 3. Build order (SAFE-NOW vs NEEDS-FRESH)

`NEEDS-FRESH` = touches the Coq registry / `cap_witness` / `declared_caps` / `is_known_free` /
`repr_of` / the `$bump` invariant → full-attention + adversarial pass.

**Phase 0 — scaffolding & honesty plumbing**
- **B0** tier manifest `docs/stdlib/tier-manifest.toml` + `check-tier-manifest.sh` + `stdlib-tier-report.sh` (3 columns, never blended). **SAFE-NOW**
- **B1** populate `effect_map` (`frontend/lower/mod.rs:412` drops it today) per-fn direct/transitive `Effect` sets. **SAFE-NOW**
- **B2** split `Effect::IO` → Stdout vs Fs. **NEEDS-FRESH** (review #3: load-bearing for declared_caps; bind atomically with B6 — a populated-but-unsplit effect map → honest declared_caps = under-declaration = accept-but-unsafe).
- **B3** draft (un-linked) self-host bodies: `datetime.now/monotonic_ns`, `random.*`, `io.read_*`, + the **pure** `http` builders (`response/json/with_headers/req_*/query_params` — pure data, no cap, self-hostable immediately). **SAFE-NOW**

**Phase 1 — capability foundation (prerequisite gate, atomic)**
- **B4** Task #35: `Capability::id` single-source + generated/asserted Coq id block + CI drift gate (like `check-stdlib-purity-registry.sh`). MUST precede any id widening (mis-numbered id = silent accept-but-unsafe). **NEEDS-FRESH.** *(Review #9: this is foundation hygiene that also protects the PROVEN column — run in parallel with the strong-proof queue, not deferred behind it.)*
- **B5** capability vocab + caps-witness honesty — **atomic unit, NEEDS-FRESH:**
  - `lib.rs:471` extend `enum Capability` + `Capability::id` arms.
  - `proofs/CapabilityBound.v` enumerate new ids + adversarial `Example`s. **No theorem change** — `subset_check_sound` (`Subset.v`) is universal over `nat`.
  - `certificate.rs:167` `CallIndirect` taint: hardcoded `Stdout` → **`Capability::ALL`** (review #2: derive from a single `ALL` const; B4 drift gate asserts `CallIndirect taint == Capability::ALL`, else an 8th cap forgotten = un-witnessed closure).
  - `classify_corpus.rs:465` `is_known_free`: STOP blanket-trusting `n.contains('.')`; dotted ⇒ known-free **only if `purity::is_pure(module,func)`**. **(Review #1: this must land BEFORE B7 — the moment a capability-bearing dotted op exists, `contains('.')` waves `datetime.now` through as caps-clean = accept-but-unsafe. The `is_known_free` narrowing + the first capability op are ONE atomic unit. Also applies to BOTH fold closures — `reaches_capability_or_unknown` AND `reachable_caps_or_tainted`.)**
  - atomically reword "no undeclared **Stdout** effect" → the full set in `TRUSTED_BASE.md` + `corpus-wall.sh` + comments (#34 claim-drift gate).
- **B6** honest `declared_caps`: replace `lower/mod.rs:128` `if is_effect { [Stdout] }` with `transitive Effect → Capability` map. Depends on B1+B2. Under-declare = unsafe; over-declare = breaks parity. **NEEDS-FRESH.**

**Phase 2 — reachable-now host prims** (each follows the `fd_write` chain: `prim.almd` decl → `lower_prim_call` arm → `PrimKind` variant → render arm → `preamble` import → `cap_witness` arm). All **NEEDS-FRESH** (each carries a capability):
- **B7** `ClockTimeGet{clock_id}` → `Capability::Clock` — `datetime.now/monotonic_ns`, `env.unix_timestamp/millis`. Tier-2.
- **B8** `RandomGet` → `Capability::Random` — `random.int/float/choice/shuffle`. Tier-2.
- **B9** `ProcExit` → `Capability::Process` — `process.exit -> Never`. Tier-2.
- **B10** `FdRead{fd}` → `Capability::Stdin` — `io.read_*`, `process.stdin_lines`. **Review #6: needs a declared non-overlapping scratch-region constant table FIRST** (the read out-buffer must not collide with the print iovec@8 / itoa scratch under interleaved `println(int.to_string(read_line()))` — else silent wrong output, worse than a trap). Tier-2.

**Phase 3 — raw-pointer / mem**
- **B11** `bytes.data_ptr(b)->Int = prim.handle(b)+12` — **SAFE-NOW** (zero cert/cap/ownership event; idiom already shipping in `bytes_core.almd`; returns Int not RawPtr so no B12). **BUT tier-3 OFFSET, not a freebie that raises any proven count** (review #4: hardcodes `LIST_HEADER=12` ≠ v0's +4 → cannot byte-match; the round-trip test only proves v1's own layout).
- **B12** `repr_of` `Ty::RawPtr => ScalarWidth::Word`. **NEEDS-FRESH** (first new arm in this subset; adversarial pass: a RawPtr Scalar never masquerades as a heap value in `verify_ownership`).
- **B13** `as_ptr/as_mut_ptr -> RawPtr` (same body as B11, RawPtr return → needs B12). Tier-3.
- **B14** `from_raw_ptr`/`copy_to_ptr` — first RawPtr-PARAM path (`bind_params` slots it scalar, never a heap borrow). Tier-3 UNSAFE-contract.
- **B15** `heap_mark/heap_reset` + `mem.save/restore` + `bytes.heap_save/restore` — **HARD; review #5: NOT a stdlib-reachable prim under the current proof.** `heap_reset` below a mark bypasses `$rc_dec` + corrupts `$freelist`; the ownership proof is BLIND (a live ref surviving `restore` = accept-but-unsafe use-after-free in the SAME binary). **Default: DEFER entirely until an arena-escape-freedom proof exists** (not "gated UNSAFE-floor-only" — that still puts an accept-but-unsafe prim in the verified surface).

**Phase 4 — runner unblock / blocked**
- **B16** runner `wasmtime run --dir . --env …` — **NEEDS-FRESH (deliberate):** silently changes the sandbox the whole proof is about; highest single unblock (enables all `fs.*` + `env.get/cwd` + `process.env`). Without it, fs prims compile+validate then trap `ENOTCAPABLE` at runtime = silent parity break vs v0 native.
- **B-BLOCKED:** `http.*`/`net.*`/`env.set` — document wasm-unsupported / native-only (§2).

## 4. Soundness story (why vocab expansion KEEPS "no undeclared effect")

The 4th property `within_bound = subset_prop allowed used` is defined over `nat` lists in
`Subset.v`, **universally quantified, no capability enumerated**. So adding ids generalizes the
proof **for free** — `subset_check_sound` is byte-identical; the only Coq edits are the registry
comment + non-load-bearing `Example`s. **The entire burden moves to the UNTRUSTED emission side:
`cap_witness` must OVER-approximate real host effects** (every host reach contributes its id to
`used`). Discharged by: the exhaustive `RtFn::capability` match (a forgotten variant = compile
error) + each host-prim `cap_witness` arm + the `Capability::ALL` `CallIndirect` taint + walling-
or-cap-emitting every effectful `CallFn` path. The single break = a host op reaching a cap without
contributing its id → vacuity for that id (accept-but-unsafe), entirely emission-side. That is why
B4+B5+B6 are atomic NEEDS-FRESH, and a sandbox-exit prim WITHOUT a matching cap + witness is
forbidden. Facet C carries NO capability → cannot weaken this property at all (its risk is the
ownership/RC proof, B15, a separate invariant).

**★ FRESH-BUILD ADVERSARIAL Q1 (pinned — the SINGLE soundness-critical question for every brick in
this plan):** *"Does the emitter UNDER-declare any capability anywhere — i.e. is there a path that
touches a host effect (fs/net/clock/random/…) but contributes NO id to `used`?"* Because soundness
now rests entirely on `cap_witness` OVER-approximating, **over-declaration (conservative) is always
safe; UNDER-declaration is the only accept-but-unsafe hole.** So every fresh adversarial pass starts
here, and the concrete under-declaration vectors to refute are exactly: (1) a new host-prim with no
`cap_witness` arm; (2) a dotted effectful `CallFn` waved through by `is_known_free` (B5); (3) a
`CallIndirect` taint narrower than `Capability::ALL` (B5/B4); (4) `declared_caps` mapping an effect
to fewer caps than it reaches (B6/B2); (5) a forgotten `RtFn::capability` variant (compile-error,
so structurally closed). If none of these under-declares, the brick is sound — the OTHER properties
(ownership/names) are unaffected by capability work except B15 (separate invariant).

## 5. Honest 3-tier metric (resolves "100% needs redefining")

```
STDLIB FUNCTIONAL COVERAGE  543 / 621
  ├─ PROVEN (byte-matches v0)               : 543  ← the ONLY column any claim may call "proven"
  ├─ DECLARED-CAPS-SAFE (runs, caps⊇used)   :   0
  ├─ OFFSET-FUNCTIONAL (ptr round-trip)     :   0
  ├─ BLOCKED-IN-WASM-preview1 (native-only) : ~36  ← http/net/env.set; needs WASI 0.2/p2 (frontier)
  └─ unimplemented (buildable on this target):  ~42
```
The **4th column is load-bearing honesty**: the ~36 are NOT "left to do on this target" — they need
the WASI-0.2 migration (`wasm-platform-frontier.md`). Hiding them in `unimplemented` would let "78
left" read as "78 doable", which is the same over-claim the 3-column split forbids — apply the
physical-separation discipline to the WALL too. `check-tier-manifest.sh` (B0) computes this column
from a `blocked_target = "preview1"` field, so the wall is a number, not prose.
- **Tier-1 PROVEN** — a render→wasm→wasmtime byte-match `#[test]` (`assert_eq!(out, <v0 golden>)`).
  **Review #7: `check-tier-manifest.sh` must verify the cited test CONTAINS a v0-golden byte
  assertion (grep `assert_eq!`+golden marker), not merely that a `#[test]` of that name exists** —
  else a smoke test mislabeled `proven` reaches the cardinal sin (works-reported-as-proven).
- **Tier-2 DECLARED-CAPS-SAFE** — caps ACCEPT (`reachable ⊆ declared`, reuse the gate) + a runs-
  without-trap smoke (exit-0, **nothing asserted about stdout bytes**). Claim = "does only what it
  declares," NOT "produces X".
- **Tier-3 OFFSET-FUNCTIONAL** — a layout-independent functional law (`load8(store8(a,v))==v`),
  never literal byte-equality.

**Redefined goal:** **"621/621 FUNCTIONAL across three tiers — ~594 of them PROVEN."** The headline
number is the PROVEN sub-count. ~594 is a *projection* (self-corrected by the tier-report script),
NOT an asserted achievement. **No API or doc may emit a blended "621 proven."**

## 6. Smallest genuinely-safe first step
**B11** (`bytes.data_ptr`) — the ONE item touching no proof/gate/cap/foundation. But tag it
**OFFSET** (tier-3), not a proven freebie. Everything else is NEEDS-FRESH behind the strong-proof
queue (§ priority).

## 7. PRIORITY — this is the PLAN, not a queue jump
This work's ceiling is tier-2/3 (weak proof). **Strong-proof essentials come FIRST** — they each
land as PROVEN and raise the headline: `string.split` (List[String] build), `list.map/filter/fold`
(higher-order self-host), **#60 StringInterp** (core language surface). Schedule §3 AFTER them.
Exception: **B11** opportunistic; **B4** (review #9) runs in PARALLEL (it protects the proven
column too), not deferred.

## 8. Review must-fixes (folded into §2–§5 above)
1. `is_known_free` narrowing must land BEFORE B7 + cover BOTH fold closures (else dotted cap call = accept-but-unsafe). 2. `CallIndirect` taint = `Capability::ALL` (derived) + B4 asserts coverage. 3. B2 relabelled NEEDS-FRESH, atomic with B6. 4. B11 is tier-3 OFFSET, not "safe and done". 5. B15 = DEFER (not UNSAFE-floor-only). 6. B10 needs a static scratch-region map first. 7. tier check verifies the byte-assertion, not just test existence. 8. `process.args` differs test-runner vs `almide run --` (production parity). 9. B4 hoisted to parallel-with-strong-proof.

## env.args v1 — validated design + the entanglement (2026-06-25)

env.args is the WASI floor's FIRST I/O capability for v1. It MIRRORS the proven random.int mechanism (the ONE admitted effectful call today): self-host stdlib fn → a `prim.*` op that carries a Capability → transitive cap_witness → `used ⊆ declared` cert verification → render emits the WASI call. The exact pieces:
1. `Capability::CliArgs` (lib.rs:599 enum; the Coq registry mapping lib.rs:614 must stay injective+stable: Stdout=0, Entropy=1, CliArgs=2) — the FORMAL trust-spine addition.
2. `PrimKind::ArgsSizesGet` + `PrimKind::ArgsGet` (lib.rs:443 region) carrying `Capability::CliArgs` (cert accounting at certificate.rs:178, mirroring random_get→Entropy).
3. `stdlib/env_args.almd` — self-host `env_args() -> List[String]`: `prim.args_sizes_get(argc_p, bufsize_p)` → alloc argv+buf → `prim.args_get` → loop the argc null-terminated strings into a `List[String]` (the buffer parsing — more involved than random.int's single i64 read).
4. render_wasm WASI runtime for the 2 prims (render_wasm HAS the WASI floor — random.int's random_get works on v1).
5. calls.rs:187 admit `env.args` (extend the `is_admitted_effectful` predicate).

🚨 ENTANGLEMENT (why this is bigger than a clean capability add): the BYTE-MATCH ORACLE is broken. `almide run --target wasm -- file.csv` does NOT forward the `--` args to the wasm execution (env.args() returns EMPTY → csv-to-json prints "usage" instead of the JSON) — the args plumbing is broken EVEN in the full emit backend. And the wasm_cross harness runs pure fixtures only (no program-args support). So before env.args can be cert-byte-verified, the args plumbing (CLI runner forwarding + the harness passing identical argv to native and wasmtime) must be fixed — that is the genuine FIRST step of "段階的に", a CLI/test-infra brick separate from the trust-spine capability work.

SEQUENCING: (a) fix the args plumbing + wasm_cross harness args support (the byte-match oracle), THEN (b) the env.args capability mirroring random.int (steps 1-5). csv-to-json ALSO needs fs.read_text (path_open+fd_read self-host + Capability::FsRead), and almide-grep + fs.walk — each its own capability brick. The full WASI file-I/O floor for these two apps is ~3-4 capability bricks + the infra. Recommended as a focused fresh effort (delegate+gate per capability).
