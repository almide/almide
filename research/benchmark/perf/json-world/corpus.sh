#!/usr/bin/env bash
# The corpus rows (ADR-0033 phase 0): twitter.json (string-heavy),
# citm_catalog.json (key-heavy) and canada.json (float-heavy), parse and
# stringify per operation, for Almide (head and reborn, native and wasm AOT),
# serde_json and yyjson (native and wasm). Alternating rounds on this one
# runner; the median of JW_ROUNDS (default 11) per cell. Almide's wasm
# allocation count per file comes from the embedded host's counter.
# Needs: ALMIDE_HEAD, ALMIDE_REBORN, JW_DEPS (setup.sh), rustc + wasm32-wasip1, wasmtime.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${OUT:-$PWD/corpus-out}"
DEPS="${JW_DEPS:-$PWD/deps}"
ROUNDS="${JW_ROUNDS:-11}"
mkdir -p "$OUT/bin" "$OUT/log"
YY="$DEPS/yyjson/src"
WASI="$DEPS/wasi-sdk"
ab() { local d; d="$(mktemp -d "$OUT/tmp.XXXX")"; TMPDIR="$d" "$@"; local rc=$?; rm -rf "$d"; return $rc; }
for tag in head reborn; do
  var="ALMIDE_${tag^^}"; almide="${!var:-}"
  ab "$almide" build "$HERE/corpus/corpus.almd" -o "$OUT/bin/almide-$tag" --release > "$OUT/log/almide-$tag.log" 2>&1 || echo "BUILD-FAILED almide-$tag"
  ab "$almide" build "$HERE/corpus/corpus.almd" --target wasm -o "$OUT/bin/almide-$tag.wasm" > "$OUT/log/almide-$tag-wasm.log" 2>&1 || echo "BUILD-FAILED almide-$tag.wasm"
done
gcc -O3 -DNDEBUG -I"$HERE/c" -I"$YY" "$HERE/corpus/corpus_yyjson.c" "$YY/yyjson.c" -o "$OUT/bin/yyjson" || echo "BUILD-FAILED yyjson"
"$WASI/bin/clang" --target=wasm32-wasip1 -O3 -DNDEBUG -I"$HERE/c" -I"$YY" "$HERE/corpus/corpus_yyjson.c" "$YY/yyjson.c" -o "$OUT/bin/yyjson.wasm" || echo "BUILD-FAILED yyjson.wasm"
cargo build --release --manifest-path "$HERE/rust/Cargo.toml" --target-dir "$OUT/ct" --bin corpus_serde > "$OUT/log/serde.log" 2>&1 && cp "$OUT/ct/release/corpus_serde" "$OUT/bin/serde" || echo "BUILD-FAILED serde"
cargo build --release --manifest-path "$HERE/rust/Cargo.toml" --target-dir "$OUT/ct" --target wasm32-wasip1 --bin corpus_serde > "$OUT/log/serde-wasm.log" 2>&1 && cp "$OUT/ct/wasm32-wasip1/release/corpus_serde.wasm" "$OUT/bin/serde.wasm" || echo "BUILD-FAILED serde.wasm"
for w in "$OUT"/bin/*.wasm; do wasmtime compile "$w" -o "${w%.wasm}.cwasm" || echo "AOT-FAILED $w"; done
python3 - "$OUT" "$DEPS/corpus" "$ROUNDS" "${ALMIDE_HEAD}" "${ALMIDE_REBORN}" "$HERE/corpus/corpus.almd" <<'PY'
import os, re, statistics, subprocess, sys, shutil
out, corpus, rounds, ah, ar, src = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4], sys.argv[5], sys.argv[6]
b = os.path.join(out, "bin")
impls = []
for name in ["almide-head", "almide-reborn", "serde", "yyjson"]:
    if os.path.exists(os.path.join(b, name)): impls.append((name, "native", [os.path.join(b, name)]))
    if os.path.exists(os.path.join(b, name + ".cwasm")):
        impls.append((name, "wasm-aot", ["wasmtime", "run", "--dir", ".", "--allow-precompiled", os.path.join(b, name + ".cwasm")]))
md = ["# corpus rows (ns per operation, median of %d rounds; lower is better)" % rounds, ""]
for f in ["twitter", "citm_catalog", "canada"]:
    wd = os.path.join(out, "wd-" + f); os.makedirs(wd, exist_ok=True)
    shutil.copy(os.path.join(corpus, f + ".json"), os.path.join(wd, "corpus.json"))
    samples = {(n, k): {"parse": [], "stringify": []} for n, k, _ in impls}
    first = {}
    for r in range(rounds + 1):
        order = impls if r % 2 == 0 else list(reversed(impls))
        for n, k, cmd in order:
            p = subprocess.run(cmd, cwd=wd, capture_output=True, text=True)
            m = re.search(r"CORPUS parse=(\d+) stringify=(\d+)", p.stdout)
            if p.returncode != 0 or not m:
                first[(n, k)] = "FAIL rc=%d %s" % (p.returncode, (p.stderr or p.stdout)[-200:].replace("\n", " "))
                continue
            first.setdefault((n, k), p.stdout.splitlines()[0])
            if r > 0:
                samples[(n, k)]["parse"].append(int(m.group(1))); samples[(n, k)]["stringify"].append(int(m.group(2)))
    md += ["## %s (%d bytes)" % (f, os.path.getsize(os.path.join(wd, "corpus.json"))), "",
           "| impl | leg | parse ms | stringify ms | parse MB/s | first line |", "|---|---|---:|---:|---:|---|"]
    size = os.path.getsize(os.path.join(wd, "corpus.json"))
    for n, k, _ in impls:
        s = samples[(n, k)]
        if s["parse"]:
            pm = statistics.median(s["parse"]) / 1e6; sm = statistics.median(s["stringify"]) / 1e6
            md.append("| %s | %s | %.3f | %.3f | %.1f | %s |" % (n, k, pm, sm, size / 1e6 / (pm / 1e3) if pm else 0, first.get((n, k), "")))
        else:
            md.append("| %s | %s | — | — | — | %s |" % (n, k, first.get((n, k), "")))
    # Almide wasm allocations for the whole program (embedded host counter).
    for tag, almide in (("head", ah), ("reborn", ar)):
        p = subprocess.run([almide, "run", src, "--target", "wasm"], cwd=wd, capture_output=True, text=True,
                           env=dict(os.environ, ALMIDE_WASM_ALLOC_COUNT="1"))
        m = re.search(r"__ALMD_WASM_ALLOC (?:allocs=)?(\d+)", p.stderr)
        md.append("")
        md.append("almide-%s wasm allocations, whole program (1 + 20 parses + 20 stringifies): %s" % (tag, m.group(1) if m else "n/a (rc=%d)" % p.returncode))
    md.append("")
text = "\n".join(md)
open(os.path.join(out, "results.md"), "w").write(text)
print(text)
s = os.environ.get("GITHUB_STEP_SUMMARY")
if s: open(s, "a").write(text + "\n")
PY
