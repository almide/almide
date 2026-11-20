#!/usr/bin/env bash
# A measurement, not a gate: for every certified fixture and every stdlib test
# module, build the artifact the structural wall judges, then record
#   - the sizes of the shipped wasm, the certificate bundle, the anchor file
#     and the linear certificate,
#   - the wall time of almide-verify (the witness check alone) and of
#     almide-anchor-check (the check against the shipped bytes),
#   - the anchor check's verdict and its row-4 census line.
# One TSV row per artifact in $OUT/rows.tsv; totals on stdout. It runs none of
# the kernel judges (no Rocq needed) and fails on nothing but a build error.
#
#   scripts/probe-witness-census.sh <out-dir>
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:?usage: scripts/probe-witness-census.sh <out-dir>}"
mkdir -p "$OUT/work" "$OUT/keep"
OUT="$(cd "$OUT" && pwd)"
W="$OUT/work"

cargo build -q --bin almide || exit 1
cargo build -q -p almide-wasm --example emit_structural_bundle || exit 1
cargo build -q --release -p almide-verify -p almide-anchor-check -p almide-lin-prove || exit 1
TD="${CARGO_TARGET_DIR:-$ROOT/target}"
ALMIDE="$TD/debug/almide"
PRODUCER="$TD/debug/examples/emit_structural_bundle"
VERIFY="$TD/release/almide-verify"
ANCHORCHECK="$TD/release/almide-anchor-check"
LINPROVE="$TD/release/almide-lin-prove"

now() { date +%s%N; }
size() { if [ -f "$1" ]; then wc -c < "$1" | tr -d ' '; else echo 0; fi; }

printf 'leg\tfile\tproduced\twasm_bytes\tbundle_bytes\tanchors_bytes\tlcert_bytes\tverify_rc\tverify_ns\tanchor_rc\tanchor_ns\tlinprove_ns\tcensus\tverdict\n' > "$OUT/rows.tsv"

one() { # one <leg> <file>
  local leg="$1" f="$2" rc=0
  rm -f "$W"/a.*
  if [ "$leg" = tests ]; then
    "$PRODUCER" --tests "$f" --write-artifact "$W/a.wasm" --anchors "$W/a.anchors" > "$W/a.bundle" 2> "$W/a.err" || rc=$?
  else
    "$ALMIDE" build "$f" --target wasm -o "$W/a.wasm" > "$W/a.err" 2>&1 || rc=$?
    if [ "$rc" -eq 0 ]; then
      "$PRODUCER" "$f" --artifact "$W/a.wasm" --anchors "$W/a.anchors" > "$W/a.bundle" 2> "$W/a.err" || rc=$?
    fi
  fi
  if [ "$rc" -ne 0 ]; then
    printf '%s\t%s\tno:%s\t0\t0\t0\t0\t-\t0\t-\t0\t0\t-\t%s\n' "$leg" "$f" "$rc" "$(grep -m1 . "$W/a.err" | cut -c1-160 | tr '\t' ' ')" >> "$OUT/rows.tsv"
    return
  fi
  local t0 t1 vrc=0 arc=0 vns ans lns=0
  t0=$(now); "$VERIFY" bundle "$W/a.bundle" > "$W/a.verify" 2>&1 || vrc=$?; t1=$(now); vns=$((t1 - t0))
  t0=$(now); "$ANCHORCHECK" "$W/a.wasm" "$W/a.bundle" "$W/a.anchors" > "$W/a.anchor" 2>&1 || arc=$?; t1=$(now); ans=$((t1 - t0))
  local lin=()
  if grep -q '^prims .' "$W/a.anchors"; then
    t0=$(now)
    if "$LINPROVE" "$W/a.wasm" "$W/a.anchors" > "$W/a.lcert" 2> "$W/a.lprove" \
      && "$ANCHORCHECK" --lin-verify "$W/a.wasm" "$W/a.anchors" "$W/a.lcert" > "$W/a.lin" 2>&1; then
      [ -s "$W/a.lcert" ] && lin=(--lin "$W/a.lcert")
    fi
    t1=$(now); lns=$((t1 - t0))
  fi
  local census="-"
  if [ "$arc" -eq 0 ]; then
    census="$("$ANCHORCHECK" --census "${lin[@]}" "$W/a.wasm" "$W/a.bundle" "$W/a.anchors" 2>&1 | grep -m1 '^CENSUS' || echo -)"
  fi
  if [ "$arc" -eq 0 ]; then
    "$ANCHORCHECK" --census "${lin[@]}" "$W/a.wasm" "$W/a.bundle" "$W/a.anchors" 2>/dev/null | grep '^CENSUSFN' > "$W/a.fn" || true
    python3 scripts/probe-witness-census-split.py "$leg" "$f" "$W/a.bundle" "$W/a.anchors" "$W/a.fn" "$OUT/fnrows.tsv" || echo "split failed: $f" >&2
  fi
  local verdict
  verdict="$(grep -m1 -E 'REJECT|OK' "$W/a.anchor" | cut -c1-240 | tr '\t' ' ')"
  printf '%s\t%s\tyes\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$leg" "$f" \
    "$(size "$W/a.wasm")" "$(size "$W/a.bundle")" "$(size "$W/a.anchors")" "$(size "$W/a.lcert")" \
    "$vrc" "$vns" "$arc" "$ans" "$lns" "$census" "$verdict" >> "$OUT/rows.tsv"
  # Keep what a rejection needs to be read by hand.
  if [ "$arc" -ne 0 ]; then
    local d="$OUT/keep/$(basename "$f" .almd)"
    mkdir -p "$d"
    cp "$W/a.wasm" "$W/a.bundle" "$W/a.anchors" "$W/a.anchor" "$d/" 2>/dev/null
    "$ANCHORCHECK" --explain "$W/a.wasm" "$W/a.bundle" "$W/a.anchors" > "$d/explain.txt" 2>&1 || true
    if command -v wasm2wat >/dev/null 2>&1; then
      wasm2wat --enable-all "$W/a.wasm" -o "$d/a.wat" 2> "$d/wat.err" || true
    fi
  fi
}

grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt | while IFS= read -r f; do one fixtures "$f"; done
ls spec/stdlib/*_test.almd | while IFS= read -r f; do one tests "$f"; done

python3 - "$OUT/rows.tsv" <<'PY'
import csv, re, sys, statistics
rows = list(csv.DictReader(open(sys.argv[1]), delimiter="\t", quoting=csv.QUOTE_NONE))
for leg in ("fixtures", "tests"):
    rs = [r for r in rows if r["leg"] == leg]
    ok = [r for r in rs if r["produced"] == "yes"]
    acc = [r for r in ok if r["anchor_rc"] == "0"]
    print(f"== {leg}: {len(rs)} listed, {len(ok)} produced, {len(acc)} accepted by the anchor check, {len(ok) - len(acc)} rejected")
    if not ok:
        continue
    def tot(k): return sum(int(r[k]) for r in ok)
    w, b, a, l = tot("wasm_bytes"), tot("bundle_bytes"), tot("anchors_bytes"), tot("lcert_bytes")
    print(f"   bytes: wasm {w}, bundle {b} ({b / w:.3f}x), anchors {a} ({a / w:.3f}x), linear cert {l} ({l / w:.3f}x)")
    for k, name in (("verify_ns", "almide-verify"), ("anchor_ns", "anchor check"), ("linprove_ns", "linear prover + its check")):
        v = sorted(int(r[k]) / 1e6 for r in ok)
        print(f"   {name}: total {sum(v) / 1000:.1f} s, median {statistics.median(v):.1f} ms, p95 {v[int(len(v) * 0.95) - 1]:.1f} ms, max {v[-1]:.1f} ms")
    t = [0] * 10
    n = 0
    for r in acc:
        m = [int(x) for x in re.findall(r"\d+", r["census"])]
        if len(m) == 10:
            n += 1
            t = [x + y for x, y in zip(t, m)]
    def pct(x, y): return f"{100 * x / y:.1f}%" if y else "n/a"
    print(f"   census over {n} artifact(s):")
    print(f"     certified frames: {t[0]} access(es), {t[1]} located ({pct(t[1], t[0])}; {t[2]} by the linear certificate), {t[3]} header word(s), {t[4]} bulk")
    print(f"     runtime routines: {t[5]} access(es), {t[6]} located ({pct(t[6], t[5])}; {t[7]} by the linear certificate), {t[8]} header word(s), {t[9]} bulk")
    print(f"     whole artifact:   {t[0] + t[5]} access(es), {t[1] + t[6]} located ({pct(t[1] + t[6], t[0] + t[5])})")
    for r in ok:
        if r["anchor_rc"] != "0":
            print(f"   REJECT {r['file']}: {r['verdict']}")
PY

python3 - "$OUT/fnrows.tsv" <<'PY2'
import sys, collections
rows = [l.rstrip("\n").split("\t") for l in open(sys.argv[1])]
hows = collections.Counter()
seen = set()
agg = collections.defaultdict(lambda: [0] * 9)
for r in rows:
    leg, file, how, klass, name = r[:5]
    func, frame = r[5], r[6]
    n = [int(x) for x in r[7:15]]
    if (leg, file) not in seen:
        seen.add((leg, file))
        hows[(leg, how)] += 1
    key = (leg, klass, "frame" if frame == "1" else "not-frame")
    a = agg[key]
    a[0] += 1
    for i, v in enumerate(n):
        a[i + 1] += v
print("== per-function split (how each artifact's names were placed): " + ", ".join(f"{k[0]}/{k[1]} {v}" for k, v in sorted(hows.items())))
print("   leg      class    frame      fns  accesses located  (linear) header   bulk  other-load other-store other-byte  located%  located+header+bulk%")
for key in sorted(agg):
    a = agg[key]
    acc = a[1] or 1
    print(f"   {key[0]:8} {key[1]:8} {key[2]:9} {a[0]:6} {a[1]:8} {a[2]:7} {a[3]:9} {a[4]:6} {a[5]:6} {a[6]:10} {a[7]:11} {a[8]:10}  {100 * a[2] / acc:7.1f}%  {100 * (a[2] + a[4] + a[5]) / acc:7.1f}%")
PY2
