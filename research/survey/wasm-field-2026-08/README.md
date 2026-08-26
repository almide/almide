# wasm field expansion measurement — reproduction guide (2026-08)

Same-harness measurement of Almide's greenfield wasm lane against 2026's
active wasm-targeting languages. Results: [LEDGER.md](./LEDGER.md).
Raw generated tables: `out/results.md` / `out/results.json` (not committed;
produced by `./measure.sh`).

## What is measured

7 kernels (ported from `crates/almide-wasm/tests/perf/*.almd`, workload
constants and operation counts preserved exactly) x 8 lanes x 5 metrics:

1. run time — stock `wasmtime` CLI, 1 warmup + 5 timed runs, best AND median,
   minus the same lane's empty-program baseline under the same runner config
2. compile time — CLI end-to-end with warm caches; the leaf source file is
   `touch`ed before every rep so no lane gets a no-op cache hit; 1 warmup + 5 reps
3. standalone `.wasm` size in bytes
4. peak RSS of the run (`/usr/bin/time -l`, "maximum resident set size")
5. portability — runs on the stock wasmtime CLI with default flags: yes/no

Output correctness is asserted on every timed run (integer kernels compare the
byte output; float_math compares as parsed f64 — decimal formatting differs
across languages, see the ledger).

## Machine / pinned versions of record

See the "Toolchain versions" section of LEDGER.md (recorded from the actual
measurement run). Almide is the greenfield lane at SHA `46e689518`
(branch `survey/wasm-field-2026-08` base). Apple Silicon macOS, no other load.

## Toolchain install (clean machine)

Everything below /opt/homebrew comes from Homebrew; `~/.moon` from MoonBit's
official installer.

```bash
# runner + binaryen
brew install wasmtime binaryen

# Rust: any rustc >= 1.94 works; the wasm32-wasip1 std component is fetched
# and overlaid automatically by setup.sh (works without rustup — the local
# toolchain here is qusp-managed, so `rustup target add` is not available)

# Go mainline (wasip1 needs >= 1.21; measured with the system go)
# TinyGo — requires a Go <= 1.26 toolchain and wasm-opt:
brew install tinygo-org/tools/tinygo go@1.26
# (if /opt/homebrew/bin/wasm-opt goes missing after tap installs: brew link --overwrite binaryen)

# MoonBit (official installer -> ~/.moon/bin)
curl -fsSL https://cli.moonbitlang.com/install/unix.sh | bash

# Grain 0.7.2 — the official macOS distribution is an UNSIGNED x86_64 binary:
# macOS SIGKILLs (and Gatekeeper may even delete) it out of the cask. Download
# the release binary, ad-hoc sign it, run under Rosetta 2:
curl -sL -o ~/.local/bin/grain \
  https://github.com/grain-lang/grain/releases/download/grain-v0.7.2/grain-mac-x64
chmod +x ~/.local/bin/grain && codesign -s - ~/.local/bin/grain
softwareupdate --install-rosetta --agree-to-license   # if Rosetta not present

# AssemblyScript: local npm install, done by setup.sh (needs node+npm)

# Kotlin/Wasm: gradle + a JDK; the Kotlin 2.3.21 multiplatform plugin and its
# Node distribution are downloaded by gradle on first build
brew install gradle
```

## Build + measure

```bash
./setup.sh     # builds every lane's .wasm once (first run: large downloads)
./measure.sh   # full matrix -> out/results.json + out/results.md
./measure.sh grain kotlin   # re-measure only the named lanes (merges into results.json)
```

`setup.sh` also builds `tools/emit-only/` — a thin driver over the exact
`almide-spine`/`almide-wasm` pipeline that stops after writing the `.wasm`.
It exists because the product runner (`almide-wasm-run --emit-wasi`) always
executes the module after emitting, which would fold run time into the
compile-time metric. Its output is byte-identical to `--emit-wasi` (verified
with `cmp` during the survey).

## Lane notes / porting doctrine

- Integer width: every port uses 64-bit integers (Rust `i64`, Go/TinyGo
  `int64`, AS `i64`, Kotlin `Long`, MoonBit `Int64`) to match Almide's `Int`.
  Exception: Grain uses its default `Number` (tagged 63-bit fixnum) because
  Grain's `Int64` needs non-idiomatic explicit operator imports; all workload
  values fit both representations exactly.
- Go and TinyGo compile byte-identical sources (`src/go/` vs `src/tinygo/`,
  the latter differing only by a header comment).
- Sort kernels copy the unsorted input each round (Almide's `list.sort`
  returns a fresh sorted list), then use each language's stdlib sort.
  `sort_by`'s key `(x) => 0 - x` maps to `sort_by_key`/`sortedBy` where the
  language has one (Rust, MoonBit, Kotlin) and to the equivalent descending
  comparator where it only has comparator sorts (Go, AS, Grain).
- list_pipeline materializes each stage (map -> new collection, filter -> new
  collection, fold) in every lane — no lazy iterator fusion — matching the
  Almide source's semantics.
- Kotlin recursion uses `tailrec` (the language's dedicated construct for
  exactly this shape). Go (mainline) and AssemblyScript have no tail-call
  elimination: their recursion kernels exhaust the stock wasmtime call stack
  (recorded as-is) and carry a reference measurement under
  `wasmtime -W max-wasm-stack=1073741824`.
- Grain builds fixed-size arrays with `Array.init` (its arrays are
  fixed-length; the per-element computations are identical).

## Layout

```
LEDGER.md          results ledger (the deliverable)
README.md          this file
setup.sh           one-time build of all lanes
measure.sh/.py     the measurement driver (re-runnable)
src/<lane>/        ported kernel sources + empty baselines
tools/emit-only/   Almide compile-time driver (emit without execute)
out/               build artifacts + results (gitignored)
```
