#!/usr/bin/env bash
# NATIVE v1 PERF SHAPES GATE (#4006) — the second check behind
# scripts/check-native-v1-perf.sh's "a shape no shape bench or certified
# program exercises" blind spot.
#
# The v1-vs-v0 perf gate measures whole programs: the certified floor and the
# shape benches. A render path none of them reaches is never timed, so a
# program outside the corpus that takes it could run slower than v0 and no
# gate would know. This gate makes that set visible and holds it.
#
# What it checks: the verified native renderer (crates/almide-mir/src/
# render_native*.rs) is built with coverage instrumentation, and the perf
# gate's corpus (every spec/ program on proofs/native-verified-floor.txt plus
# research/benchmark/perf/v1-shapes/*.almd) is rendered through it
# (crates/almide-mir/examples/render_corpus.rs). A code region of the
# renderer that EMITS Rust (its text contains `line!(`, `push_str(`,
# `writeln!(` or `emit(`) and that no corpus program executes is an
# unexercised render path. They are counted per renderer function and held
# against proofs/native-v1-perf-shapes-baseline.txt:
#
# - a function above its recorded count fails: a new render path landed with
#   no program in the perf corpus that takes it (add a shape bench that does,
#   so check-native-v1-perf.sh times it), or a corpus program stopped
#   reaching one;
# - a function below its recorded count fails too: lower its row (the
#   baseline only shrinks, `--update` rewrites it);
# - a corpus program that no longer renders fails (the corpus went blind).
#
# The baseline is a DEBT list: each row names render paths that exist and no
# perf program exercises yet. It is not an allowance — it may only fall.
#
# Negative control: NATIVE_V1_SHAPES_DROP=<regex> leaves the matching
# programs out of the corpus; dropping the floor programs named a-m
# (`^spec/wasm_cross/[a-m]`) must raise a count and exit 1
# (proofs/gate-verification.toml; the nightly runs it).
set -uo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

BASELINE="proofs/native-v1-perf-shapes-baseline.txt"
UPDATE=0
[ "${1:-}" = "--update" ] && UPDATE=1
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

SYSROOT="$(rustc --print sysroot)"
LLVM_BIN="$(echo "$SYSROOT"/lib/rustlib/*/bin | awk '{print $1}')"
if [ ! -x "$LLVM_BIN/llvm-cov" ]; then
  rustup component add llvm-tools >/dev/null 2>&1 || rustup component add llvm-tools-preview >/dev/null 2>&1
  LLVM_BIN="$(echo "$SYSROOT"/lib/rustlib/*/bin | awk '{print $1}')"
fi
[ -x "$LLVM_BIN/llvm-cov" ] || { echo "native-v1-perf-shapes: llvm-cov not found under $SYSROOT"; exit 2; }

TARGET="${NATIVE_V1_SHAPES_TARGET_DIR:-target/shapes-cov}"
if ! RUSTFLAGS="-C instrument-coverage" LLVM_PROFILE_FILE="$WORK/build-%m-%p.profraw" \
  cargo build -q --release -p almide-mir --example render_corpus --target-dir "$TARGET" >"$WORK/build.log" 2>&1; then
  echo "native-v1-perf-shapes: the instrumented build failed"; tail -20 "$WORK/build.log"; exit 2
fi
BIN="$TARGET/release/examples/render_corpus"

{ grep -E '^spec/' proofs/native-verified-floor.txt; ls research/benchmark/perf/v1-shapes/*.almd; } \
  | grep -vE "${NATIVE_V1_SHAPES_DROP:-^\$}" > "$WORK/corpus.txt"
LLVM_PROFILE_FILE="$WORK/run-%m.profraw" "$BIN" < "$WORK/corpus.txt" > "$WORK/verdicts.txt"
fail=0
# A floor program that no longer renders blinds the corpus; a shape bench
# that does not render is not compared by the perf gate either (UNCERTIFIED
# there), so it is reported and its paths simply do not count.
if grep -v $'\tOK$' "$WORK/verdicts.txt" | grep -q '^spec/'; then
  echo "native-v1-perf-shapes: floor programs that do not render:"
  grep -v $'\tOK$' "$WORK/verdicts.txt" | grep '^spec/' | head -20
  fail=1
fi
grep -v $'\tOK$' "$WORK/verdicts.txt" | grep -v '^spec/' | sed 's/^/UNCERTIFIED /'

"$LLVM_BIN/llvm-profdata" merge -sparse "$WORK"/run-*.profraw -o "$WORK/run.profdata" || { echo "native-v1-perf-shapes: profdata merge failed"; exit 2; }
"$LLVM_BIN/llvm-cov" export "$BIN" -instr-profile="$WORK/run.profdata" -skip-expansions > "$WORK/cov.json" \
  || { echo "native-v1-perf-shapes: llvm-cov export failed"; exit 2; }

python3 - "$WORK/cov.json" "$BASELINE" "$UPDATE" "$fail" "$LLVM_BIN/llvm-cxxfilt" <<'PY'
import json, re, sys, os, subprocess
cov, baseline, update, fail, cxxfilt = sys.argv[1], sys.argv[2], sys.argv[3] == "1", int(sys.argv[4]), sys.argv[5]
root = os.getcwd() + "/"
EMIT = ("line!(", "push_str(", "writeln!(", "emit(")
data = json.load(open(cov))["data"][0]
texts = {}

def text_of(path, l0, c0, l1, c1):
    if path not in texts:
        texts[path] = open(path).read().split("\n")
    ls = texts[path][l0 - 1:l1]
    if not ls:
        return ""
    if len(ls) == 1:
        return ls[0][c0 - 1:c1 - 1]
    return "\n".join([ls[0][c0 - 1:]] + ls[1:-1] + [ls[-1][:c1 - 1]])

names = sorted({f["name"] for f in data["functions"]})
plain = subprocess.run([cxxfilt], input="\n".join(names), capture_output=True, text=True).stdout.split("\n")
# Drop the crate disambiguators (`almide_mir[1a2b3c]`) and legacy hashes, so
# a row survives a rebuild.
demangled = {n: re.sub(r"::h[0-9a-f]{16}$", "", re.sub(r"\[[0-9a-f]+\]", "", d)) for n, d in zip(names, plain)}

def demangle(name):
    return demangled.get(name, name)

counts = {}
seen_fns = set()
for f in data["functions"]:
    files = f["filenames"]
    for r in f["regions"]:
        l0, c0, l1, c1, n, fid, _exp, kind = r[:8]
        path = files[fid]
        rel = path[len(root):] if path.startswith(root) else path
        if not re.match(r"crates/almide-mir/src/render_native[a-z_]*\.rs$", rel):
            continue
        key = (rel, demangle(f["name"]))
        seen_fns.add(key)
        if kind != 0 or n != 0:
            continue
        if any(t in text_of(path, l0, c0, l1, c1) for t in EMIT):
            counts[key] = counts.get(key, 0) + 1

rows = {}
if os.path.exists(baseline):
    for line in open(baseline):
        if not line.strip() or line.startswith("#"):
            continue
        rel, fn, n = line.rstrip("\n").split("\t")
        rows[(rel, fn)] = int(n)

if update:
    hdr = [l for l in open(baseline)] if os.path.exists(baseline) else []
    hdr = [l for l in hdr if l.startswith("#")]
    with open(baseline, "w") as out:
        out.writelines(hdr)
        for k in sorted(counts):
            out.write(f"{k[0]}\t{k[1]}\t{counts[k]}\n")
    print(f"native-v1-perf-shapes: baseline rewritten ({sum(counts.values())} unexercised render paths in {len(counts)} functions)")
    sys.exit(fail)

errs = []
for k, n in sorted(counts.items()):
    b = rows.get(k, 0)
    if n > b:
        errs.append(f"UNEXERCISED {k[0]} {k[1]}: {n} render path(s) no perf program takes (baseline {b}) — add a shape bench to research/benchmark/perf/v1-shapes/")
for k, b in sorted(rows.items()):
    n = counts.get(k, 0)
    if n < b:
        errs.append(f"SHRINK {k[0]} {k[1]}: {n} unexercised (baseline {b}) — lower its row in {baseline} (--update)")
if not seen_fns:
    errs.append("no renderer function in the coverage map — the instrumentation went blind")
for e in errs:
    print(e)
total = sum(counts.values())
if errs or fail:
    print(f"native-v1-perf-shapes: FAILED ({total} unexercised render paths, baseline {sum(rows.values())})")
    sys.exit(1)
print(f"native-v1-perf-shapes: ok ({total} unexercised render paths across {len(counts)} functions, all recorded; {len(seen_fns)} renderer functions mapped)")
PY
