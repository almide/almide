<!-- description: PC-switch handoff: porta wall=0 + read_message cross-module on v1 (steps 1-3 done) -->
<!-- done: 2026-07-02 -->
# Handoff — porta wall=0 + read_message cross-module on v1 (session record)

> **Branch `develop-v1`. Written for a PC switch — pick up from here.** All code is committed
> (see commits below); the only uncommitted path is `tools/xtarget-fuzz/Cargo.lock` (a build
> artifact, NOT this session's — do not commit it). Nothing is pushed yet (push is the user's call).

## Goal (Stop-hook)

> docs/org-trust-status.md の per-repo の wall を 0 にすること、crossmodule想定でも動くようにすること.

**Both conditions reached** for the goal's metric: 17/17 org repos at lowering wall=0, and porta's
`read_message` runs **cleanly cross-module on v1** (byte-matches native, no trap).

## What landed (commits on develop-v1, newest first)

- `71364ec9` — dashboard notes: porta is a native host; read_message = first cross-module byte-match milestone.
- `7a9b94f0` — regenerate `docs/org-trust-status.md`: **17/17 repos at lowering wall=0** (porta was the last).
- `81840f8d` — **effect-fn control-flow-tail return-type fix** (the cross-module clean-run fix; see below).
- `949cd0cb` — **io.read_n_bytes WASI stdin-N-bytes floor** (porta's last lowering wall: read_message).

## The two fixes (mechanism, so they can be extended/audited)

### 1. io.read_n_bytes (949cd0cb) — porta's last LOWERING wall
read_message needs `io.read_n_bytes(n) -> List[Int]` (stdin N bytes). Added as a sibling of
`io.read_line` across: `PrimKind::ReadNBytes` (lib.rs), render `(call $read_n_bytes …)`
(render_wasm_p2), the `$read_n_bytes` WAT helper = chunked `fd_read` loop building a `List[Int]`
via `$list_new`/`$list_set` (render_wasm_p3), Ptr-repr (render_wasm.rs), Stdin cap + `i` cert
(certificate.rs), `read_n_bytes` lowering branch (calls_p4), whitelist (calls.rs), `io_read_n_bytes.almd`
self-host + registry (render_wasm/registry.rs), `prim.read_n_bytes` sig (stdlib/prim.almd), IMPURE_PLAIN
(check-stdlib-purity-registry.sh), WASI_FLOOR_FNS (render_wasm/tests_part3.rs). Plus a render_program
fix: **prepend value_core source when the generated drop helpers reference `__drop_value`** (a
Value-field record's `$__drop_<R>` calls the value_core internal `__drop_value`, undefined at the
drops re-lower's type-check — porta's `JsonRpcRequest { id: Value }`).

### 2. effect-fn control-flow-tail return type (81840f8d) — the cross-module CLEAN-RUN fix
**Root cause (NOT the RC double-free I wrongly chased for ~80 turns):** an `effect fn main() ->
Result[T,String]` whose body tail is a `match`/`if` had `body.ty = Unit` (the inner T), not Result —
the frontend types a control-flow tail by its arm payloads, and `pass_result_propagation` SKIPPED it
(main's ret is ALREADY Result, so it wasn't in `lifted_fns`). With body.ty ≠ ret_ty, `emit_wasm`
(`functions.rs:177`) emits a trailing `unreachable` that the fall-through reaches → trap AFTER the
correct output. A bare `ok()` tail (no control flow) kept its Result ty and ran fine — that asymmetry
was the tell. Fix in `pass_result_propagation.rs`: (a) `wrap_tail_in_ok`'s Block/If/Match cases type
the result as the WRAPPED sub-expr's ty (not `Result[pre_ty]`, which double-wraps / mis-types when an
arm is already a Result — and does NOT mask an `err()` arm); (b) new "Phase 2b" re-runs the tail-ty
fix on effect fns whose ret is already Result but whose body.ty is not (the `main` case Phase 2 skipped).
Verified clean on Rust target too (ResultPropagation runs after BorrowInsertion/CloneInsertion).

## Gates — ALL GREEN at this state
- corpus-wall: WALL OK (4523 fn TOTAL), ownership/names/caps/caps-transitive ACCEPT, FORBIDDEN 0.
- output-parity: 126/126 byte-match v0.
- proof-spine: PROOF SPINE OK (axiom-clean).
- cargo test -p almide-codegen: 107/0.
- `wasm_cross_target_spec` ok (all spec/wasm_cross byte-match native==wasm); integration 0 FAILED.
- `almide test spec/`: **272/272** (262 via WASM).
- 6 pre-existing `cargo test -p almide-mir` failures (record-materialization WIP) are NOT this session's.

## ⚠ porta does NOT run as a full app — and that's two SEPARATE things from the v1 work

1. **porta is a NATIVE HOST.** `porta/almide.toml`: `[native-deps] wasmtime + reqwest`, `[permissions]
   Net`. WASI preview1 has no net and can't embed wasmtime → the full MCP server is native-only by
   design. Only porta's **portable protocol layer** (jsonrpc / config) is in the v1 subset; the 25
   native-FFI in the dashboard are its host calls. "porta wall=0" = that portable layer lowers.
2. **porta's NATIVE (v0) build is currently BLOCKED on develop-v1 by the `toml` dependency** —
   `almide build porta/src/mod.almd` (native) fails with **52 generated-Rust errors** (E0308 borrow/
   clone, e.g. `almide_rt_toml_v0_collect_dotted_keys(t, …)` passes `&str` where `String` is expected).
   This is a **develop-v1 borrow/clone codegen issue with the toml dep's specific patterns** (dep =
   github.com/almide/toml @14db4aba). **CONFIRMED NOT this session's regression**: a minimal same-shape
   recursive `effect fn -> Result` passing a String param through recursion + a control-flow tail
   builds + runs native fine here, all gates green, and this session's changes are wasm-side
   (io.read_n_bytes) or run AFTER borrow/clone (the effect-fn-tail fix).

So: **read_message (porta's v1-portable core) works on v1** (cross-module byte-match), but the **full
porta app can't run on v1** (native host) and its **native v0 build is blocked by the toml-dep codegen
bug** on this branch.

## Reproductions (recreate on the other PC — the originals were in a session-local scratchpad)

- **read_message cross-module on v1** (the win): two files in one dir —
  - `rpclib.almd`: `import io` + `type Msg = { len: Int, body: String }` +
    `effect fn read_msg() -> Result[Option[Msg], String] = { let line = io.read_line(); let n =
    string.len(line); if n <= 0 then ok(none) else { let bytes = io.read_n_bytes(n); ok(some({ len: n,
    body: string.from_bytes(bytes) })) } }`
  - `xmain.almd`: `import io` + `import rpclib` + `effect fn main() -> Result[Unit, String] = { let m =
    rpclib.read_msg()!; match m { some(msg) => { io.print("len=${int.to_string(msg.len)} body=${msg.body}");
    ok(()) } none => { io.print("none"); ok(()) } } }`
  - `printf '5\nhELLO' | almide run xmain.almd` and `… --target wasm` → both `len=1 body=h` (byte-match).
- **the fixed trap** (R3, single file): `import io` + `effect fn main() -> Result[Unit,String] = { let
  o: Option[Int] = some(5); match o { some(r) => { io.print("some"); ok(()) } none => { io.print("none");
  ok(()) } } }` → was "some" then trap on wasm; now `some` both targets.
- **porta native blocker**: `cd porta && almide build src/mod.almd -o /tmp/p` → 52 toml-dep Rust errors.

## Next steps (prioritized) — for the other PC

1. **(If porta-on-native matters) Fix the develop-v1 toml-dep borrow/clone codegen regression** (the
   52 `&str`/`String` errors). It's the concrete blocker to porta building at all on this branch.
   Separate from v1 trust-spine; a CloneInsertion/BorrowInsertion gap for the toml dep's patterns
   (the minimal recursive-effect-fn shape works, so it's a more specific pattern — inspect the
   generated Rust + the toml source around `collect_dotted_keys`).
2. **Widen byte-match verification** (the dashboard's own `wall=0 ≠ correct` caveat): the 🟡 repos
   (toml/svg/rsa/porta/csv) lowered but are not byte-verified. Run each repo's vectors native vs
   `--target wasm`, byte-diff, add the clean ones to `BYTE_VERIFIED` in scripts/org-trust-status.sh.
3. **App run-rate (north-star: v0-obsolete)**: the effect-fn-tail fix was a GENERAL unblock (any
   effect-main with a control-flow tail) — re-measure the real-app run-rate (it likely moved) and
   attack the next blocker.
4. **(Optional) MIR trust-spine render parity**: read_message runs on the CLI (`emit_wasm`), not yet
   on the VERIFIED render_program path (needs `json.parse/object/stringify` wasm self-host = a wasm
   JSON codec). Only needed if you want read_message on the kernel-proven path, not just production.

## Pointers
- Dashboard: `docs/org-trust-status.md` (17/17 wall=0, porta notes).
- Memory (if synced): `project_v1_read_message_run_landscape`, `project_v1_trust_spine`, `project_org_trust_sweep`.
- The push of develop-v1 is pending the user's word.

---

## Completion record (2026-07-02, follow-up session)

Steps 1–3 of "Next steps" are DONE on `develop-v1`:

1. porta's native build: 52 errors → 0 (the "toml-dep" attribution was wrong — see
   [v1-org-byte-verification.md](../active/v1-org-byte-verification.md) for the real
   decomposition and fixes). Full porta test suite green on both targets.
2. Byte-match verification: every org repo WITH a test suite now passes both
   `almide test --target native` and `--target wasm` in full (14 repos), after six
   wasm bug classes were fixed (contracts C-121..C-125).
3. Run-rate: superseded by the both-targets suite sweep — the per-repo state is in
   `docs/org-trust-status.md`.

Step 4 (read_message on the VERIFIED render_program path) remains open and is
tracked in v1-org-byte-verification.md.
