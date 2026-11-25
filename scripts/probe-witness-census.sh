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

# The checker as it was before the guard fixes (OLD_REF), carrying this
# branch's probe tooling: the control mutants are judged by both.
ANCHOROLD=""
if [ -n "${OLD_REF:-}" ]; then
  git fetch -q --depth=1 origin "$OLD_REF" || exit 1
  git worktree add -q --detach "$OUT/old" FETCH_HEAD || exit 1
  for f in census.rs main.rs mutate.rs; do cp "crates/almide-anchor-check/src/$f" "$OUT/old/crates/almide-anchor-check/src/$f"; done
  sed -i 's/census_with, located,/census_with, census_writes, located,/' "$OUT/old/crates/almide-anchor-check/src/lib.rs"
  (cd "$OUT/old" && CARGO_TARGET_DIR="$OUT/old-target" cargo build -q --release -p almide-anchor-check) || exit 1
  ANCHOROLD="$OUT/old-target/release/almide-anchor-check"
  echo "old checker: $OLD_REF at $(git -C "$OUT/old" rev-parse --short HEAD)"
fi

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
    census="$("$ANCHORCHECK" --census "${lin[@]}" "$W/a.wasm" "$W/a.bundle" "$W/a.anchors" 2>&1 | grep -m1 '^CENSUS:' || echo -)"
  fi
  if [ "$arc" -eq 0 ]; then
    "$ANCHORCHECK" --census "${lin[@]}" "$W/a.wasm" "$W/a.bundle" "$W/a.anchors" 2>/dev/null > "$W/a.census" || true
    grep '^CENSUSFN' "$W/a.census" > "$W/a.fn" || true
    if [ "${CENSUS_LIGHT:-0}" != 1 ]; then
      "$ANCHORCHECK" --mutate "$W/a.wasm" "$W/a.bundle" "$W/a.anchors" 2>/dev/null | awk -v l="$leg" -v f="$f" '{print l "\t" f "\t" $0}' >> "$OUT/mutrows.tsv" || true
    fi
    grep '^WINDOWFN' "$W/a.census" | awk -v l="$leg" -v f="$f" '{$1=""; print l "\t" f "\t" substr($0,2)}' >> "$OUT/windowrows.tsv" || true
    grep '^STOREFN' "$W/a.census" | awk -v l="$leg" -v f="$f" '{$1=""; print l "\t" f "\t" substr($0,2)}' >> "$OUT/storerows.tsv" || true
    if [ -n "$ANCHOROLD" ]; then
      MUT_ONLY=control "$ANCHOROLD" --mutate "$W/a.wasm" "$W/a.bundle" "$W/a.anchors" 2>/dev/null | awk -v l="$leg" -v f="$f" '{print l "\t" f "\t" $0}' >> "$OUT/mutrows-old.tsv" || true
    fi
    grep '^SHAPEFN' "$W/a.census" | awk -v l="$leg" -v f="$f" '{print l "\t" f "\t" $2 "\t" $3 "\t" $4 "\t" $5}' >> "$OUT/shaperows.tsv" || true
    grep '^ROUTINE' "$W/a.census" | awk -v l="$leg" -v f="$f" '{print l "\t" f "\t" $2 "\t" $3 "\t" $4 "\t" $5}' >> "$OUT/routinerows.tsv" || true
    grep '^CENSUSX' "$W/a.census" | awk -v l="$leg" -v f="$f" '{print l "\t" f "\t" $2 "\t" $3 "\t" $4}' >> "$OUT/fxrows.tsv" || true
    grep '^CENSUSY' "$W/a.census" | awk -v l="$leg" -v f="$f" '{print l "\t" f "\t" $2 "\t" $3 "\t" $4}' >> "$OUT/fyrows.tsv" || true
    grep -E '^(ROLEFN|PROVENFN) ' "$W/a.census" | awk -v l="$leg" -v f="$f" '{print l "\t" f "\t" $0}' >> "$OUT/provenrows.tsv" || true
    grep '^BODYFN' "$W/a.census" | awk -v l="$leg" -v f="$f" '{print l "\t" f "\t" $2 "\t" $3 "\t" $4 "\t" $5 "\t" $6 "\t" $7}' >> "$OUT/bodyrows.tsv" || true
    python3 scripts/probe-witness-census-split.py "$leg" "$f" "$W/a.bundle" "$W/a.anchors" "$W/a.fn" "$OUT/fnrows.tsv" || echo "split failed: $f" >&2
  fi
  local verdict
  verdict="$(grep -m1 -E 'REJECT|OK' "$W/a.anchor" | cut -c1-240 | tr '\t' ' ')"
  printf '%s\t%s\tyes\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$leg" "$f" \
    "$(size "$W/a.wasm")" "$(size "$W/a.bundle")" "$(size "$W/a.anchors")" "$(size "$W/a.lcert")" \
    "$vrc" "$vns" "$arc" "$ans" "$lns" "$census" "$verdict" >> "$OUT/rows.tsv"
  # Keep what a rejection needs to be read by hand (and two accepted modules whose mutants are read).
  if [ "$arc" -ne 0 ] || [ "$(basename "$f")" = args_surface.almd ] || [ "$(basename "$f")" = result_eq.almd ]; then
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

python3 - "$OUT/fnrows.tsv" "$OUT/fxrows.tsv" <<'PY3'
import sys, collections
klass = {}
for l in open(sys.argv[1]):
    r = l.rstrip("\n").split("\t")
    klass[(r[0], r[1], r[5])] = (r[3], r[4])
agg = collections.defaultdict(collections.Counter)
names = collections.defaultdict(collections.Counter)
for l in open(sys.argv[2]):
    leg, file, pos, key, n = l.rstrip("\n").split("\t")
    c, name = klass.get((leg, file, pos), ("runtime", "-"))
    agg[(leg, c)][key] += int(n)
    region = key.split("|")[0]
    agg[(leg, c)]["REGION " + region] += int(n)
    if c == "stdlib":
        names[leg][name.split(".")[0] + "." + name.split(".")[-1].split("__")[0]] += int(n)
print("== the accesses no class takes, by where they sit and what they are (top 25 per class)")
for key in sorted(agg):
    tot = sum(v for k, v in agg[key].items() if not k.startswith("REGION "))
    print(f"   --- {key[0]} / {key[1]}: {tot}")
    for k, v in sorted(((k, v) for k, v in agg[key].items() if k.startswith("REGION ")), key=lambda x: -x[1]):
        print(f"       {k:44} {v:8} {100 * v / tot:5.1f}%")
    for k, v in sorted(((k, v) for k, v in agg[key].items() if not k.startswith("REGION ")), key=lambda x: -x[1])[:25]:
        print(f"       {k:44} {v:8} {100 * v / tot:5.1f}%")
for leg in sorted(names):
    print(f"   --- {leg} / stdlib functions holding the most (top 25)")
    for k, v in names[leg].most_common(25):
        print(f"       {k:44} {v:8}")
PY3

echo "== mutants of the located instructions (kind, verdict, count) =="
if [ -s "$OUT/mutrows.tsv" ]; then
  awk -F'\t' '$3 ~ /^MUT / {split($3,a," "); c[$1" "a[2]" "a[3]]+=a[4]} END{for(k in c) print "   " k, c[k]}' "$OUT/mutrows.tsv" | sort
  echo "   survivors on the located instruction or its address or condition operands, by class and instruction shape:"
  grep -P '\tMUTSURV' "$OUT/mutrows.tsv" | awk -F'\t' '{split($3,a," "); w=a[6]; gsub(/[0-9]+/,"N",w); c[$1" "a[2]" "w]++} END{for(k in c) print c[k], k}' | sort -nr | head -40 | sed 's/^/     /' || true
  echo "   examples:"
  grep -P '\tMUTSURV' "$OUT/mutrows.tsv" | awk -F'\t' '{split($3,a," "); k=$1" "a[2]; if (n[k]++ < 3) print}' | head -45 | sed 's/^/     /' || true
fi

if [ -s "$OUT/mutrows-old.tsv" ]; then
  echo "== the control mutants, judged by the checker before the guard fixes =="
  awk -F'\t' '$3 ~ /^MUT / {split($3,a," "); c[$1" "a[2]" "a[3]]+=a[4]} END{for(k in c) print "   " k, c[k]}' "$OUT/mutrows-old.tsv" | sort
  echo "   and by this branch's checker (the same classes):"
  awk -F'\t' '$3 ~ /^MUT / {split($3,a," "); if (a[2] ~ /control|branch-depth|base/) c[$1" "a[2]" "a[3]]+=a[4]} END{for(k in c) print "   " k, c[k]}' "$OUT/mutrows.tsv" | sort
  echo "   survivors under this branch's checker (control, branch-depth), by shape:"
  grep -P '\tMUTSURV [a-z]+/[a-z]+/(abort-nop|arm-escape|arm-escape-early|exit-drop|branch-depth) ' "$OUT/mutrows.tsv" | awk -F'\t' '{split($3,a," "); w=a[6]; gsub(/[0-9]+/,"N",w); c[$1" "a[2]" "w]++} END{for(k in c) print c[k], k}' | sort -nr | head -30 | sed 's/^/     /' || true
  grep -P '\tMUTSURV [a-z]+/[a-z]+/(abort-nop|arm-escape|arm-escape-early|exit-drop|branch-depth) ' "$OUT/mutrows.tsv" | head -12 | sed 's/^/     /' || true
fi

python3 - "$OUT/fnrows.tsv" "$OUT/storerows.tsv" <<'PY4'
import sys, collections
klass = {}
for l in open(sys.argv[1]):
    r = l.rstrip("\n").split("\t")
    klass[(r[0], r[1], r[5])] = r[3]
agg = collections.defaultdict(lambda: [0] * 7)
for l in open(sys.argv[2]):
    leg, file, rest = l.rstrip("\n").split("\t")
    p = rest.split()
    a = agg[(leg, klass.get((leg, file, p[0]), "runtime"))]
    for i, v in enumerate(p[1:8]):
        a[i] += int(v)
print("== instructions that WRITE memory (store, memory.copy, memory.fill), by function class")
print("   leg      class      writes  bounds    init  fields  header    bulk   other  located%  unrecorded")
for key in sorted(agg):
    a = agg[key]
    loc = a[1] + a[2] + a[3]
    print(f"   {key[0]:8} {key[1]:8} {a[0]:8} {a[1]:7} {a[2]:7} {a[3]:7} {a[4]:7} {a[5]:7} {a[6]:7}  {100 * loc / (a[0] or 1):7.1f}%  {a[0] - loc:9}")
PY4

python3 - "$OUT/fnrows.tsv" "$OUT/shaperows.tsv" "$OUT/routinerows.tsv" "$OUT/mutrows.tsv" "$OUT" <<'PY5'
import sys, collections, re
fn, shp, rtn, mut, out = sys.argv[1:6]
klass = {}
acc = collections.Counter()
for l in open(fn):
    r = l.rstrip("\n").split("\t")
    klass[(r[0], r[1], r[5])] = r[3]
    acc[(r[0], r[3])] += int(r[7])
agg = collections.Counter()
try:
    for l in open(shp):
        leg, file, func, shape, wr, n = l.rstrip("\n").split("\t")
        agg[(leg, klass.get((leg, file, func), "runtime"), shape, wr)] += int(n)
except FileNotFoundError:
    pass
print("== located bounds accesses by the shape of their guard (list-index is the shape Proposition 1 covers)")
print("   leg      class    shape          writes?      n   % of the class's accesses")
for key in sorted(agg):
    a = acc[(key[0], key[1])] or 1
    print(f"   {key[0]:8} {key[1]:8} {key[2]:14} {'write' if key[3] == '1' else 'read':6} {agg[key]:8}   {100 * agg[key] / a:6.2f}%")
print("== the routines the anchor file names, by body shape (calls resolved, module-specific operands masked)")
shapes = collections.defaultdict(collections.Counter)
mods = collections.Counter()
try:
    for l in open(rtn):
        leg, file, name, f, h, n = l.rstrip("\n").split("\t")
        shapes[name][(h, n)] += 1
        mods[name] += 1
except FileNotFoundError:
    pass
for name in sorted(shapes):
    print(f"   {name}: {mods[name]} (module, callee) pairs, {len(shapes[name])} distinct body shape(s)")
    for (h, n), c in shapes[name].most_common(12):
        print(f"       {h} {n:>5} instruction(s)  in {c}")
ctx = collections.defaultdict(list)
for l in open(mut):
    p = l.rstrip("\n").split("\t")
    if len(p) < 3 or not p[2].startswith("MUTCTX "):
        continue
    f = p[2].split(" ", 3)
    norm = re.sub(r"\d+", "N", f[3])
    ctx[(p[0], f[1], norm)].append((p[1], f[2], f[3]))
print(f"== the code around the surviving bounds-operand and branch mutants: {sum(len(v) for v in ctx.values())} survivor(s), {len(ctx)} distinct shape(s) (numbers masked)")
with open(out + "/mutctx.txt", "w") as o:
    for i, (key, v) in enumerate(sorted(ctx.items(), key=lambda kv: (-len(kv[1]), kv[0]))):
        files = collections.Counter(x[0] for x in v)
        head = f"#{i} {key[0]} {key[1]}: {len(v)} survivor(s) in {len(files)} file(s), e.g. {v[0][0]} {v[0][1]}"
        print("   " + head)
        o.write(head + "\n")
        o.write("   files: " + ", ".join(f"{k} x{n}" for k, n in files.most_common(8)) + "\n")
        for part in v[0][2].split(" ; "):
            o.write("      " + part + "\n")
        o.write("\n")
PY5
