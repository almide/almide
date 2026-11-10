#!/usr/bin/env bash
# NATIVE WALL (#4006): the certificate of every program the verified native
# renderer renders, judged per build by the kernel-proven checker.
#
# `almide build --target rust` renders a program through the v1 trust spine
# (almide_mir::pipeline::try_render_rust_source) and falls back to the
# standard codegen on a wall. proofs/native-verified-floor.txt lists every
# spec/wasm_cross fixture that renders (grow-only, held by
# tests/native_verified_floor_test.rs). Before this gate, a rendered
# function's ownership was checked only by `verify_ownership`, the Rust
# mirror inside the renderer. Here, per fixture, the producer
# (crates/almide-mir/examples/emit_native_bundle.rs) writes:
#   - the rendered Rust source, and
#   - a version-5 certificate bundle: the ownership witness of EVERY function
#     the renderer rendered, taken from the exact MIR it rendered (after the
#     probe charges, the result-window rewrite, the make-unique elision and
#     the move-param pass), the program's `native-calls` witness (#4006 class
#     P, proofs/NativeCalls.v: each function's heap-param modes, each call
#     site's modes agreeing with its callee's, each moved argument bound to
#     the caller's move-out), with the source's SHA-256 pinned.
# Three verdicts must agree on every witness: the checker extracted from the
# Coq proof (proofs/checker), the Rocq KERNEL (a generated file of
# vm_compute goals) and almide-verify, which also recomputes the pinned
# digest. A drill flips one byte of a rendered source (almide-verify must
# read MISMATCH), and the POISON LEG requires every
# proofs/poisoned-certs/native/*.cert — the witness the renderer writes with
# a release lost and its Rust mirror bypassed — and every
# proofs/poisoned-certs/native-calls/*.cert — the call binding written with a
# lying call-site mode or a missing move — REJECTED by all three.
#
# What this certifies is the MIR the source was rendered from (RC balance,
# OwnershipChecker.check_xc). TRUSTED: that rustc compiles the rendered
# source into a binary that keeps it (`Dup` as `.clone()`, `Drop` erased to
# Rust's scope-end drop), and every program outside the floor, which ships
# from the standard codegen with no certificate. A region window (#4006 C2)
# is witnessed as its arena mark (the save's `i`, the rewind's `d`); that a
# twin node's release may wait for the rewind is the region-pure verdict,
# interim trust (docs/contracts/proven-vs-trusted.md).
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")"
ROOT="$(cd .. && pwd)"
FLOOR="$ROOT/proofs/native-verified-floor.txt"
CANDIDATES="$ROOT/proofs/native-v1-perf-candidates.txt"
COQC="${COQC:-$(command -v coqc || true)}"
[ -n "$COQC" ] || { echo "NATIVE WALL FAIL: coqc is not on PATH (the kernel verdict needs it)" >&2; exit 1; }

echo "== build the producer, almide-verify and the kernel-proven checker =="
(cd "$ROOT" && cargo build -q -p almide-mir --example emit_native_bundle && cargo build -q -p almide-verify)
TD="${CARGO_TARGET_DIR:-$ROOT/target}/debug"
PRODUCER="$TD/examples/emit_native_bundle"
VERIFY="$TD/almide-verify"
./build-checker.sh >/dev/null

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

echo "== render every floor and migration fixture and produce its bundle =="
# Judge migration candidates before promoting the floor. The same programs
# enter the perf and shapes gates, so a floor promotion can wait for all
# verdicts without leaving their certificates outside the kernel check.
{ cat "$FLOOR"; cat "$CANDIDATES"; } | sed '/^#/d; /^$/d' | sort -u > "$OUT/corpus"
N=0
: > "$OUT/index"
while IFS= read -r fx; do
  case "$fx" in ''|'#'*) continue ;; esac
  N=$((N + 1))
  if ! (cd "$ROOT" && "$PRODUCER" "$fx" "$OUT/$N.rs") > "$OUT/$N.bundle" 2> "$OUT/$N.err"; then
    echo "NATIVE WALL FAIL: $fx is in the checked native corpus but the producer refused it:" >&2
    sed 's/^/  /' "$OUT/$N.err" >&2
    exit 1
  fi
  set +e
  "$VERIFY" bundle "$OUT/$N.bundle" > "$OUT/$N.verify" 2>&1
  echo $? > "$OUT/$N.verify.rc"
  set -e
  printf '%s\t%s\n' "$N" "$fx" >> "$OUT/index"
done < "$OUT/corpus"
[ "$N" -gt 0 ] || { echo "NATIVE WALL FAIL: proofs/native-verified-floor.txt lists no fixture" >&2; exit 1; }
echo "  $N fixture bundle(s)"

echo "== drill: one byte of a rendered source changed — the verifier must reject =="
mkdir -p "$OUT/tamper"
cp "$OUT/1.rs" "$OUT/tamper/1.rs"
python3 -c 'import sys; b = bytearray(open(sys.argv[1], "rb").read()); b[len(b) // 2] ^= 1; open(sys.argv[1], "wb").write(b)' "$OUT/tamper/1.rs"
sed "s|^artifact sha256 \([0-9a-f]*\) .*|artifact sha256 \1 $OUT/tamper/1.rs|" "$OUT/1.bundle" > "$OUT/tamper/1.bundle"
rc=0
"$VERIFY" bundle "$OUT/tamper/1.bundle" > "$OUT/tamper/verify" 2>&1 || rc=$?
if [ "$rc" -ne 1 ] || ! grep -q MISMATCH "$OUT/tamper/verify"; then
  echo "NATIVE WALL FAIL: almide-verify did not reject a rendered source one byte off (exit $rc)" >&2
  sed 's/^/  /' "$OUT/tamper/verify" >&2
  exit 1
fi
echo "  ok: the flipped byte reads MISMATCH (exit 1)"

echo "== the extracted checker judges every witness; the kernel file is generated =="
KGEN="$(mktemp /tmp/KernelNative_XXXXXX).v"
set +e
python3 - "$OUT" "$ROOT/proofs/checker" "$KGEN" <<'PYEOF'
import os, subprocess, sys
out, checker, kgen = sys.argv[1:4]

def parse(path):
    b = open(path, "rb").read()
    i, recs = b.index(b"\n") + 1, []
    while i < len(b):
        j = b.index(b"\n", i)
        line = b[i:j].decode()
        i = j + 1
        if line.startswith("witness "):
            _, prop, n, fn = line.split(" ", 3)
            n = int(n)
            recs.append((prop, fn, b[i:i + n].decode()))
            i += n + 1
    return recs

# The Rocq function that judges each property the native bundle carries.
KERNEL_FN = {"ownership": "check_xc", "native-calls": "check_native_calls_cert", "bounds": "check_bounds_cert"}
fail, accepts, total = [], [], 0
one = os.path.join(out, "one.w")
for row in open(os.path.join(out, "index")):
    k, fx = row.rstrip("\n").split("\t")
    recs = parse(os.path.join(out, k + ".bundle"))
    vtext = open(os.path.join(out, k + ".verify")).read()
    vrc = open(os.path.join(out, k + ".verify.rc")).read().strip()
    if not any(l.startswith("ARTIFACT") and l.endswith(": MATCH") for l in vtext.splitlines()):
        fail.append(f"{fx}: almide-verify did not confirm the rendered source's SHA-256")
    if vrc != "0":
        fail.append(f"{fx}: almide-verify's bundle outcome is exit {vrc}")
    if not recs:
        fail.append(f"{fx}: the bundle carries no witness")
    if not any(prop == "native-calls" for prop, _, _ in recs):
        fail.append(f"{fx}: the bundle carries no native-calls witness (the call binding, #4006 class P)")
    # Row 4: every function the bundle certifies the ownership of carries
    # its bounds witness too (render_native_bounds).
    owned_fns = {fn for prop, fn, _ in recs if prop == "ownership"}
    bounded_fns = {fn for prop, fn, _ in recs if prop == "bounds"}
    for fn in sorted(owned_fns - bounded_fns):
        fail.append(f"{fx}: {fn} carries no bounds witness (row 4)")
    for prop, fn, w in recs:
        total += 1
        with open(one, "w") as f:
            f.write(w)
        rc = subprocess.run([checker, prop, one], capture_output=True).returncode
        if rc != 0:
            fail.append(f"{fx}: [{prop}] {fn} REJECTED by the extracted checker (exit {rc}): {w[:200]!r}")
        elif prop not in KERNEL_FN:
            fail.append(f"{fx}: [{prop}] {fn} has no kernel verdict in this gate")
        else:
            accepts.append((prop, w))

def lit(s):
    return '"' + s.replace('"', '""') + '"'
with open(kgen, "w") as g:
    g.write("From AlmideTrust Require Import OwnershipChecker NativeCalls BoundsCert.\n")
    g.write("From Stdlib Require Import String List.\nImport ListNotations.\nOpen Scope string_scope.\n")
    LIMIT = 60000
    for prop, fn_name in KERNEL_FN.items():
        chunk, size = [], 0
        for w in [w for p, w in accepts if p == prop] + [None]:
            if w is None or (chunk and size + len(w) > LIMIT):
                if chunk:
                    g.write("Goal forallb %s [%s] = true.\nProof. vm_compute. reflexivity. Qed.\n" % (fn_name, ";".join(lit(x) for x in chunk)))
                chunk, size = [], 0
            if w is not None:
                chunk.append(w)
                size += len(w)

n = sum(1 for _ in open(os.path.join(out, "index")))
nown = sum(1 for p, _ in accepts if p == "ownership")
ncalls = sum(1 for p, _ in accepts if p == "native-calls")
nbounds = sum(1 for p, _ in accepts if p == "bounds")
print(f"  [ownership] {nown} + [native-calls] {ncalls} + [bounds] {nbounds} = {len(accepts)}/{total} witness(es) of {n} rendered program(s) accepted by the extracted checker; almide-verify CERTIFIED every bundle")
for m in fail[:20]:
    print("  FAIL " + m)
sys.exit(1 if fail else 0)
PYEOF
PRC=$?
set -e
if [ "$PRC" -ne 0 ]; then
  rm -f "$KGEN"
  echo "NATIVE WALL FAIL: a rendered program's witness is rejected, or the verdicts disagree (above)." >&2
  exit 1
fi

echo "== KERNEL ORACLE: the Rocq kernel re-derives every verdict above =="
if (cd "$ROOT/proofs" && "$COQC" -Q . AlmideTrust "$KGEN" >/dev/null 2>&1); then
  echo "  KERNEL OK: every native witness verdict certified by the Rocq kernel (vm_compute)"
  rm -f "$KGEN" "${KGEN%.v}.vo" "${KGEN%.v}.vos" "${KGEN%.v}.vok" "${KGEN%.v}.glob"
else
  echo "NATIVE WALL FAIL: the KERNEL disagrees with the extracted checker on a native witness." >&2
  rm -f "$KGEN"
  exit 1
fi

echo "== POISON LEG: every native poisoned certificate is REJECTED by all three verdicts =="
# proofs/poisoned-certs/native/ holds ownership witnesses (a release lost),
# proofs/poisoned-certs/native-calls/ the program call bindings (#4006 class
# P: a lying call-site mode, a missing move) — each the witness the producer
# writes with the bug re-injected (scripts/witness-reinject.sh native /
# native-calls pins them).
PGEN="$(mktemp /tmp/KernelNativePoison_XXXXXX).v"
printf 'From AlmideTrust Require Import OwnershipChecker NativeCalls BoundsCert.\nFrom Stdlib Require Import String.\nOpen Scope string_scope.\n' > "$PGEN"
NP=0
for leg in "native ownership check_xc" "native-calls native-calls check_native_calls_cert" "native-bounds bounds check_bounds_cert"; do
  read -r dir prop kfn <<<"$leg"
  POISONED=("$ROOT"/proofs/poisoned-certs/"$dir"/*.cert)
  if [ ! -e "${POISONED[0]}" ]; then
    echo "NATIVE WALL FAIL: proofs/poisoned-certs/$dir/ holds no certificate — the poison leg would pass vacuously." >&2
    rm -f "$PGEN"; exit 1
  fi
  for cert in "${POISONED[@]}"; do
    name="${cert#"$ROOT"/}"
    set +e
    ./checker "$prop" "$cert" >/dev/null 2>&1; XRC=$?
    "$VERIFY" "$prop" "$cert" >/dev/null 2>&1; VRC=$?
    set -e
    if [ "$XRC" -ne 1 ] || [ "$VRC" -ne 1 ]; then
      echo "NATIVE WALL FAIL: $name was not rejected (extracted checker exit $XRC, almide-verify exit $VRC; want 1 and 1)." >&2
      rm -f "$PGEN"; exit 1
    fi
    python3 -c 'import sys; print("Goal %s \"%s\" = false.\nProof. vm_compute. reflexivity. Qed." % (sys.argv[1], open(sys.argv[2]).read().replace("\"", "\"\"")))' "$kfn" "$cert" >> "$PGEN"
    NP=$((NP + 1))
    echo "  REJECT $name ($prop: extracted checker + almide-verify)"
  done
done
if (cd "$ROOT/proofs" && "$COQC" -Q . AlmideTrust "$PGEN" >/dev/null 2>&1); then
  echo "  KERNEL OK: the Rocq kernel rejects all $NP native poisoned certificate(s)"
  rm -f "$PGEN" "${PGEN%.v}.vo" "${PGEN%.v}.vos" "${PGEN%.v}.vok" "${PGEN%.v}.glob"
else
  echo "NATIVE WALL FAIL: the KERNEL did not reject a native poisoned certificate." >&2
  rm -f "$PGEN"; exit 1
fi

echo
echo "NATIVE WALL OK: over the $N programs the verified native renderer renders, every rendered"
echo "function's ownership witness and every program's call binding are accepted — the extracted"
echo "checker, the Rocq kernel and almide-verify agreeing — and every bundle names the SHA-256 of"
echo "the Rust source rendered."
