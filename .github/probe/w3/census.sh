#!/bin/bash
# census.sh <src-tree> <outdir> [tag]: per-function linear-certificate census of
# the self-host json/value bodies over the json/value fixtures and stdlib tests.
set -uo pipefail
T=$1; O=$2; tag=${3:-head}; mkdir -p "$O"
cd "$T"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$T/target}"
cargo build -q --release --bin almide 2>&1 | tail -3
cargo build -q --release -p almide-lin-prove -p almide-anchor-check -p almide-verify 2>&1 | tail -3
cargo build -q --release -p almide-wasm --example emit_structural_bundle 2>&1 | tail -3
R=$CARGO_TARGET_DIR/release
A=$R/almide; P=$R/examples/emit_structural_bundle; LP=$R/almide-lin-prove; AC=$R/almide-anchor-check
: > "$O/rows-$tag.tsv"
k=0
LIST=$(command ls spec/wasm_cross/ | grep -E '^(json|value|codec)' | sed 's|^|spec/wasm_cross/|'; command ls spec/stdlib/ | grep -E '^(json|value)' | sed 's|^|spec/stdlib/|')
[ -f /tmp/probe/w3/census-only ] && LIST=$(grep -v '^#' /tmp/probe/w3/census-only)
for f in $LIST; do
  k=$((k+1)); d="$O/$tag/$k"; mkdir -p "$d"
  case "$f" in
    spec/stdlib/*) "$P" --tests "$f" --write-artifact "$d/a.wasm" --anchors "$d/a.anchors" > "$d/a.bundle" 2> "$d/err" || { echo "SKIP(test-route rc=$?) $f"; continue; } ;;
    *) "$A" build "$f" --target wasm -o "$d/a.wasm" > "$d/err" 2>&1 || { echo "SKIP(build) $f"; continue; }
       "$P" "$f" --artifact "$d/a.wasm" --anchors "$d/a.anchors" > "$d/a.bundle" 2>> "$d/err" || { echo "SKIP(producer) $f: $(head -c 200 $d/err)"; continue; } ;;
  esac
  if grep -q '^prims .' "$d/a.anchors"; then
    "$LP" "$d/a.wasm" "$d/a.anchors" > "$d/a.lcert" 2> "$d/a.lprove" || echo "LINPROVE FAIL $f"
    "$AC" --lin-verify "$d/a.wasm" "$d/a.anchors" "$d/a.lcert" > "$d/a.lin" 2>&1 || echo "LINVERIFY REJECT $f: $(cat $d/a.lin)"
    "$AC" --census --lin "$d/a.lcert" "$d/a.wasm" "$d/a.bundle" "$d/a.anchors" 2>&1 | grep -E '^(CENSUS|ANCHORS REJECT)' > "$d/a.census"
    "$R/almide-verify" bounds "$d/a.lcert" > "$d/a.lverify" 2>&1; echo "verify-rc $?" >> "$d/a.lverify"
    grep -E '^(certified|uncertified)' "$d/a.lprove" | sed "s|^|$f\t|" >> "$O/rows-$tag.tsv"
    echo "$f :: $(cat $d/a.lin) :: $(tail -1 $d/a.lverify) :: $(cut -c1-200 $d/a.census)"
  else
    echo "$f :: no prim sites"
  fi
done
# The translation of the json bodies of one artifact, holes as `_`.
for d in "$O/$tag"/*/; do
  if [ -f "$d/a.anchors" ] && grep -q "json_parse" "$d/a.anchors"; then
    "$AC" --lin-skel "$d/a.wasm" "$d/a.anchors" > "$O/skel-$tag.txt" 2>&1
    python3 - "$d/a.anchors" "$O/skel-$tag.txt" "${SKEL_FNS:-json_parse,__jp_byte,__jp_word}" <<'PY2'
import sys, re
anch, skel, want = open(sys.argv[1]).read(), open(sys.argv[2]).read().splitlines(), sys.argv[3].split(",")
line = next((l for l in anch.splitlines() if l.startswith("selfhost ")), "")
names = {}
for t in line.split()[1:]:
    parts = t.split(":")
    if len(parts) >= 3: names[parts[0]] = re.sub(r"^__selfhost_\d+\.", "", parts[2])
for l in skel:
    m = re.match(r"Q(\d+)\{", l)
    if m and names.get(m.group(1)) in want:
        print("SKEL", names[m.group(1)], l[:6000])
PY2
    break
  fi
done
python3 - "$O/rows-$tag.tsv" <<'PY'
import sys, collections
rows = [l.rstrip("\n").split("\t") for l in open(sys.argv[1]) if l.strip()]
agg = collections.OrderedDict()
for r in rows:
    f, st, pos, name, n = r[0], r[1], r[2], r[3], int(r[4])
    why = r[5] if len(r) > 5 else ""
    a = agg.setdefault(name, {"sites": n, "cert": 0, "uncert": 0, "why": collections.Counter()})
    a["sites"] = max(a["sites"], n)
    if st == "certified": a["cert"] += 1
    else: a["uncert"] += 1; a["why"][why[:160]] += 1
print("\n== per-function census (name, prim sites, #artifacts certified, #artifacts uncertified, counted sites when uncertified, reasons)")
tot = 0
for name, a in sorted(agg.items()):
    counted = a["sites"] if a["uncert"] else 0
    tot += counted
    print(f"{name}\t{a['sites']}\tcert={a['cert']}\tuncert={a['uncert']}\tcounted={counted}\t" + " | ".join(f"{w} (x{c})" for w, c in a["why"].most_common(3)))
print("TOTAL counted sites (functions uncertified in any artifact):", tot)
PY
