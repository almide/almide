#!/usr/bin/env bash
# STRUCTURAL CORPUS WALL (#2759, #1696 step 4): the corpus sweep of the leg
# that ships. corpus-wall.sh drives the MIR lowering, which no wasm artifact
# comes from since #2761; this twin drives the STRUCTURAL build — the
# product route `almide build --target wasm` takes (`almide::wasm_route`) —
# over every fixture golden/witness-fixtures.txt lists as fully certified
# (#2754: every frame of its shipped pass carries an ownership witness).
#
# Per fixture, the producer (crates/almide-wasm/examples/
# emit_structural_bundle.rs) writes one certificate bundle:
#   ownership        one witness per frame, recorded as the RC instructions
#                    were emitted (witness.rs)
#   call-modes       the program's call sites against their callees' frame
#                    conventions (witness_modes.rs)
#   names            per index space of the stock-WASI bytes the build ships,
#                    and per function's locals (cert_project.rs)
#   caps             per source-declared function: declared | transitive reach
#   caps-transitive  the whole call graph, the reach computed IN the proof
#
# ARTIFACT IDENTITY (#2760): each fixture is first built by the product path,
# `almide build --target wasm`, and the producer refuses unless that file is
# byte-for-byte the module its own route produced; the bundle then pins the
# file's SHA-256 (version-2 `artifact` record), and almide-verify recomputes
# the digest from the file and REJECTS a mismatch. The digests are written to
# proofs/structural-artifacts.sha256 (fixture -> SHA-256 of the shipped
# bytes; generated, not committed), cross-checked against `shasum -a 256` —
# the producer's sha2, almide-verify's own SHA-256 and shasum: three
# implementations of one number — and a drill flips one byte of a built
# artifact, which almide-verify must reject.
#
# ANCHORS (#4011 (d)): the producer also writes where in the shipped bytes
# each bounds and init event is, and almide-anchor-check (its own wasm
# decoder, no compiler code) holds every located event to the instruction
# there — a site's allocation size and its stores' width, offset and address
# local, a fill loop's exact shape, a check's compare-and-abort, an access's
# load or store — and each access's guard's OPERANDS (operands.rs): the
# compare tests this access's index against the count of the list it
# addresses. Every fixture must read ANCHORS OK: the events the proven
# checkers judge are the instructions the artifact contains.
#
# RC (#4277): the same check matches every rc_inc and release call of each
# certified frame against the ownership witness's recorded events (the anchor
# file's `rc` lines), and every recorded event against a call; a call or an
# event left over, or a check past the binary's deadline, is ANCHORS REJECT.
# No counted window class is left (#4278 B1: `element` retired by the slot
# consume `r`, `routine-internal` and `arm-internal` by the routines' and
# arms' own logged events; a release before an abort is matched like any
# other since sec-b2); the rebinds with no instruction and the judges with
# no rebind are totalled per run below,
# and so are the consumptions a runtime
# helper's checked signature accounts (`signed`, #4278 B1: no longer a
# trusted class — each helper's own `rc-sig` record is judged below).
#
# Every witness is judged by THREE independent verdicts, and all three must
# agree: the checker extracted from the Coq proof (proofs/checker), the Rocq
# KERNEL itself (a generated file of vm_compute goals, coqc'd), and
# almide-verify (the portable re-checker a binary distribution runs). The
# report gives acceptance per property. A witness may REJECT only if
# proofs/structural-wall-exceptions.txt names it (a shrink-only ledger: a
# listed witness that now accepts is stale and fails the gate).
#
# STDLIB-TEST LEG (`structural-wall.sh --stdlib-tests`, #4005 A4): the same
# judges over every spec/stdlib/*_test.almd as `almide test --target wasm`
# renders it (`almide::wasm_route::route_wasm_tests`, the test lane's one
# rendering, through the producer's `--tests` form). The wall corpus above is
# the list of fixtures certified on EVERY frame; the stdlib tests are what
# reaches the self-host bodies a program can link, so their `prim.load*` /
# `prim.store*` certificates (the bounds witness) are judged on the bodies
# that actually ship. There is no build artifact for a test file: the
# producer writes the stock-WASI bytes it judged, and the leg runs that file
# under wasmtime — the bytes judged are the bytes that pass the tests. A
# frame the leg could not certify is COUNTED (an `uncertified` record), not
# a failure; every witness present must be accepted by all three verdicts
# or named in proofs/structural-wall-tests-exceptions.txt (shrink-only).
# A test file the structural leg declines (it runs on native) or one that
# needs the embedded host (abort sites) has no stock artifact: counted, named.
# The poison legs below are corpus-independent and run in the default form.
set -euo pipefail
MODE=fixtures
if [ "${1:-}" = "--stdlib-tests" ]; then MODE=tests; fi
cd "$(dirname "$0")"
ROOT="$(cd .. && pwd)"
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1

FIXTURES="$ROOT/crates/almide-wasm/tests/golden/witness-fixtures.txt"
EXCEPTIONS="$ROOT/proofs/structural-wall-exceptions.txt"
if [ "$MODE" = tests ]; then
  EXCEPTIONS="$ROOT/proofs/structural-wall-tests-exceptions.txt"
  WASMTIME="${WASMTIME:-$(command -v wasmtime || true)}"
  [ -n "$WASMTIME" ] || { echo "STRUCTURAL WALL FAIL: wasmtime is not on PATH (the stdlib-test leg runs each judged artifact)" >&2; exit 1; }
fi
COQC="${COQC:-$(command -v coqc || true)}"
[ -n "$COQC" ] || { echo "STRUCTURAL WALL FAIL: coqc is not on PATH (the kernel verdict needs it)" >&2; exit 1; }

echo "== build the compiler, the bundle producer, almide-verify and the kernel-proven checker =="
(cd "$ROOT" && cargo build -q --bin almide && cargo build -q -p almide-wasm --example emit_structural_bundle && cargo build -q -p almide-verify -p almide-anchor-check)
TD="${CARGO_TARGET_DIR:-$ROOT/target}/debug"
ALMIDE="$TD/almide"
PRODUCER="$TD/examples/emit_structural_bundle"
VERIFY="$TD/almide-verify"
ANCHORCHECK="$TD/almide-anchor-check"
# The linear prover searches (simplex, loop invariants): release, so its
# time budget is spent on search rather than on debug arithmetic.
(cd "$ROOT" && cargo build -q --release -p almide-lin-prove)
LINPROVE="${CARGO_TARGET_DIR:-$ROOT/target}/release/almide-lin-prove"
./build-checker.sh >/dev/null
HASHES="$ROOT/proofs/structural-artifacts.sha256"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

# The full certified list on a full run (a push to develop or main, a tag, a
# manual run, a PR into main). Under ALMIDE_CI_SLICE=k (a PR into develop) the
# commit-rotated 1-in-k slice plus the fixtures the change touches
# (scripts/lib/ci-slice.sh), so consecutive PRs certify different slices. A
# slice judges exceptions only for the fixtures it ran and does not rewrite
# the artifact digest file.
. "$ROOT/scripts/lib/ci-slice.sh"
if [ "$MODE" = tests ]; then
  (cd "$ROOT" && LC_ALL=C ls spec/stdlib/*_test.almd) > "$OUT/listed"
  (cd "$ROOT" && ci_slice_filter structural-wall-tests < "$OUT/listed") > "$OUT/selected"
  HASHES="$OUT/structural-artifacts.sha256"
  : > "$OUT/declined"; : > "$OUT/embedded-only"; : > "$OUT/run-failed"; : > "$OUT/anchor-rejects"; : > "$OUT/run-host-limited"
else
  grep -v -E '^(#|$)' "$FIXTURES" > "$OUT/listed"
  (cd "$ROOT" && ci_slice_filter structural-wall < "$OUT/listed") > "$OUT/selected"
  if ci_slice_active; then
    HASHES="$OUT/structural-artifacts.sha256"
  fi
fi
echo "== build each certified fixture (almide build --target wasm) and produce its bundle =="
N=0
SEEN=0
{
  echo "# fixture -> SHA-256 of the bytes 'almide build --target wasm' ships for it (#2760)."
  echo "# Generated by proofs/structural-wall.sh; each certificate bundle pins its row."
} > "$HASHES.tmp"
while IFS= read -r f; do
  case "$f" in ''|'#'*) continue ;; esac
  SEEN=$((SEEN + 1))
  N=$((N + 1))
  if [ "$MODE" = tests ]; then
    rc=0
    (cd "$ROOT" && "$PRODUCER" --tests "$f" --write-artifact "$OUT/$N.wasm" --anchors "$OUT/$N.anchors") > "$OUT/$N.bundle" 2> "$OUT/$N.err" || rc=$?
    case "$rc" in
      0) ;;
      3) echo "$f" >> "$OUT/embedded-only"; N=$((N - 1)); continue ;;
      *) printf '%s\t%s\n' "$f" "$(grep -m1 . "$OUT/$N.err" | cut -c1-160)" >> "$OUT/declined"; N=$((N - 1)); continue ;;
    esac
    # The judged bytes pass their tests on a stock runtime (the
    # `ALMIDE_TEST_WASM_RUNNER=wasmtime` lane's invocation).
    if ! (cd "$ROOT" && timeout 600 "$WASMTIME" --dir=/ -S inherit-env=y --env=ALMIDE_CWD="$ROOT" "$OUT/$N.wasm" < /dev/null) > "$OUT/$N.run" 2>&1; then
      # An import the stock runtime does not define (`almide:process/spawn`,
      # served by the embedded host only) is the host's limit, not a verdict
      # on the bytes: counted and named. Any other failure fails the leg.
      if grep -q 'unknown import' "$OUT/$N.run"; then
        printf '%s\t%s\n' "$f" "$(grep -m1 -o 'unknown import: `[^`]*`' "$OUT/$N.run")" >> "$OUT/run-host-limited"
      else
        echo "$f" >> "$OUT/run-failed"
      fi
    fi
  elif ! (cd "$ROOT" && "$ALMIDE" build "$f" --target wasm -o "$OUT/$N.wasm") > "$OUT/$N.err" 2>&1; then
    echo "STRUCTURAL WALL FAIL: 'almide build --target wasm' refused a certified fixture: $f" >&2
    sed 's/^/  /' "$OUT/$N.err" >&2
    exit 1
  fi
  if [ "$MODE" = fixtures ] && ! (cd "$ROOT" && "$PRODUCER" "$f" --artifact "$OUT/$N.wasm" --anchors "$OUT/$N.anchors") > "$OUT/$N.bundle" 2> "$OUT/$N.err"; then
    echo "STRUCTURAL WALL FAIL: the producer refused a certified fixture: $f" >&2
    sed 's/^/  /' "$OUT/$N.err" >&2
    exit 1
  fi
  printf '%s\t%s\n' "$N" "$f" >> "$OUT/index"
  # Verdict 3: almide-verify, one run per bundle (its per-witness lines are
  # read back in bundle order below); it also recomputes the artifact digest.
  rc=0
  "$VERIFY" bundle "$OUT/$N.bundle" > "$OUT/$N.verify" 2>&1 || rc=$?
  echo "$rc" > "$OUT/$N.verify.rc"
  # The third SHA-256 — shasum's — against the digest the bundle pins.
  sum="$(shasum -a 256 "$OUT/$N.wasm" | cut -d' ' -f1)"
  pinned="$(sed -n 's/^artifact sha256 \([0-9a-f]*\) .*/\1/p' "$OUT/$N.bundle")"
  if [ "$sum" != "$pinned" ]; then
    echo "STRUCTURAL WALL FAIL: $f: the bundle pins sha256 '$pinned', shasum reads $sum" >&2
    exit 1
  fi
  printf '%s  %s\n' "$sum" "$f" >> "$HASHES.tmp"
  # #4005 A4: the module's self-host prim accesses. The prover is untrusted;
  # what it certifies must be the translation of the shipped functions
  # (--lin-verify) and is judged as a bounds witness below. What it leaves
  # out stays in the counted class.
  if grep -q '^prims .' "$OUT/$N.anchors"; then
    if ! "$LINPROVE" "$OUT/$N.wasm" "$OUT/$N.anchors" > "$OUT/$N.lcert" 2> "$OUT/$N.lprove"; then
      echo "STRUCTURAL WALL FAIL: $f: the linear prover failed:" >&2
      sed 's/^/  /' "$OUT/$N.lprove" >&2
      exit 1
    fi
    if ! "$ANCHORCHECK" --lin-verify "$OUT/$N.wasm" "$OUT/$N.anchors" "$OUT/$N.lcert" > "$OUT/$N.lin" 2>&1; then
      echo "STRUCTURAL WALL FAIL: $f: the linear certificate is not the translation of the shipped functions:" >&2
      sed 's/^/  /' "$OUT/$N.lin" >&2
      exit 1
    fi
    rc=0
    "$VERIFY" bounds "$OUT/$N.lcert" > "$OUT/$N.lverify" 2>&1 || rc=$?
    echo "$rc" > "$OUT/$N.lverify.rc"
  fi
done < "$OUT/selected"
# #4005 A4: the runtime routines the anchor file's `routines` line names,
# then the anchor check, per artifact in parallel (each stage reads only its
# own artifact's files; the verdicts are read back in index order below).
# The same untrusted prover writes the routine certificate and its site
# selection (the accesses it proves; the rest stay counted); the
# selection's events must be the translation of the shipped routines
# (--lin-verify-routines), and the certificate is judged as a bounds
# witness below by all three verdicts. #4011 (d): every located bounds /
# init event is its instruction. With it, the row-4 census (#4005) per
# function, the accesses the verified linear and routine certificates
# cover counted located — read by the heap-access ledger after this stage
# (proofs/lib/heap_ledger.py).
stage2() {
  local N="$1"
  if grep -q '^routines .' "$OUT/$N.anchors"; then
    if ! "$LINPROVE" --routines "$OUT/$N.wasm" "$OUT/$N.anchors" "$OUT/$N.rsel" > "$OUT/$N.rcert" 2> "$OUT/$N.rprove"; then
      echo "the routine prover failed:" > "$OUT/$N.s2fail"; cat "$OUT/$N.rprove" >> "$OUT/$N.s2fail"; return
    fi
    if ! "$ANCHORCHECK" --lin-verify-routines "$OUT/$N.wasm" "$OUT/$N.anchors" "$OUT/$N.rcert" "$OUT/$N.rsel" > "$OUT/$N.rlin" 2>&1; then
      echo "the routine certificate is not the translation of the shipped routines:" > "$OUT/$N.s2fail"; cat "$OUT/$N.rlin" >> "$OUT/$N.s2fail"; return
    fi
    local rc=0
    "$VERIFY" bounds "$OUT/$N.rcert" > "$OUT/$N.rverify" 2>&1 || rc=$?
    echo "$rc" > "$OUT/$N.rverify.rc"
  fi
  local census=(--census --census-fns)
  if [ -s "$OUT/$N.lcert" ]; then census+=(--lin "$OUT/$N.lcert"); fi
  if [ -f "$OUT/$N.rsel" ]; then census+=(--rlin "$OUT/$N.rcert" "$OUT/$N.rsel"); fi
  local arc=0
  "$ANCHORCHECK" "${census[@]}" "$OUT/$N.wasm" "$OUT/$N.bundle" "$OUT/$N.anchors" > "$OUT/$N.anchor" 2>&1 || arc=$?
  echo "$arc" > "$OUT/$N.arc"
}
export -f stage2
export OUT LINPROVE ANCHORCHECK VERIFY
: >> "$OUT/index"
JOBS="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
cut -f1 "$OUT/index" | xargs -P "$JOBS" -I{} bash -c 'stage2 "$1"' _ {}
while IFS="$(printf '\t')" read -r K f; do
  if [ -f "$OUT/$K.s2fail" ]; then
    echo "STRUCTURAL WALL FAIL: $f: $(head -1 "$OUT/$K.s2fail")" >&2
    tail -n +2 "$OUT/$K.s2fail" | sed 's/^/  /' >&2
    exit 1
  fi
  [ -f "$OUT/$K.arc" ] || { echo "STRUCTURAL WALL FAIL: $f: the anchor check did not run" >&2; exit 1; }
  arc="$(cat "$OUT/$K.arc")"
  if [ "$arc" -ne 0 ] && [ "$MODE" = tests ]; then
    # The stdlib-test leg names each artifact whose events are not its
    # instructions (judged against the exception ledger's `anchors` rows
    # below) and judges the rest; its totals leave this artifact out.
    printf '%s\t%s\n' "$f" "$(grep -m1 'REJECT' "$OUT/$K.anchor" | cut -c1-200)" >> "$OUT/anchor-rejects"
    mv "$OUT/$K.anchor" "$OUT/$K.anchor-rejected"
  elif [ "$arc" -ne 0 ]; then
    echo "STRUCTURAL WALL FAIL: $f: a witness event is not the instruction the shipped bytes hold:" >&2
    sed 's/^/  /' "$OUT/$K.anchor" >&2
    exit 1
  fi
done < "$OUT/index"
# #4005 row 4: every heap access no certificate covers, per runtime routine,
# per self-host body and per fixture's own code, held to the shrink-only
# ledger (a count above its row fails; on a full run, so does a row above
# what it measures — lower it). STRUCTURAL_WALL_LEDGER_OUT=<file> also
# writes what this run measured, in the ledger's own format.
LEDGER="$ROOT/proofs/heap-access-ledger.tsv"
[ "$MODE" = tests ] && LEDGER="$ROOT/proofs/heap-access-ledger-tests.tsv"
python3 "$ROOT/proofs/lib/heap_ledger.py" measure "$OUT" > "$OUT/heap.measured" || { echo "STRUCTURAL WALL FAIL: the row-4 census is unreadable" >&2; exit 1; }
if [ -n "${STRUCTURAL_WALL_LEDGER_OUT:-}" ]; then cp "$OUT/heap.measured" "$STRUCTURAL_WALL_LEDGER_OUT"; fi
full=1; if ci_slice_active; then full=0; fi
python3 "$ROOT/proofs/lib/heap_ledger.py" check "$LEDGER" "$OUT/heap.measured" "$full" || exit 1
[ "$N" -gt 0 ] || { echo "STRUCTURAL WALL FAIL: no certified fixture listed in $FIXTURES" >&2; exit 1; }
[ "$SEEN" -eq "$(wc -l < "$OUT/selected" | tr -d ' ')" ] || { echo "STRUCTURAL WALL FAIL: read $SEEN of the selected fixtures — the scan went blind" >&2; exit 1; }
mv "$HASHES.tmp" "$HASHES"
if [ "$MODE" = tests ]; then
  echo "  $N of $SEEN stdlib test file(s) judged; $(wc -l < "$OUT/declined" | tr -d ' ') declined by the structural leg (they run on native), $(wc -l < "$OUT/embedded-only" | tr -d ' ') need the embedded host (no stock artifact)"
  sed 's/^/    declined: /' "$OUT/declined"
  sed 's/^/    embedded-only: /' "$OUT/embedded-only"
  if [ -s "$OUT/run-failed" ]; then
    echo "STRUCTURAL WALL FAIL: a judged artifact does not pass its tests under wasmtime:" >&2
    while IFS= read -r f; do
      echo "  $f" >&2
      k="$(awk -F'\t' -v f="$f" '$2 == f {print $1}' "$OUT/index")"
      tail -5 "$OUT/$k.run" | sed 's/^/    /' >&2
    done < "$OUT/run-failed"
    exit 1
  fi
  echo "  every other judged artifact passes its tests under wasmtime; $(wc -l < "$OUT/run-host-limited" | tr -d ' ') import what the stock runtime does not define:"
  sed 's/^/    /' "$OUT/run-host-limited"
  # Anchor rejects against the ledger's `anchors` rows (file<TAB>anchors<TAB>-<TAB>issue):
  # a reject the ledger does not name fails, and so does a row that no longer rejects.
  { grep -v -E '^(#|$)' "$EXCEPTIONS" 2>/dev/null || true; } | awk -F'\t' '$2 == "anchors" {print $1}' | LC_ALL=C sort -u > "$OUT/anchor-ledger"
  cut -f1 "$OUT/anchor-rejects" | LC_ALL=C sort -u > "$OUT/anchor-seen"
  echo "  [anchors] $(wc -l < "$OUT/anchor-seen" | tr -d ' ') of $N artifact(s) hold an event that is not its instruction:"
  sed 's/^/    /' "$OUT/anchor-rejects"
  unnamed="$(LC_ALL=C comm -23 "$OUT/anchor-seen" "$OUT/anchor-ledger")"
  stale="$(LC_ALL=C comm -13 "$OUT/anchor-seen" "$OUT/anchor-ledger" | while IFS= read -r f; do if grep -qxF "$f" "$OUT/selected"; then echo "$f"; fi; done)"
  if [ -n "$unnamed" ] || [ -n "$stale" ]; then
    [ -z "$unnamed" ] || { echo "STRUCTURAL WALL FAIL: ANCHORS REJECT on an artifact no exception names:" >&2; echo "$unnamed" | sed 's/^/  /' >&2; }
    [ -z "$stale" ] || { echo "STRUCTURAL WALL FAIL: STALE anchors exception (the artifact now reads ANCHORS OK — remove the row):" >&2; echo "$stale" | sed 's/^/  /' >&2; }
    exit 1
  fi
else
  echo "  $N fixture bundle(s); artifact digests in proofs/structural-artifacts.sha256"
fi
python3 - "$OUT" <<'PYEOF'
import os, re, sys
out = sys.argv[1]
keys = ["init_sites", "init_stores", "fills", "init_unlocated", "checks", "accesses", "bounds_unlocated", "operands"]
fkeys = ["f_sites", "f_stores", "f_accesses", "f_guarded", "f_tags", "f_unlocated"]
ftot = dict.fromkeys(fkeys, 0)
fpat = re.compile(r"FIELDS OK: (\d+) site\(s\) / (\d+) store\(s\), (\d+) access\(es\) \((\d+) case field\(s\) under their tag test\), (\d+) tag read\(s\), (\d+) unlocated")
tot = dict.fromkeys(keys, 0)
pat = re.compile(r"init (\d+) site\(s\) / (\d+) store\(s\), (\d+) fill\(s\), (\d+) unlocated; bounds (\d+) check\(s\), (\d+) access\(es\), (\d+) unlocated, (\d+) operand pair\(s\)")
for f in os.listdir(out):
    if f.endswith(".anchor"):
        text = open(os.path.join(out, f)).read()
        m = pat.search(text)
        if m:
            for k, v in zip(keys, m.groups()):
                tot[k] += int(v)
        m = fpat.search(text)
        if not m:
            sys.exit(f"STRUCTURAL WALL FAIL: {f} carries no FIELDS OK line — the fields witness was not held to the bytes")
        for k, v in zip(fkeys, m.groups()):
            ftot[k] += int(v)
if tot["operands"] != tot["accesses"]:
    sys.exit(f"STRUCTURAL WALL FAIL: {tot['accesses']} located access(es) but {tot['operands']} held to their guard's operands")
# #4277: and every matched release on a local held to that local's last
# read (anchor-check rc/after.rs) — a line without that count is no verdict.
rcpat = re.compile(r"RC OK: (\d+) frame\(s\), (\d+) event\(s\) matched to (\d+) call\(s\), (\d+) rebind\(s\) with no instruction, (\d+) judge\(s\) with no rebind, (\d+) signed consumption\(s\), (\d+) release\(s\) the last read of their local")
rk = ["frames", "events", "calls", "rebind", "judges", "signed", "last_reads"]
rc = dict.fromkeys(rk, 0)
for f in os.listdir(out):
    if f.endswith(".anchor"):
        m = rcpat.search(open(os.path.join(out, f)).read())
        if not m:
            sys.exit(f"STRUCTURAL WALL FAIL: {f}: no RC verdict — the anchor file carried no rc lines, or the check held no release to its last read")
        for k, v in zip(rk, m.groups()):
            rc[k] += int(v)
print(f"  [rc] RC OK on every artifact: {rc['frames']} frame(s), {rc['events']} event(s) matched to {rc['calls']} call(s); "
      f"{rc['signed']} signed consumption(s), {rc['last_reads']} release(s) held to their local's last read; counted (#4278): "
      f"{rc['rebind']} rebind(s) with no instruction, {rc['judges']} judge(s) with no rebind")
print(f"  [anchors] ANCHORS OK on every artifact: {tot['init_sites']} site(s) / {tot['init_stores']} store(s) and {tot['fills']} fill(s), "
      f"{tot['checks']} check(s) and {tot['accesses']} access(es) held to the shipped bytes, every access's guard operands to it; unlocated (dead bodies): "
      f"{tot['init_unlocated']} site(s), {tot['bounds_unlocated']} bounds event(s)")
print(f"  [anchors] FIELDS OK on every artifact: {ftot['f_sites']} site(s) / {ftot['f_stores']} store(s), {ftot['f_accesses']} field access(es) "
      f"({ftot['f_guarded']} variant case field(s) held to their tag test), {ftot['f_tags']} tag read(s); unlocated (dead bodies): {ftot['f_unlocated']}")
PYEOF

echo "== drill: one byte of a shipped artifact changed — the verifier must reject =="
mkdir -p "$OUT/tamper"
sed 's|^artifact sha256 \([0-9a-f]*\) .*|artifact sha256 \1 app.wasm|' "$OUT/1.bundle" > "$OUT/tamper/1.bundle"
python3 -c 'import sys; b = bytearray(open(sys.argv[1], "rb").read()); b[len(b) // 2] ^= 1; open(sys.argv[2], "wb").write(b)' \
  "$OUT/1.wasm" "$OUT/tamper/app.wasm"
rc=0
"$VERIFY" bundle "$OUT/tamper/1.bundle" > "$OUT/tamper/verify" 2>&1 || rc=$?
if [ "$rc" -ne 1 ] || ! grep -q MISMATCH "$OUT/tamper/verify"; then
  echo "STRUCTURAL WALL FAIL: almide-verify did not reject an artifact one byte off (exit $rc)" >&2
  sed 's/^/  /' "$OUT/tamper/verify" >&2
  exit 1
fi
cp "$OUT/1.wasm" "$OUT/tamper/app.wasm"
rc=0
"$VERIFY" bundle "$OUT/tamper/1.bundle" > /dev/null 2>&1 || rc=$?
# The stdlib-test leg's bundle may declare uncertified frames: its untouched
# control is then INCOMPLETE (exit 3), the same outcome the leg requires of it.
want=0
if [ "$MODE" = tests ] && grep -aq '^uncertified ' "$OUT/1.bundle"; then want=3; fi
if [ "$rc" -ne "$want" ]; then
  echo "STRUCTURAL WALL FAIL: the drill's control (the untouched artifact) did not verify (exit $rc, want $want)" >&2
  exit 1
fi
echo "  ok: the flipped byte reads MISMATCH (exit 1); the untouched artifact verifies (exit 0)"

echo "== drill: a linear certificate with one proof emptied, and one held to another artifact, must be refused =="
# The first fixture whose certificate certifies a body, and the first other
# fixture whose certificate differs from it.
LA=""; LB=""
while IFS="$(printf '\t')" read -r k _; do
  [ -s "$OUT/$k.lcert" ] && grep -q '^certified' "$OUT/$k.lprove" || continue
  if [ -z "$LA" ]; then LA="$k"; elif ! cmp -s "$OUT/$LA.lcert" "$OUT/$k.lcert"; then LB="$k"; break; fi
done < "$OUT/index"
if [ -n "$LA" ]; then
  # Proof tamper: the first non-empty Farkas combination becomes the empty
  # one, which entails no fact that names a variable.
  python3 -c 'import re, sys; s = open(sys.argv[1]).read(); t = re.sub(r"\((\d+:\d+(?: \d+:\d+)*)\)(\d+)", "()1", s, count=1); assert t != s; open(sys.argv[2], "w").write(t)' \
    "$OUT/$LA.lcert" "$OUT/tamper/lin.cert"
  rc1=0; "$VERIFY" bounds "$OUT/tamper/lin.cert" > /dev/null 2>&1 || rc1=$?
  rc2=0; ./checker bounds "$OUT/tamper/lin.cert" > /dev/null 2>&1 || rc2=$?
  if [ "$rc1" -eq 0 ] || [ "$rc2" -ne 1 ]; then
    echo "STRUCTURAL WALL FAIL: a linear certificate with one proof emptied was not refused (almide-verify exit $rc1, extracted checker exit $rc2)" >&2
    exit 1
  fi
  echo "  ok: the emptied proof is refused by almide-verify (exit $rc1) and the extracted checker (exit $rc2)"
  if [ -n "$LB" ]; then
    rc=0; "$ANCHORCHECK" --lin-verify "$OUT/$LB.wasm" "$OUT/$LB.anchors" "$OUT/$LA.lcert" > "$OUT/tamper/lin" 2>&1 || rc=$?
    if [ "$rc" -ne 1 ] || ! grep -q 'LIN REJECT' "$OUT/tamper/lin"; then
      echo "STRUCTURAL WALL FAIL: a linear certificate held to another artifact was not refused (exit $rc)" >&2
      sed 's/^/  /' "$OUT/tamper/lin" >&2
      exit 1
    fi
    echo "  ok: the certificate held to another artifact is LIN REJECT ($(head -1 "$OUT/tamper/lin" | cut -c1-100))"
  fi
elif ! ci_slice_active; then
  echo "STRUCTURAL WALL FAIL: no fixture's linear certificate certifies a body — the drill has nothing to tamper" >&2
  exit 1
fi

echo "== the extracted checker judges every witness; the kernel file is generated =="
KGEN="$(mktemp /tmp/KernelStructural_XXXXXX).v"
set +e
python3 - "$OUT" "$ROOT/proofs/checker" "$EXCEPTIONS" "$KGEN" "$MODE" <<'PYEOF'
import os, subprocess, sys
out, checker, exceptions_path, kgen, mode = sys.argv[1:6]
FN = {"ownership": "check_xc", "names": "check_names_cert", "caps": "check_caps_cert",
      "caps-transitive": "check_prog_cert", "call-modes": "check_modes_cert",
      "bounds": "check_lbounds_cert", "init": "check_init_cert", "fields": "check_fields_cert", "rc-sig": "check_sig_cert"}

def parse(path):
    b = open(path, "rb").read()
    i, recs, uncert = b.index(b"\n") + 1, [], []
    while i < len(b):
        j = b.index(b"\n", i)
        line = b[i:j].decode()
        i = j + 1
        if line.startswith("witness "):
            _, prop, n, fn = line.split(" ", 3)
            n = int(n)
            recs.append((prop, fn, b[i:i + n].decode()))
            i += n + 1
        elif line.startswith("uncertified "):
            uncert.append(line)
    return recs, uncert

def verify_lines(path):
    got = []
    for line in open(path).read().splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[0] in ("ACCEPT", "REJECT"):
            got.append((parts[1], parts[2], parts[0] == "ACCEPT"))
    return got

exceptions = set()
if os.path.exists(exceptions_path):
    for line in open(exceptions_path):
        line = line.strip()
        if line and not line.startswith("#"):
            fx, prop, fn = line.split("\t")[:3]
            if prop != "anchors":  # the stdlib-test leg's artifact rows, judged in bash
                exceptions.add((fx, prop, fn))

fail = []
stats = {p: [0, 0] for p in FN}
fixtures_ok = 0
uncert_total, uncert_files, uncert_reasons = 0, 0, {}
rejected_seen = set()
accepts = {p: [] for p in FN}   # kernel: forallb f [...] = true
rejects = []                     # kernel: f w = false
one = os.path.join(out, "one.w")
for row in open(os.path.join(out, "index")):
    k, fx = row.rstrip("\n").split("\t")
    recs, uncert = parse(os.path.join(out, k + ".bundle"))
    if uncert and mode == "tests":
        # The stdlib-test leg counts what it could not certify (above).
        uncert_total += len(uncert)
        uncert_files += 1
        for u in uncert:
            r = u.split(" ", 2)[2] if u.count(" ") >= 2 else u
            r = r.split(":")[0][:60]
            uncert_reasons[r] = uncert_reasons.get(r, 0) + 1
    elif uncert:
        fail.append(f"{fx}: the bundle declares {len(uncert)} uncertified frame(s) — the fixture is listed as certified ({uncert[0]})")
    portable = verify_lines(os.path.join(out, k + ".verify"))
    vtext = open(os.path.join(out, k + ".verify")).read()
    vrc = open(os.path.join(out, k + ".verify.rc")).read().strip()
    if not any(l.startswith("ARTIFACT") and l.endswith(": MATCH") for l in vtext.splitlines()):
        fail.append(f"{fx}: almide-verify did not confirm the artifact's SHA-256")
    if len(portable) != len(recs):
        fail.append(f"{fx}: almide-verify judged {len(portable)} of {len(recs)} witnesses")
        continue
    all_ok = True
    for (prop, fn, w), (vp, vf, vok) in zip(recs, portable):
        if (vp, vf) != (prop, fn):
            fail.append(f"{fx}: almide-verify's verdict order drifted at [{prop}] {fn}")
            break
        with open(one, "w") as f:
            f.write(w)
        rc = subprocess.run([checker, prop, one], capture_output=True).returncode
        if rc not in (0, 1):
            fail.append(f"{fx}: the extracted checker gave no verdict (exit {rc}) on [{prop}] {fn}")
            continue
        ok = rc == 0
        if ok != vok:
            fail.append(f"{fx}: [{prop}] {fn}: extracted checker {'ACCEPT' if ok else 'REJECT'}, almide-verify {'ACCEPT' if vok else 'REJECT'}")
        stats[prop][1] += 1
        key = (fx, prop, fn)
        if ok:
            stats[prop][0] += 1
            accepts[prop].append(w)
        else:
            all_ok = False
            rejects.append((prop, w))
            rejected_seen.add(key)
            if key not in exceptions:
                fail.append(f"{fx}: [{prop}] {fn} REJECTED and no exception names it: {w[:200]!r}")
    fixtures_ok += all_ok
    # The stdlib-test leg: a bundle declaring uncertified frames is
    # INCOMPLETE (exit 3) by almide-verify's own reading, and exactly then.
    want = "3" if (mode == "tests" and uncert) else "0"
    if all_ok and vrc != want:
        fail.append(f"{fx}: every witness accepts, but almide-verify's bundle outcome is exit {vrc} (want {want})")
# #4005 A4: each module's linear certificate is one more bounds witness,
# judged by the same three verdicts. A refusal has no exception row: what the
# prover could not prove is left out of the certificate, never put in it.
lin = {"certified": [0, 0], "uncertified": [0, 0]}
effort = [0, 0, set()]  # pivots taken, the most any module took, the budgets
names = {"certified": {}, "uncertified": {}}
why = {}
for row in open(os.path.join(out, "index")):
    k, fx = row.rstrip("\n").split("\t")
    cpath = os.path.join(out, k + ".lcert")
    if not os.path.exists(cpath):
        continue
    for l in open(os.path.join(out, k + ".lprove")).read().splitlines():
        parts = l.split("\t")
        if parts[0] == "effort" and len(parts) == 5:
            effort[0] += int(parts[1]); effort[1] = max(effort[1], int(parts[1])); effort[2].add(parts[3])
        if parts[0] in lin and len(parts) >= 4:
            lin[parts[0]][0] += 1
            lin[parts[0]][1] += int(parts[3])
            # Distinct self-host bodies by name, each with its prim sites.
            names[parts[0]][parts[2]] = max(names[parts[0]].get(parts[2], 0), int(parts[3]))
            if parts[0] == "uncertified" and len(parts) >= 5:
                why[parts[4].split(":")[0][:60]] = why.get(parts[4].split(":")[0][:60], 0) + 1
    w = open(cpath).read().rstrip("\n")
    rc = subprocess.run([checker, "bounds", cpath], capture_output=True).returncode
    vok = open(os.path.join(out, k + ".lverify.rc")).read().strip() == "0"
    stats["bounds"][1] += 1
    if rc == 0 and vok:
        stats["bounds"][0] += 1
        accepts["bounds"].append(w)
    else:
        fail.append(f"{fx}: the linear certificate: extracted checker exit {rc}, almide-verify {'ACCEPT' if vok else 'REJECT'}")
# #4005 A4: each module's ROUTINE certificate (the runtime routines the
# `routines` line names) is one more bounds witness, judged the same way.
rout = [0, 0, 0, 0]  # procedures, certified accesses, counted accesses, routines left out
for row in open(os.path.join(out, "index")):
    k, fx = row.rstrip("\n").split("\t")
    cpath = os.path.join(out, k + ".rcert")
    if not os.path.exists(cpath):
        continue
    for l in open(os.path.join(out, k + ".rprove")).read().splitlines():
        parts = l.split("\t")
        if parts[0] == "routine" and len(parts) == 5:
            rout[0] += 1
            rout[1] += int(parts[3])
            rout[2] += int(parts[4])
        elif parts[0] == "unrouted":
            rout[3] += 1
    w = open(cpath).read().rstrip("\n")
    rc = subprocess.run([checker, "bounds", cpath], capture_output=True).returncode
    vok = open(os.path.join(out, k + ".rverify.rc")).read().strip() == "0"
    stats["bounds"][1] += 1
    if rc == 0 and vok:
        stats["bounds"][0] += 1
        accepts["bounds"].append(w)
    else:
        fail.append(f"{fx}: the routine certificate: extracted checker exit {rc}, almide-verify {'ACCEPT' if vok else 'REJECT'}")
print(f"  [bounds/routines] {rout[0]} runtime routine procedure(s) certified over the artifacts, {rout[1]} access(es) their "
      f"selections prove, {rout[2]} left counted; {rout[3]} routine(s) left out of their table")
print(f"  [bounds/linear] {lin['certified'][0]} self-host body(ies) certified ({lin['certified'][1]} prim site(s)), "
      f"{lin['uncertified'][0]} left in the counted class ({lin['uncertified'][1]} prim site(s)); "
      f"search budget {'/'.join(sorted(effort[2])) or '-'} pivot(s) per module, {effort[0]} taken, at most {effort[1]} by one module")
only_unc = {n: k for n, k in names["uncertified"].items() if n not in names["certified"]}
print(f"  [bounds/linear] distinct bodies: {len(names['certified'])} certified somewhere ({sum(names['certified'].values())} prim site(s)), "
      f"{len(only_unc)} never certified ({sum(only_unc.values())} prim site(s))")
for w, k in sorted(why.items(), key=lambda x: -x[1])[:8]:
    print(f"    uncertified because: {w} ({k})")
ran = {row.rstrip("\n").split("\t")[1] for row in open(os.path.join(out, "index"))}
for key in sorted(k for k in exceptions - rejected_seen if k[0] in ran):
    fail.append("STALE exception (no witness of that name rejects — remove the row): " + "\t".join(key))

def lit(s):
    return '"' + s.replace('"', '""') + '"'
# The kernel file is split into parts of at most PART literal bytes, each
# its own coqc run (proofs/lib/kernel_parts.py): the kernel keeps every
# goal's statement, so one file of every witness grows its memory with the
# corpus. The manifest's counts are checked before the run (no goal or
# witness dropped), and a drill set is written beside it: two honest
# witnesses, each its own part, and the same with a corrupted third in a
# later part — the run must accept the first and refuse the second, and
# refuse the first once a part file is gone.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(checker)), "lib"))
import kernel_parts
LIMIT = 60000  # literal bytes per goal: bounds the kernel's peak term
PART = 4000000  # literal bytes per kernel file
HEADER = "From AlmideTrust Require Import OwnershipChecker NameTotality CapabilityBound CapabilityReach CallModes HelperSig BoundsCert InitCert FieldCert RoutineBoundsCert RoutineProcsCert RoutineLinCert.\n" + "From Stdlib Require Import String List.\nImport ListNotations.\nOpen Scope string_scope.\n"
goals = []
for prop, ws in accepts.items():
    chunk, size = [], 0
    for w in ws + [None]:
        if w is None or (chunk and size + len(w) > LIMIT):
            if chunk:
                goals.append(("Goal forallb %s [%s] = true.\nProof. vm_compute. reflexivity. Qed.\n" % (FN[prop], ";".join(lit(x) for x in chunk)), size, len(chunk)))
            chunk, size = [], 0
        if w is not None:
            chunk.append(w)
            size += len(w)
for prop, w in rejects:
    goals.append(("Goal %s %s = false.\nProof. vm_compute. reflexivity. Qed.\n" % (FN[prop], lit(w)), len(w), 1))
kernel_parts.write(kgen[:-2], HEADER, goals, PART)
with open(kgen[:-2] + ".count", "w") as g:
    g.write("%d\n" % (sum(len(ws) for ws in accepts.values()) + len(rejects)))
small = sorted((len(w), prop, w) for prop, ws in accepts.items() for w in ws if w.strip())[:64]
one_path = kgen[:-2] + ".one"
goal1 = lambda prop, w: ("Goal forallb %s [%s] = true.\nProof. vm_compute. reflexivity. Qed.\n" % (FN[prop], lit(w)), 1, 1)
bad = None
for _, prop, w in small[2:]:
    for cand in (w + " @", "@" + w, w[: len(w) // 2], w + ";" + w[::-1]):
        with open(one_path, "w") as f:
            f.write(cand)
        if subprocess.run([checker, prop, one_path], capture_output=True).returncode == 1:
            bad = (prop, cand)
            break
    if bad:
        break
if len(small) < 3 or bad is None:
    fail.append("kernel drill: no small accepted witness has a corruption the extracted checker rejects")
else:
    honest = [goal1(prop, w) for _, prop, w in small[:2]]
    kernel_parts.write(kgen[:-2] + "_dh", HEADER, honest, 1)
    kernel_parts.write(kgen[:-2] + "_dc", HEADER, honest + [goal1(*bad)], 1)

for p, (a, t) in stats.items():
    print(f"  [{p}] {a}/{t} witness(es) accepted by the extracted checker and almide-verify")
# #4005 coverage: how many index-operator accesses the bounds witnesses
# record, and how many of them carry no check of their own token count
# (elided by #3345 and re-derived by the checker). Only accepted witnesses.
import re as _re
toks = {k: 0 for k in "acwse"}
for w in accepts["bounds"]:
    for t in _re.findall(r"[acwse](?=\d)", w):
        toks[t] += 1
print(f"  [bounds] recorded: {toks['a']} read(s), {toks['w']} write(s), {toks['c']} emitted check(s), {toks['e']} loop exit test(s), {toks['s']} kill(s), over {sum(1 for w in accepts['bounds'] if w.strip(';'))} non-empty witness(es)")
# #4013 coverage: the construction sites the init witnesses record (list
# literals today; every other allocation site records nothing).
sites = sum(len([x for x in w.split(";") if x.strip()]) for w in accepts["init"])
print(f"  [init] recorded: {sites} construction site(s), over {sum(1 for w in accepts['init'] if w.strip())} non-empty witness(es)")
# #4279 coverage: the record / tuple / variant sites, field accesses and tag
# reads the fields witnesses record.
ftoks = {k: 0 for k in "Lkat"}
for w in accepts["fields"]:
    for item in w.split(";"):
        item = item.strip()
        if item and item[0] in ftoks:
            ftoks[item[0]] += 1
print(f"  [fields] recorded: {ftoks['k']} construction site(s), {ftoks['a']} field access(es), {ftoks['t']} tag read(s) over {ftoks['L']} layout(s), in {sum(1 for w in accepts['fields'] if w.strip())} non-empty witness(es)")
n = sum(1 for _ in open(os.path.join(out, "index")))
print(f"  {fixtures_ok}/{n} fixture(s) accepted on every property; {len(rejects)} rejected witness(es), each named in the exception ledger")
if mode == "tests":
    print(f"  [uncertified] {uncert_total} frame(s) the leg could not certify, in {uncert_files} of {n} test file(s); by reason:")
    for r, c in sorted(uncert_reasons.items(), key=lambda kv: (-kv[1], kv[0]))[:25]:
        print(f"    {c:6d}  {r}")
for m in fail[:20]:
    print("  FAIL " + m)
sys.exit(1 if fail else 0)
PYEOF
PRC=$?
set -e
if [ "$PRC" -ne 0 ]; then
  rm -f "$KGEN"
  echo "STRUCTURAL WALL FAIL: a certified fixture's witness is rejected, or the verdicts disagree (above)." >&2
  exit 1
fi

echo "== KERNEL ORACLE: the Rocq kernel re-derives every verdict above =="
KSTART=$(date +%s)
# One coqc per part (proofs/lib/kernel_parts.py's manifest), in parallel:
# min(nproc, available memory / KERNEL_PART_MB) at once, each part's peak
# resident size recorded when /usr/bin/time can; every part must pass, and
# the manifest's counts must be the witnesses fed in.
KBASE="${KGEN%.v}"
kernel_jobs() {
  local n mem per
  n="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
  per="${KERNEL_PART_MB:-3000}"
  mem="$(awk '/^MemAvailable:/ {print int($2 / 1024)}' /proc/meminfo 2>/dev/null || true)"
  if [ -n "$mem" ] && [ "$((mem / per))" -lt "$n" ]; then n="$((mem / per))"; fi
  [ "$n" -ge 1 ] || n=1
  echo "$n"
}
kernel_run() { # <manifest> <witnesses>: 0 only when the counts hold and every part passes
  python3 "$ROOT/proofs/lib/kernel_parts.py" check "$1" "$2" || return 1
  KTIME=0; if /usr/bin/time -f %M -o /dev/null true 2>/dev/null; then KTIME=1; fi
  export COQC KTIME
  tail -n +3 "$1" | cut -f1 | (cd "$ROOT/proofs" && xargs -P "$(kernel_jobs)" -I{} sh -c '
    if [ "$KTIME" = 1 ]; then /usr/bin/time -f %M -o "$1.rss" "$COQC" -Q . AlmideTrust "$1" >/dev/null 2>&1 || exit 255
    else "$COQC" -Q . AlmideTrust "$1" >/dev/null 2>&1 || exit 255; fi' _ {})
}
kernel_clean() { # <manifest>
  [ -f "$1" ] || return 0
  tail -n +3 "$1" | cut -f1 | while IFS= read -r k; do rm -f "$k" "$k.rss" "${k%.v}.vo" "${k%.v}.vos" "${k%.v}.vok" "${k%.v}.glob"; done
  rm -f "$1"
}
if kernel_run "$KBASE.lst" "$(cat "$KBASE.count")"; then
  KPARTS="$(($(wc -l < "$KBASE.lst") - 2))"
  KRSS="$(cat "$KBASE"_p*.v.rss 2>/dev/null | sort -n | tail -1)"
  echo "  KERNEL OK: every structural witness verdict certified by the Rocq kernel in $(( $(date +%s) - KSTART ))s (vm_compute, $KPARTS part(s), $(kernel_jobs) at once, peak ${KRSS:-?} KiB per part)"
  kernel_clean "$KBASE.lst"
else
  echo "STRUCTURAL WALL FAIL: the KERNEL disagrees with the extracted checker on a structural witness (extraction divergence), or a kernel part is missing or short (above)." >&2
  kernel_clean "$KBASE.lst"
  exit 1
fi
# The split's own drill: two honest witnesses, each its own part, pass; a
# corrupted third in a later part fails the run; so does a missing part.
if ! kernel_run "$KBASE"_dh.lst 2; then
  echo "STRUCTURAL WALL FAIL: kernel drill: the two honest drill parts were not accepted" >&2; exit 1
fi
if kernel_run "$KBASE"_dc.lst 3 2>/dev/null; then
  echo "STRUCTURAL WALL FAIL: kernel drill: a corrupted witness in part 3 of 3 was accepted" >&2; exit 1
fi
rm -f "$KBASE"_dh_p1.v
if kernel_run "$KBASE"_dh.lst 2 2>/dev/null; then
  echo "STRUCTURAL WALL FAIL: kernel drill: a run with a part file gone was accepted" >&2; exit 1
fi
echo "  KERNEL OK: the split drill — honest parts accepted, a corrupted witness in a later part and a missing part refused"
kernel_clean "$KBASE"_dh.lst; kernel_clean "$KBASE"_dc.lst; rm -f "$KBASE.count" "$KBASE.one"

if [ "$MODE" = tests ]; then
  echo
  echo "STRUCTURAL WALL OK (stdlib-test leg): over the $N spec/stdlib test module(s) 'almide test --target wasm'"
  echo "renders on the structural leg, every ownership, call-mode, bounds, init, fields, name-totality, capability"
  echo "and call-graph witness is accepted — the extracted checker, the Rocq kernel and almide-verify agreeing —"
  echo "every located event is the instruction the judged bytes hold, and those bytes pass their tests under wasmtime."
  exit 0
fi

# POISON LEG (#3259), the negative side of the sweep above: the sweep only
# shows that honest structural witnesses ACCEPT. proofs/poisoned-certs/
# *structural*.cert are witnesses the structural recorder (witness.rs) writes
# for an unsafe emission, each of which every verdict must REJECT.
#   3259-structural-read-after-free.cert — an owned block read after its
#   release (`ibdb`). The recorder logs every read of a droppable local as
#   the `b` probe on the block it holds; before #3259 a read logged nothing,
#   so the line was a balanced `id`. witness_tests.rs pins this file to the
#   recorder's own output.
echo "== POISON LEG (#3259): every structural poisoned certificate is REJECTED by all three verdicts =="
SPOISONED=("$ROOT"/proofs/poisoned-certs/*structural*.cert)
if [ ! -e "${SPOISONED[0]}" ]; then
  echo "STRUCTURAL WALL FAIL: proofs/poisoned-certs/ holds no structural certificate — the poison leg would pass vacuously." >&2
  exit 1
fi
PGEN="$(mktemp /tmp/KernelStructuralPoison_XXXXXX).v"
python3 - "${SPOISONED[@]}" > "$PGEN" <<'PYEOF'
import sys
print("From AlmideTrust Require Import OwnershipChecker.")
print("From Stdlib Require Import String.")
print("Open Scope string_scope.")
for path in sys.argv[1:]:
    print('Goal check_xc "%s" = false.' % open(path).read().replace('"', '""'))
    print("Proof. vm_compute. reflexivity. Qed.")
PYEOF
for cert in "${SPOISONED[@]}"; do
  name="${cert#"$ROOT"/}"
  set +e
  ./checker ownership "$cert" >/dev/null 2>&1; XRC=$?
  "$VERIFY" ownership "$cert" >/dev/null 2>&1; PRC=$?
  set -e
  if [ "$XRC" -ne 1 ] || [ "$PRC" -ne 1 ]; then
    echo "STRUCTURAL WALL FAIL: $name was not rejected (extracted checker exit $XRC, almide-verify exit $PRC; want 1 and 1)." >&2
    rm -f "$PGEN"; exit 1
  fi
  echo "  REJECT $name (extracted checker + almide-verify)"
done
if (cd "$ROOT/proofs" && "$COQC" -Q . AlmideTrust "$PGEN" >/dev/null 2>&1); then
  echo "  KERNEL OK: the Rocq kernel rejects all ${#SPOISONED[@]} structural poisoned certificate(s)"
  rm -f "$PGEN" "${PGEN%.v}.vo" "${PGEN%.v}.vos" "${PGEN%.v}.vok" "${PGEN%.v}.glob"
else
  echo "STRUCTURAL WALL FAIL: the KERNEL did not reject a structural poisoned certificate (check_xc = false failed)." >&2
  rm -f "$PGEN"; exit 1
fi

# BOUNDS / INIT POISON LEG (#4005, #4013): proofs/poisoned-certs/<property>/*.cert
# are the witnesses the structural emitter writes with a bounds check or a
# slot store lost — scripts/witness-reinject.sh <property> re-injects each
# bug and pins its witness to
# these files. Every verdict must REJECT each one, on every run.
for prop in bounds init fields; do
  case "$prop" in bounds) PFN=check_lbounds_cert; PMOD=RoutineLinCert ;; init) PFN=check_init_cert; PMOD=InitCert ;; fields) PFN=check_fields_cert; PMOD=FieldCert ;; esac
  echo "== $(printf %s "$prop" | tr a-z A-Z) POISON LEG: every lost-${prop} witness is REJECTED by all three verdicts =="
  PPOISONED=("$ROOT"/proofs/poisoned-certs/$prop/*.cert)
  if [ ! -e "${PPOISONED[0]}" ]; then
    echo "STRUCTURAL WALL FAIL: proofs/poisoned-certs/$prop/ holds no certificate — the $prop poison leg would pass vacuously." >&2
    exit 1
  fi
  PGEN="$(mktemp /tmp/KernelPoison_XXXXXX).v"
  python3 - "$PMOD" "$PFN" "${PPOISONED[@]}" > "$PGEN" <<'PYEOF'
import sys
mod, fn = sys.argv[1], sys.argv[2]
print("From AlmideTrust Require Import %s." % mod)
print("From Stdlib Require Import String.")
print("Open Scope string_scope.")
for path in sys.argv[3:]:
    print('Goal %s "%s" = false.' % (fn, open(path).read().replace('"', '""')))
    print("Proof. vm_compute. reflexivity. Qed.")
PYEOF
  for cert in "${PPOISONED[@]}"; do
    name="${cert#"$ROOT"/}"
    set +e
    ./checker "$prop" "$cert" >/dev/null 2>&1; XRC=$?
    "$VERIFY" "$prop" "$cert" >/dev/null 2>&1; PRC=$?
    set -e
    if [ "$XRC" -ne 1 ] || [ "$PRC" -ne 1 ]; then
      echo "STRUCTURAL WALL FAIL: $name was not rejected (extracted checker exit $XRC, almide-verify exit $PRC; want 1 and 1)." >&2
      rm -f "$PGEN"; exit 1
    fi
    echo "  REJECT $name (extracted checker + almide-verify)"
  done
  if (cd "$ROOT/proofs" && "$COQC" -Q . AlmideTrust "$PGEN" >/dev/null 2>&1); then
    echo "  KERNEL OK: the Rocq kernel rejects all ${#PPOISONED[@]} $prop poisoned certificate(s)"
    rm -f "$PGEN" "${PGEN%.v}.vo" "${PGEN%.v}.vos" "${PGEN%.v}.vok" "${PGEN%.v}.glob"
  else
    echo "STRUCTURAL WALL FAIL: the KERNEL did not reject a $prop poisoned certificate ($PFN = false failed)." >&2
    rm -f "$PGEN"; exit 1
  fi
done

echo
echo "STRUCTURAL WALL OK: over the $N certified spec/wasm_cross fixtures, every ownership, call-mode,"
echo "bounds, init, fields, name-totality, capability and call-graph witness of the structural build is accepted —"
echo "the extracted checker, the Rocq kernel and almide-verify agreeing on every verdict — and"
echo "every bundle names the SHA-256 of the file 'almide build --target wasm' shipped for it."
