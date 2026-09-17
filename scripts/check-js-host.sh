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
#      binary — the host is one more leg of the cross-target contract.
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
  echo "ok   $f (${leg:-any} leg)"
done

if [ "$n" -eq 0 ]; then echo "::error::js-host: no fixtures under $FIXTURE_DIR"; exit 1; fi
if [ "$fail" -ne 0 ]; then echo "js-host: FAILED"; exit 1; fi
echo "js-host OK: $n fixture(s) built with --host js, ran under node, matched their expected output"
