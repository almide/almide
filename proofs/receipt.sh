#!/usr/bin/env bash
# Emit the RECEIPT (受領書) for the trust chain: run the verification and fold
# the checked facts into named claims, each with its evidence, STATUS, and
# honest scope. This is the tier-1 deliverable the done-definition names — a
# third party reads it, then re-derives every claim with `make verify-trust`.
# Honesty is the point: claims are marked proven / scoped / pending, never
# overclaimed (the hard rail).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1


pass() { "$@" >/dev/null 2>&1 && echo PASS || echo FAIL; }

# SAME-TREE RE-USE. `make verify-trust` runs check.sh + gate.sh +
# structural-wall.sh + `cargo test -p almide-verify` — a strict superset of the
# four verdicts below — and records the fingerprint of the tree+toolchain it
# verified. When that fingerprint still matches (the CI job's verify-trust step
# immediately precedes its receipt step), re-running them would re-derive the
# identical verdicts on the identical inputs. Fold them in instead.
#
# The honesty rail is the FINGERPRINT, not a timestamp or a flag: it covers the
# compiler binary, every toolchain the gates invoke, the commit, the content of
# tracked modifications, and every untracked file. Edit anything, switch a
# toolchain, or run this standalone on a tree nobody verified, and it does not
# match — so the third-party path (`git clone && make receipt`) verifies in full
# exactly as before. A receipt can therefore never claim PASS for a tree that
# was not actually verified.
#
# Every verdict below is about the STRUCTURAL leg — the wasm the build ships
# (#2760). No MIR example and no translation-validation test runs here: the
# incumbent renderer those described retired in #2761.
VERIFIED_STAMP="$ROOT/proofs/.verified-fingerprint"
REUSED=no
if [ -f "$VERIFIED_STAMP" ] \
    && [ "$(cat "$VERIFIED_STAMP")" = "$(toolchain_fingerprint "$ROOT")" ]; then
    REUSED=yes
    PROOF=PASS; GATE=PASS; SWALL=PASS; VTEST=PASS
else
    PROOF=$(pass "$ROOT/proofs/check.sh")             # kernel + coqchk + axiom audit
    GATE=$(pass "$ROOT/proofs/gate.sh")               # witness rows ⊳ proven checker, with drills
    SWALL=$(pass "$ROOT/proofs/structural-wall.sh")   # certified corpus: build, hash, three verdicts
    VTEST=$(pass bash -c "cd '$ROOT' && cargo test -q -p almide-verify")
fi

# ARTIFACT IDENTITY (#2760): structural-wall.sh built every certified fixture
# with `almide build --target wasm` and wrote the SHA-256 of each shipped file
# here; every certificate bundle pins its row and almide-verify recomputes it.
# The receipt names the file and ITS digest, so a reader can check a shipped
# .wasm against a row and the row against this receipt.
ARTIFACTS="$ROOT/proofs/structural-artifacts.sha256"
if [ -s "$ARTIFACTS" ]; then
    ART_ROWS=$(grep -vc '^#' "$ARTIFACTS" || true)
    ART_DIGEST=$(shasum -a 256 "$ARTIFACTS" | cut -d' ' -f1)
else
    ART_ROWS=0; ART_DIGEST=missing; SWALL=FAIL
fi
CERTIFIED=$(sed -n 's/^# certified: \([0-9]*\) of \([0-9]*\).*/\1 of \2/p' \
    "$ROOT/crates/almide-wasm/tests/golden/witness-fixtures.txt")
# The certified fixtures that still leak through trusted internals (#2755):
# rows of proofs/certified-leaks.txt, gated by `almide-gates certified-leaks`.
CERT_LEAKS=$(grep -cv '^#\|^$' "$ROOT/proofs/certified-leaks.txt" || true)

cat <<EOF
# Receipt — Almide v1 trust chain

Reproduce every line: \`make verify-trust\` (proof + gate + tests).
Trusted base & known-limitations: proofs/TRUSTED_BASE.md.

Verdicts below were $( [ "$REUSED" = yes ] \
  && echo "folded in from a \`make verify-trust\` of THIS EXACT tree and toolchain (fingerprint match; see proofs/lib/stamp.sh)" \
  || echo "derived by running check.sh, gate.sh, structural-wall.sh and the almide-verify tests just now" )\
.

| claim | meaning | status | evidence | scope (honest) |
|---|---|---|---|---|
| C-PROVEN | the checkers' soundness rests only on the Coq kernel | ${PROOF} | the checkers these theorems make sound (\`check_xc\`, \`check_names_cert\`, \`check_caps_cert\`, \`check_prog_cert\`, \`check_modes_cert\`) are the ones that judge the STRUCTURAL leg's witnesses in C-SAFE, whose bundles pin the shipped bytes listed in proofs/structural-artifacts.sha256 (${ART_ROWS} rows, sha256 ${ART_DIGEST}); and the structural runtime's own RC routines are proven bytes → instruction trees → modeled transitions → whole runs (\`StructuralDecode\` / \`StructuralRuntime\` / \`StructuralAlloc\` / \`StructuralRun\`, grounded by check-structural-bytes.sh). proofs/check.sh: the flight-grade property set on the value-semantics subset — RC balance + membership-subset law (name totality + capability bound) + type concretization + memory-model leak-freedom (RuntimeModel) + reuse soundness (\`check_reuse_sound\`: a Reuse acts only on a uniquely-owned object) + free-list reuse-safety (\`FreeList.alloc_not_live\`: a valid allocation never returns a currently-live block — no reuse-after-free) + copy-on-write alias-safety (\`CowSafety.make_unique_yields_unique\`: MakeUnique yields a uniquely-owned block — no aliased in-place mutation) + byte-binding table (Translation) + the emitted \`\$rc_dec\`/\`\$rc_inc\` instruction trees realizing rt_dec/rt_inc (\`WasmRcDec\`) + the rc_inc instruction tree encoding to the REAL wasm bytes (\`WasmEncode\` — since #2761 a theorem about a MODELED runtime, the retired incumbent renderer's; the bytes that ship are the structural runtime's, grounded by check-structural-bytes.sh against \`StructuralDecode\`) + those bytes EXECUTING to rt_inc on a wasm stack machine + the FULL \`\$rc_dec\` bytes' SAFETY — no double-free AND leak-freedom — executed on the modeled runtime's bytes by a general interpreter with locals/globals/structured-if (\`WasmExec\`, grounded vs wat2wasm) + operand-stack balance (StackBalance) + termination of the loop-free fragment (Termination) + free-list REGION-RESET safety (\`FreeList.region_window_reuse_safe\`: a RegionSave/RegionRestore window preserves the allocator invariant across an arbitrary body, so the frontier reset leaves no free-list entry pointing into the reclaimed region) + PINNED_RC immortality (\`FreeList.pinned_stays_immortal_forever\`: an \`\$alloc8\`/\`__alloc_pinned\` block is never freed, never on the free-list and never returned by \`\$alloc\`, over an arbitrary run) + the COUNT side of reuse (\`FreeListRc.reuse_hands_back_a_zero_count_block\`: a block handed back off the free-list carries no stale reference count, and \`reuse_restores_rc_1\`: the \`\$alloc\`+constructor PAIR leaves it at exactly RC_INITIAL = 1, so \`double_release_traps\` — a re-release of a freed block hits the rc-0 sentinel); 96 audited theorems, \`Print Assumptions\` = Closed under the global context, coqchk re-checked | full (for the proven theorems; subset-scoped) |
| C-SAFE   | no double-free / use-after-free; no dangling reference; no undeclared host effect — in the wasm the build ships | ${GATE} / ${SWALL} / ${VTEST} | the STRUCTURAL leg (the only \`--target wasm\` renderer since #2761) emits every witness per build and the kernel-proven checker judges it: (1) ownership — recorded at EMISSION time, one event per RC instruction emitted (crates/almide-wasm/src/witness.rs), \`check_xc\` / \`check_all_sound\` format v5 incl. branch frames, loop activations and \`x\` exits, plus the program's call-mode witness (\`check_modes_cert\`) so the per-frame certificates compose across call sites; (2) name totality — \`check_names_cert\` / \`check_names_cert_sound\` per index space of the stock-WASI bytes and per function's locals (crates/almide-wasm/src/cert_project.rs); (3) capability bound — \`check_caps_cert\` per source-declared function and \`check_prog_cert\` over the SCC-condensed call graph (the reach computed in the proof). proofs/structural-wall.sh runs all of them over the certified corpus (${CERTIFIED} spec/wasm_cross fixtures) through THREE verdicts — the extracted checker, the Rocq kernel, almide-verify — and proofs/gate.sh carries a row and a tamper drill per property. ARTIFACT: each fixture is built with \`almide build --target wasm\`, the producer refuses unless that file is byte-for-byte the module its witnesses describe, and the bundle pins the file's SHA-256 (bundle v2 \`artifact\`); almide-verify recomputes it and REJECTS a mismatch (a flipped byte is drilled). Digests: proofs/structural-artifacts.sha256, ${ART_ROWS} rows, sha256 ${ART_DIGEST} | **the certified set only**: ${CERTIFIED} fixtures carry a witness on every frame of their shipped pass; every other frame DECLINES with a counted reason (crates/almide-wasm/tests/golden/witness-declines.txt), never under-records — a program outside the set carries no C-SAFE claim. Certified frames exclude trusted native arms and runtime helpers; ${CERT_LEAKS} certified fixtures still leak through them, tracked in proofs/certified-leaks.txt (blocks live at exit and the owning OPEN issue per fixture, shrink-only, gated by \`almide-gates certified-leaks\`). TRUSTED, not proven: that the recorder's events are the RC instructions the bytes contain (the recording discipline links them, not a decoder); the projector's \`fs_call\` op→capability table and its constant-slot reading; the source declaration table; and \`to_wasi\` (names are judged over the shipped bytes, ownership and capabilities over the module before it). Capabilities: console, entropy, env/argv, fs read, fs write, clock, stdin, network, foreign imports — a plain \`fn\` is bounded by the console (output, and the stdin byte readers io.almd declares plain), an \`effect fn\` by every modeled capability (vacuous for it without a manifest). Memory indices are validator-checked, not witnessed |
| C-FAITHFUL | the emitted artifact refines the ALS model | partial | the structural leg's RC runtime: the SHIPPED bytes of \`\$inc\`/\`\$dec_flat\`/\`\$free\`/\`\$alloc\` decode to instruction trees (\`StructuralDecode\`, grounded against the emitter's own dump by proofs/check-structural-bytes.sh, no hand-copied constants), those trees realize the modeled RC / free-list transitions (\`StructuralRuntime\`, \`StructuralAlloc\`), and whole runs of them preserve the heap invariant with no aliased handout (\`StructuralRun\`). The op→instruction table (\`Translation\`) and \`WasmRcDec\`/\`WasmEncode\`/\`WasmExec\` are theorems about the RETIRED incumbent renderer's modeled runtime (#2761) and are not re-derived for the structural one | COVERED: the four runtime RC routines, bytes to semantics. NOT covered: that each program function's instruction stream calls them exactly where its ownership witness records an event (trusted recording discipline, see C-SAFE); type-specific drop glue and every other emitted body; full-module semantics against ALS. Observable behaviour of the structural build is MEASURED, not proven — byte-identical stdout/stderr/exit against native and the interp oracle (run manifest, output-parity) — evidence, not refinement |
| C-WALL   | the lowering boundary is a WALL over the real corpus, not a hole | ${SWALL} | the structural leg is the only wasm route (\`almide::wasm_route\`, #2752): a program it declines is a named E082 wall, never a hand-over to another renderer; the census of programs the router once handed to the incumbent (#2741) reached 0 rows and retired. The walled spec/wasm_cross fixtures are enumerated and equality-pinned (proofs/determinism-walled-baseline.txt), and for the certificate a frame outside the witnessed subset DECLINES with a counted reason (witness-declines.txt, shrink-only per reason) rather than being recorded partially. proofs/structural-wall.sh re-verifies the certified set (${CERTIFIED}) on every property through three verdicts | **measurement + wall**, not a completion claim: coverage is the certified-fixture count above (grow-only, crates/almide-wasm/tests/golden/witness-fixtures.txt) and the decline histogram is the roadmap (#2755–#2758). Totality + acceptance is NOT an output-correctness claim; output correctness is output-parity's, on its baseline set |
| C-REPRO  | byte-reproducible across hosts | CI-gated | the STRUCTURAL leg (the default \`--target wasm\` renderer, #2753): scripts/check-host-determinism.sh (compiler built natively vs wasm32-wasip1) and scripts/check-browser-determinism.sh (vs wasm32-unknown-unknown, the playground's target) emit every spec/wasm_cross fixture through \`almide::wasm_route\` forced structural and demand byte-identical output (the structural module + its stock-WASI form) | the fixtures the structural leg declines are not measured: 5 of 802, named in proofs/determinism-walled-baseline.txt with their cause issues (shrink-only, equality-pinned); the incumbent renderer's reproducibility is no longer measured (it is retiring, #1696) |

Irreducible base (cannot be proven, named in TRUSTED_BASE.md): Coq kernel,
OCaml extraction (CertiCoq/CompCert will close it), hardware, ALS validity.
Completeness is relative to the declared use; absolute-semantics coverage is
NOT claimed.
EOF

# ── G-F4: the reference app, app-scoped (#776) ──────────────────────────
# Every line DERIVED from the tree right now (greps over committed
# artifacts), never asserted. The app is the flight-profile PID control
# kernel; its cross-target byte identity is RATIFIED state (the
# run-manifest row), it is in the certified set the structural wall sweeps
# for the C-SAFE row (every frame witnessed, its artifact hashed), and the
# Ferrocene leg is the weekly lane over the same manifest.
APP="spec/wasm_cross/flight_pid_control.almd"
APP_FAIL=0
app_row() {
  if eval "$2" >/dev/null 2>&1; then echo "| $1 | PASS |"; else echo "| $1 | FAIL |"; APP_FAIL=1; fi
}
{
echo
echo "## Reference app (G-F4, #776): the flight PID control kernel"
echo
echo "App: \`$APP\` — Q16.16 fixed-point PID + saturation + anti-windup under a"
echo "counted loop (contract C-230). Stages, each derived from the tree:"
echo
echo "| stage | status |"
echo "|---|---|"
app_row "runs (fixture present, contract header C-230)" "grep -q '@contract: C-230' $APP"
app_row "oracle byte-match ratified (run-manifest row)" "grep -q $APP crates/almide-spine/tests/golden/spec-run-manifest.txt"
app_row "contract ledger row (C-230 present)" "grep -q 'C-230' docs/contracts/contracts.toml"
app_row "certificates issued (in the certified set the C-SAFE row sweeps)" "grep -qx $APP crates/almide-wasm/tests/golden/witness-fixtures.txt"
app_row "readable Rust + traceability (--trace-map, #572)" "grep -q trace_map src/cli/emit.rs"
app_row "Ferrocene leg (in the weekly lane's manifest corpus, #573)" "grep -q $APP crates/almide-spine/tests/golden/spec-run-manifest.txt"
echo
echo "The WCET story for the Critical shape (the C-WCET keystone's documented"
echo "half) is docs/project/WCET-STORY.md (#569); the per-loop certificate"
echo "witness remains future work and is NOT claimed here."
}
if [ "$APP_FAIL" -ne 0 ]; then
  echo "receipt: a reference-app stage above is FAIL" >&2
  exit 1
fi

# The receipt is honest only if it can go RED: a rendered FAIL in the verdict
# table must also fail the process that produced it — before this guard the
# script's exit was the heredoc's, so CI's "make receipt" step stayed green
# around a FAIL table (#984). The fingerprint-reuse branch is all-PASS by
# construction; this bites only the derived path.
case "${PROOF}${GATE}${SWALL}${VTEST}" in
  *FAIL*) echo "receipt: a verdict above is FAIL — refusing to exit green (#984)" >&2; exit 1 ;;
esac
