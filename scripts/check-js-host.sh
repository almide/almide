#!/usr/bin/env bash
# JS host gate (#2265). `almide build f.almd --target wasm --host js` writes
# the JS host the compiler itself generates next to the module; this runs
# every spec/wasm_host_js fixture under node through that host and asserts:
#   1. the build succeeds and lands on the leg the fixture's `// @leg:` line
#      names (the structural leg's `@extern(wasm, ..)` wall is tracked here —
#      a route change is a visible diff, never a silent one);
#   2. `run()` prints exactly <fixture>.expected (stdout, byte-compared);
#   3. <fixture>.host.mjs, when present, supplies the `js` hooks and an
#      `after(module)` that exercises the exported wrappers and asserts;
#   4. a fixture with no `@extern` also prints the same lines under the native
#      binary — the host is one more leg of the cross-target contract;
#   5. (#2276) the module ships only what the program uses: the `.wasm` built
#      with `--host js` is byte-identical to the build without it unless some
#      marshalled signature carries a String, in which case the two builds
#      differ by exactly the `__alloc`/`__release` exports (and only then does
#      the glue carry `allocString`);
#   6. (#2276) the set of `wasi.<name>` shims the glue defines equals the set
#      of `wasi_snapshot_preview1` imports of the SHIPPED module — also for the
#      `--wasm-opt` build when wasm-opt is installed (the glue is derived from
#      the optimized bytes, not the renderer's);
#   7. (#2276) the glue's byte size is at or under the fixture's row in
#      spec/wasm_host_js/glue-ceiling.txt — shrinking is silent, growing is a
#      ledger edit in the same change.
#
# Requires: node (>= 18). Locally a missing node skips with a warning; in CI
# it is a failure (the job installs node, so its absence means the gate
# silently stopped gating — the #985 rule).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

BIN="${ALMIDE_BIN:-target/release/almide}"
case "$BIN" in /*) ;; *) BIN="$PWD/$BIN" ;; esac
FIXTURE_DIR="${1:-spec/wasm_host_js}"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT

if ! command -v node >/dev/null; then
  if [ "${CI:-}" = "true" ]; then
    echo "::error::js-host: node not found — in CI a missing tool is a failure (#985)"
    exit 1
  fi
  echo "::warning::node not found — skipping the JS host gate"
  exit 0
fi
"$BIN" --version >/dev/null || { echo "::error::js-host: $BIN does not run"; exit 2; }

# The shipped module's surface, from the bytes: `imports <wasi names>` and
# `exports <names>`, each sorted, one per line.
cat > "$WORK/inspect.mjs" <<'JS'
import { readFileSync } from "node:fs";
const m = new WebAssembly.Module(readFileSync(process.argv[2]));
const imports = WebAssembly.Module.imports(m).filter(i => i.module === "wasi_snapshot_preview1").map(i => i.name).sort();
const exports = WebAssembly.Module.exports(m).map(e => e.name).sort();
console.log("imports " + imports.join(" "));
console.log("exports " + exports.join(" "));
JS

# The `wasi.<name>` shims a glue file defines: the method names of its
# `const wasi = { ... };` object.
shims_of() { sed -n '/^const wasi = {$/,/^};$/p' "$1" | sed -n 's/^  \([a-z_][a-z_0-9]*\)(.*/\1/p' | sort | tr '\n' ' ' | sed 's/ $//'; }
imports_of() { node "$WORK/inspect.mjs" "$1" | sed -n 's/^imports //p'; }
exports_of() { node "$WORK/inspect.mjs" "$1" | sed -n 's/^exports //p'; }

CEILING="$FIXTURE_DIR/glue-ceiling.txt"
HAVE_WASM_OPT=0; command -v wasm-opt >/dev/null && HAVE_WASM_OPT=1
[ "$HAVE_WASM_OPT" -eq 1 ] || echo "::warning::js-host: wasm-opt not found — the --wasm-opt shim-set assertion is skipped"

cat > "$WORK/run.mjs" <<'JS'
import { pathToFileURL } from "node:url";
const [jsPath, hostPath] = process.argv.slice(2);
const mod = await import(pathToFileURL(jsPath).href);
const host = hostPath ? await import(pathToFileURL(hostPath).href) : {};
await mod.init(undefined, { js: host.js ?? {} });
mod.run();
if (host.after) await host.after(mod);
JS

fail=0; n=0
for f in "$FIXTURE_DIR"/*.almd; do
  [ -e "$f" ] || continue
  n=$((n + 1))
  stem="$(basename "$f" .almd)"
  dir="$(dirname "$f")"
  expected="$dir/$stem.expected"
  host="$dir/$stem.host.mjs"
  leg="$(sed -n 's|^// @leg: *||p' "$f" | head -1)"
  if [ ! -f "$expected" ]; then
    echo "FAIL $f: no $stem.expected next to the fixture"; fail=1; continue
  fi
  if ! "$BIN" build "$f" --target wasm --host js -o "$WORK/$stem.wasm" > "$WORK/$stem.build" 2>&1; then
    echo "FAIL $f: build"; sed 's/^/    /' "$WORK/$stem.build"; fail=1; continue
  fi
  if [ -n "$leg" ] && ! grep -q "$leg" "$WORK/$stem.build"; then
    echo "FAIL $f: expected the $leg leg, build said:"; sed 's/^/    /' "$WORK/$stem.build"; fail=1; continue
  fi
  for out in js d.ts; do
    [ -s "$WORK/$stem.$out" ] || { echo "FAIL $f: $stem.$out not written"; fail=1; }
  done
  hostarg=""; [ -f "$host" ] && hostarg="$host"
  if ! node "$WORK/run.mjs" "$WORK/$stem.js" $hostarg > "$WORK/$stem.out" 2> "$WORK/$stem.err"; then
    echo "FAIL $f: node run"; sed 's/^/    /' "$WORK/$stem.err" | head -40; fail=1; continue
  fi
  if ! cmp -s "$WORK/$stem.out" "$expected"; then
    echo "FAIL $f: stdout differs from $stem.expected"; diff "$expected" "$WORK/$stem.out" | head -20; fail=1; continue
  fi
  if ! grep -q '@extern(wasm' "$f"; then
    if ! "$BIN" run "$f" > "$WORK/$stem.native" 2> "$WORK/$stem.native.err"; then
      echo "FAIL $f: native run"; sed 's/^/    /' "$WORK/$stem.native.err" | head -20; fail=1; continue
    fi
    # The native run is `main` alone; the host may append lines from `after`.
    lines=$(wc -l < "$WORK/$stem.native" | tr -d ' ')
    if ! cmp -s "$WORK/$stem.native" <(head -n "$lines" "$expected"); then
      echo "FAIL $f: native stdout differs from the host's"; diff "$WORK/$stem.native" <(head -n "$lines" "$expected") | head -20; fail=1; continue
    fi
  fi
  # 5. the module ships only what the surface needs.
  if ! "$BIN" build "$f" --target wasm -o "$WORK/$stem.plain.wasm" > "$WORK/$stem.plain.build" 2>&1; then
    echo "FAIL $f: build without --host js"; sed 's/^/    /' "$WORK/$stem.plain.build"; fail=1; continue
  fi
  extra="$(comm -13 <(exports_of "$WORK/$stem.plain.wasm" | tr ' ' '\n') <(exports_of "$WORK/$stem.wasm" | tr ' ' '\n') | tr '\n' ' ' | sed 's/ $//')"
  if grep -q '^function allocString(' "$WORK/$stem.js"; then
    if [ "$extra" != "__alloc __release" ]; then
      echo "FAIL $f: the glue marshals a String, so the module must add exactly __alloc and __release; it adds: '$extra'"; fail=1; continue
    fi
  else
    if [ -n "$extra" ] || ! cmp -s "$WORK/$stem.plain.wasm" "$WORK/$stem.wasm"; then
      echo "FAIL $f: no String on the host surface, so --host js must leave the module byte-identical; extra exports: '$extra'"; fail=1; continue
    fi
  fi
  # 6. shim set == the shipped module's WASI imports, pre-opt and post-opt.
  if [ "$(shims_of "$WORK/$stem.js")" != "$(imports_of "$WORK/$stem.wasm")" ]; then
    echo "FAIL $f: glue shims [$(shims_of "$WORK/$stem.js")] != module imports [$(imports_of "$WORK/$stem.wasm")]"; fail=1; continue
  fi
  if [ "$HAVE_WASM_OPT" -eq 1 ]; then
    if ! "$BIN" build "$f" --target wasm --host js --wasm-opt -o "$WORK/${stem}_opt.wasm" > "$WORK/$stem.opt.build" 2>&1; then
      echo "FAIL $f: build with --wasm-opt"; sed 's/^/    /' "$WORK/$stem.opt.build"; fail=1; continue
    fi
    if [ "$(shims_of "$WORK/${stem}_opt.js")" != "$(imports_of "$WORK/${stem}_opt.wasm")" ]; then
      echo "FAIL $f: after --wasm-opt, glue shims [$(shims_of "$WORK/${stem}_opt.js")] != module imports [$(imports_of "$WORK/${stem}_opt.wasm")]"; fail=1; continue
    fi
  fi
  # 7. the glue byte ceiling.
  ceiling="$(awk -v s="$stem" '$1 == s { print $2 }' "$CEILING" 2>/dev/null)"
  size="$(wc -c < "$WORK/$stem.js" | tr -d ' ')"
  if [ -z "$ceiling" ]; then
    echo "FAIL $f: no row for $stem in $CEILING (add '$stem $size')"; fail=1; continue
  fi
  if [ "$size" -gt "$ceiling" ]; then
    echo "FAIL $f: glue is $size bytes, over its ceiling of $ceiling in $CEILING — raise the row in the same change if the growth is meant"; fail=1; continue
  fi
  echo "ok   $f (${leg:-any} leg; glue $size/$ceiling B; shims: $(shims_of "$WORK/$stem.js"))"
done

if [ "$n" -eq 0 ]; then echo "::error::js-host: no fixtures under $FIXTURE_DIR"; exit 1; fi
if [ "$fail" -ne 0 ]; then echo "js-host: FAILED"; exit 1; fi
echo "js-host OK: $n fixture(s) built with --host js, ran under node, matched their expected output, shipped only their surface"
