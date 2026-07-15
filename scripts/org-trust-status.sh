#!/usr/bin/env bash
# org-trust-status.sh — sweep the github.com/almide org's Almide-written repos through the v1
# trust-spine lowering wall and regenerate docs/org-trust-status.md.
#
# For each repo it runs the MIR `classify_corpus` example over EVERY `src/*.almd` module (a barrel
# entry hides the real code in submodules; cross-module importers are skipped, surfaced as `+N xmod`)
# and aggregates:
#   - lowers   : functions in the v1 lowering subset (the proven-checker re-verifies each)
#   - walls    : functions explicitly walled (Unsupported) — honest, never a silent miscompile
#   - status   : ✅ wall=0  /  N walls
#   - top wall : the most frequent wall reason (the lever — one brick clears a whole class)
#
# ⚠ A wall count of 0 means "every function is INSIDE the lowering subset", NOT "byte-verified".
# The real ② gate is a v0==v1 byte-match (run the repo's own vectors/tests on native AND wasm).
# `lowers but not byte-verified` is exactly the trap that produced a fake sha1=0 before the
# `var v=w` aliasing miscompile was found by byte-matching the RFC 3174 vectors. Keep that order:
# wall=0 first, byte-match SECOND and AUTHORITATIVE.
#
# Usage:
#   scripts/org-trust-status.sh                 # sweep + rewrite docs/org-trust-status.md
#   ALMIDE_ORG_DIR=/path/to/almide scripts/org-trust-status.sh
#
# Env:
#   ALMIDE_ORG_DIR  the dir holding the sibling org repos (default: parent of the main almide repo)
set -euo pipefail

# Build + binary + output live in THIS working tree (a git worktree may differ from the main repo).
work_root="$(git rev-parse --show-toplevel)"
# The org dir holds the sibling target repos (yaml, sha1, …) next to the MAIN almide repo.
main_repo="$(cd "$(git rev-parse --git-common-dir)/.." && pwd)"
ORG_DIR="${ALMIDE_ORG_DIR:-$(dirname "$main_repo")}"
OUT="$work_root/docs/org-trust-status.md"
BIN="$work_root/target/debug/examples/classify_corpus"
RBIN="$work_root/target/debug/examples/render_program"

echo "building classify_corpus + render_program…" >&2
( cd "$work_root" && cargo build -q -p almide-mir --example classify_corpus --example render_program )

# The repos verified by a real v0==wasm byte-match (not just wall=0): the repo's OWN test suite
# passes in full on BOTH `almide test --target native` and `almide test --target wasm`.
# Verified 2026-07-02 after the wasm share/layout/value-semantics fixes (contracts C-121..C-125).
# Update as new ones are checked. Not verifiable: almide-web / almide-sqlite / wasm-webgl / obsid / audio-poc (no tests),
# almide-dojo (task-bank fixtures, not a compilable suite).
BYTE_VERIFIED=" yaml sha1 toml svg rsa porta csv bigint base64 aes lumen homullus nn almide-bindgen almide-wasm-bindgen almide-lander almide-grammar "

# Only `src/*.almd` (a real module root) is swept, never embedded shims (`stdlib/`) or benchmark
# fixtures (`research/`, `benchmark/`) — those misfired into bogus repo-level numbers (e.g.
# almide-fable-llm, a Rust project, once reported a random MSR benchmark file). A repo with no
# `src/*.almd` at all ⇒ not an Almide library.
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/o"                                          # classify_corpus --out needs an existing dir
rows=""; allwalls="$tmp/allwalls.txt"; : > "$allwalls"
total_lower=0; total_wall=0; total_wall_real=0; total_wall_native=0; clean=0; counted=0
res_clean=0; res_total_fail=0; res_total_ffail=0; res_counted=0

for d in "$ORG_DIR"/*/; do
  repo="$(basename "$d")"
  [ "$repo" = "almide" ] && continue                       # the compiler itself is the v0 corpus, not a target
  # Sweep EVERY src/ module (recursive), not just the entry — a barrel `mod.almd` re-exports
  # submodules that hold the real code (porta's variants etc.), so entry-only counting reported
  # a false 0/0. classify_corpus reads ONE file with no cross-module import resolution, so a
  # module that imports a SIBLING is `frontend-rejected` and SKIPPED (counted, surfaced as
  # `+N cross-mod skipped`); a leaf/stdlib-only module is measured. Honest under-count > false 0/0.
  files="$(find "${d%/}/src" -name '*.almd' 2>/dev/null | grep -vE '_test\.almd' | sort || true)"
  if [ -z "$files" ]; then
    rows="${rows}| \`$repo\` | — | — | — | ▫ not an Almide \`src/\` library | — |\n"
    continue
  fi
  rip=0; rwl=0; rwl_real=0; rwl_native=0; measured=0; skipped=0; : > "$tmp/repowalls"
  res_ok=0; res_fail=0; res_ffail=0; res_walls=0; res_reason=""
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    # RESOLVED-STRICT (headline metric): render the module through the REAL pipeline —
    # canonical sibling discovery + dep resolution, exactly what `almide build --target wasm`
    # runs. The metric is PER-FUNCTION WALLS under resolution: a module is clean when zero
    # functions wall. A mainless library dies with the "main is outside" ARTIFACT after every
    # function lowered — that is clean, not a wall. A frontend type-error rejection is counted
    # separately (a checker bug like #783, not a lowering wall).
    if "$RBIN" "$f" > /dev/null 2> "$tmp/rerr"; then
      res_ok=$((res_ok + 1))
    else
      wl_n="$(grep -oE '^\[render_program\] [0-9]+ of [0-9]+ function' "$tmp/rerr" | grep -oE '[0-9]+' | head -1 || true)"
      wl_n="${wl_n:-0}"
      if grep -q 'type errors' "$tmp/rerr"; then
        res_ffail=$((res_ffail + 1))
        [ -z "$res_reason" ] && res_reason="frontend: $(grep -oE 'type errors: .{0,44}' "$tmp/rerr" | head -1 || true)"
      elif [ "$wl_n" = "0" ] && grep -q 'main is outside the MIR-lowering subset' "$tmp/rerr" \
           && ! grep -qE '^(effect )?fn main\(' "$f"; then
        res_ok=$((res_ok + 1))                             # mainless library: every fn lowered
      else
        res_fail=$((res_fail + 1)); res_walls=$((res_walls + wl_n))
        if [ -z "$res_reason" ]; then
          res_reason="$(grep -E '^  ' "$tmp/rerr" | head -1 | sed -E 's/`[^`]*`/X/g; s/[0-9]+/N/g' | cut -c1-56 || true)"
          [ -z "$res_reason" ] && res_reason="$(tail -1 "$tmp/rerr" | sed -E 's/[0-9]+/N/g' | cut -c1-56 || true)"
        fi
      fi
    fi
    out="$(WALL_NAMES=1 "$BIN" --out "$tmp/o" "$f" 2>&1 || true)"
    ip="$(printf '%s' "$out" | grep -oE 'in-profile \(lowers\)[ ]*: [0-9]+' | grep -oE '[0-9]+$' || true)"
    wl="$(printf '%s' "$out" | grep -oE 'walled \(Unsupported\)[ ]*: [0-9]+' | grep -oE '[0-9]+$' || true)"
    # The honest split: REAL = the wall=0 metric (pure/WASI-able lowering gaps); NATIVE-FFI =
    # structural (@extern rust/rs + no-wasm stdlib effect), excluded exactly like @extern(wasm).
    wlr="$(printf '%s' "$out" | grep -oE 'walled real \(lowering\)[ ]*: [0-9]+' | grep -oE '[0-9]+$' || echo 0)"
    wln="$(printf '%s' "$out" | grep -oE 'walled native-FFI \(excl\)[ ]*: [0-9]+' | grep -oE '[0-9]+$' || echo 0)"
    rej="$(printf '%s' "$out" | grep -oE 'frontend-rejected[ ]*: [0-9]+' | grep -oE '[0-9]+$' || echo 0)"
    if [ -z "$ip" ] || [ "${rej:-0}" != "0" ]; then skipped=$((skipped + 1)); continue; fi
    measured=$((measured + 1)); rip=$((rip + ip)); rwl=$((rwl + wl))
    rwl_real=$((rwl_real + wlr)); rwl_native=$((rwl_native + wln))
    # Keep the WALL_NAMES output but PREFIX each normalized reason with its category tag
    # (NATIVE-FFI / REAL), so the cross-repo lever still groups while showing structural vs real.
    printf '%s' "$out" | grep 'WALLED' | sed -E 's#^WALLED ([A-Z-]+) [^:]*:: [^:]+ :: #\1 #' >> "$tmp/repowalls" || true
  done <<EOF
$files
EOF
  # Resolved-strict repo verdict (independent of the classify sweep's xmod skips).
  res_n=$((res_ok + res_fail + res_ffail))
  [ "$res_n" -gt 0 ] && res_counted=$((res_counted + 1))
  if [ "$res_fail" -eq 0 ] && [ "$res_ffail" -eq 0 ] && [ "$res_n" -gt 0 ]; then
    res_clean=$((res_clean + 1)); res_status="✅ ${res_ok}/${res_n}"
  elif [ "$res_fail" -eq 0 ] && [ "$res_n" -gt 0 ]; then
    res_total_ffail=$((res_total_ffail + res_ffail)); res_status="🟠 ${res_ok}/${res_n} — ${res_reason}"
  else
    res_total_fail=$((res_total_fail + res_fail)); res_total_ffail=$((res_total_ffail + res_ffail)); res_status="🔴 ${res_ok}/${res_n} — ${res_reason}"
  fi
  if [ "$measured" -eq 0 ]; then
    if [ "$skipped" -eq 0 ]; then note="no in-subset fns"; else note="all cross-module"; fi
    rows="${rows}| \`$repo\` | $res_status | 0 | 0 | 0 | ▫ $note | — |\n"
    continue
  fi
  counted=$((counted + 1))
  total_lower=$((total_lower + rip)); total_wall=$((total_wall + rwl))
  total_wall_real=$((total_wall_real + rwl_real)); total_wall_native=$((total_wall_native + rwl_native))
  cat "$tmp/repowalls" >> "$allwalls"
  top="$(sed -E 's/`[^`]*`/X/g; s/[0-9]+/N/g' "$tmp/repowalls" | sort | uniq -c | sort -rn | head -1 | sed -E 's/^ *[0-9]+ //' | cut -c1-56 || true)"
  skipnote=""; if [ "$skipped" -gt 0 ]; then skipnote=" +${skipped} xmod"; fi
  nfnote=""; if [ "$rwl_native" -gt 0 ]; then nfnote=", ${rwl_native} native-FFI excl"; fi
  # wall=0 is the REAL count (structural native-FFI walls are excluded, like @extern(wasm) WASI).
  if [ "${rwl_real}" = "0" ]; then
    clean=$((clean + 1))
    case "$BYTE_VERIFIED" in *" $repo "*) status="✅ 0 — byte-verified${skipnote}${nfnote}";; *) status="🟡 0 — lowers, byte-match TODO${skipnote}${nfnote}";; esac
    top="—"
  else
    status="🔴 ${rwl_real}${skipnote}${nfnote}"
  fi
  rows="${rows}| \`$repo\` | $res_status | $rip | $rwl_real | $rwl_native | $status | $top |\n"
done

agg="$(sed -E 's/`[^`]*`/X/g; s/[0-9]+/N/g' "$allwalls" | sort | uniq -c | sort -rn | head -12 \
        | sed -E 's/^ *([0-9]+) (.*)$/| \1 | \2 |/' | cut -c1-100)"

{
  echo "# Almide org — v1 trust-spine status"
  echo
  echo "> Auto-generated by \`scripts/org-trust-status.sh\`. Re-run to refresh. Org dir: \`$ORG_DIR\`."
  echo
  echo "**${res_clean}/${res_counted} repos fully render under the RESOLVED v1 pipeline** (every \`src/\` module lowers wall-free under canonical sibling + dep resolution — the same path \`almide build --target wasm\` runs; ${res_total_fail} modules still wall, ${res_total_ffail} frontend-rejected). Secondary per-function classify sweep: ${clean}/${counted} repos at wall=0, real walls = ${total_wall_real} / native-FFI = ${total_wall_native} (excluded), ${total_lower} functions lowering."
  echo
  echo "> **The headline is the resolved metric.** classify_corpus reads one file with NO cross-module"
  echo "> resolution, so its per-file numbers count artifacts (\`lay.*\`/\`v.*\` refs) the real pipeline"
  echo "> resolves — keep it for the per-function wall-reason lever, not the verdict."
  echo
  echo "> **wall=0 metric = REAL lowering walls only.** A NATIVE-FFI wall is a function that"
  echo "> TRANSITIVELY calls an \`@extern(rust/rs)\` declaration (no wasm form) or a permanently-no-wasm"
  echo "> stdlib effect (\`process.exec/exit/run\`, \`http.request\`, \`net.*\`). Those can NEVER lower to"
  echo "> wasm — they need a native host — so they are EXCLUDED exactly like the \`@extern(wasm)\` WASI"
  echo "> imports already are. They are NOT lowering bugs. Only REAL walls drive the wall=0 goal."
  echo
  echo "⚠ **wall=0 ≠ correct.** It means every function is inside the v1 lowering subset, NOT that the"
  echo "output is byte-identical to v0. The authoritative ② gate is a **v0==v1 byte-match** (run the repo's"
  echo "own vectors/tests on native AND \`--target wasm\`). \`🟡 lowers, byte-match TODO\` flags repos that"
  echo "reached wall=0 but have not yet been byte-verified — exactly where a silent miscompile can hide"
  echo "(e.g. the \`var v=w\` scalar-aliasing bug that faked an early sha1=0 until the RFC vectors caught it)."
  echo
  echo "## Per-repo (sorted by walls)"
  echo
  echo "| repo | resolved (renders) | lowers | real walls | native-FFI | classify status | top wall reason |"
  echo "|------|--------------------|-------:|-----------:|-----------:|-----------------|-----------------|"
  printf '%b' "$rows" | sort -t'|' -k5 -rn
  echo
  echo "## Cross-repo wall lever (most frequent reasons)"
  echo
  echo "One brick that clears a top reason advances multiple repos at once."
  echo
  echo "| count | wall reason (normalized) |"
  echo "|------:|--------------------------|"
  echo "$agg"
  echo
  echo "## Notes"
  echo
  echo "- **Every** \`src/*.almd\` module is swept (not just the entry). The **resolved column** renders each module through the real pipeline (sibling + dep resolution), so cross-module importers ARE measured there. classify_corpus reads one file with no cross-module import resolution, so its sweep skips sibling importers — surfaced per repo as \`+N xmod\`; keep it for per-function wall reasons only."
  echo "- The \`almide\` repo itself is the v0 corpus (its own \`proofs/corpus-wall.sh\` gate), not a target here."
  echo "- \`porta\` is a NATIVE HOST (\`almide.toml\`: wasmtime + reqwest/Net) — the full MCP server is native-only by design (WASI preview1 has no net and can't embed wasmtime), so the 25 native-FFI are its host calls and only its PORTABLE protocol layer (jsonrpc/config) is in the v1 subset. Its byte-verification = the full test suite on both targets (7/7 wasm-runnable files + 1 FFI file native-only by design), plus the native (v0) build of the full MCP server, both green."
  echo "- ✅ **byte-verified** = the repo's OWN test suite passes IN FULL on both \`almide test --target native\` and \`--target wasm\` (the assertions are the vectors). The 2026-07-02 sweep took this from 2 repos (yaml, sha1) to every repo WITH a test suite, after fixing six wasm bug classes the sweep itself flushed out (share/+1 on pass-through and copied-pair paths, list-layout under-allocation, missing Value deep-eq, value.merge oracle mismatch, bytes.set value semantics — contracts C-121..C-125; see docs/roadmap/active/v1-org-byte-verification.md)."
  echo "- Repos that CANNOT be byte-verified yet: \`almide-web\` / \`almide-sqlite\` (no test vectors — write suites first), \`almide-dojo\` (task-bank fixtures, not a compilable suite)."
  echo "- To byte-verify a repo: run its own tests on BOTH targets (\`almide test --target native\` and"
  echo "  \`--target wasm\`), require a full pass on each, then add it to \`BYTE_VERIFIED\` in the script."
  echo
  echo "## Graphics / AI stack — production-target spot-check (2026-07-02)"
  echo
  echo "The graphics and AI repos are mostly BROWSER-HOSTED wasm apps (a JS host loads the module),"
  echo "so \"runs on v1\" for them = the current compiler produces a valid wasm artifact; headless"
  echo "byte-run comparison does not apply to a rendering host. Verified states:"
  echo
  echo "| repo | kind | state |"
  echo "|------|------|-------|"
  echo "| \`svg\` | graphics (pure lib) | ✅ byte-verified (suite, both targets); the cross-module \`render(group(..))\` compiler stack overflow is fixed (2026-07-03 equi-recursive unify guard) |"
  echo "| \`lumen\` | graphics (pure lib) | ✅ byte-verified (suite, both targets) |"
  echo "| \`canvas\` | browser-hosted app | ✅ wasm builds clean |"
  echo "| \`wasm-canvas\` | browser-hosted app | ✅ wasm builds clean |"
  echo "| \`wasm-webgl\` | browser-hosted app | ✅ wasm builds clean |"
  echo "| \`obsid\` | browser+native-hosted app | ✅ wasm builds clean (no test suite yet) |"
  echo "| \`almide-aituber\` | browser-hosted app | ✅ wasm builds clean (the v1 divergence was the missing develop-side #717 auto-?-branch-retype fix — cherry-picked 2026-07-02) |"
  echo "| \`homullus\` | AI agent | ✅ byte-verified (suite, both targets) |"
  echo "| \`almide-sqlite\` | native host lib | ✅ suite green (10 tests, :memory: + file persistence) — native host package (rusqlite FFI), verified 2026-07-03 |"
  echo "| \`almide-web\` | browser bindings | ✅ headless vectors green (17-line byte-match: full import surface + intern protocol + callbacks under runtime/headless.mjs) — verified 2026-07-03 |"
  echo "| \`nn\` | AI (neural nets) | ✅ byte-verified (suite: 13/13 files, both targets) — unblocked 2026-07-02 by the TCO-temp type fix, the nested-HOF lambda-pin fix (C-126), the Matrix repr, the SIMD fast-exp clamp, and the unwrap_or chain fix (C-127) |"
  echo "| \`almai\` | AI (LLM client lib) | ✅ native suite green (56 tests, 2 files) — resolved 2026-07-03 by the STRUCTURAL-TWIN merge: the checker unifies same-base-name same-shape record decls across modules, so codegen now merges them into one canonical struct (flatten twin-merge; the E0063 default-fill and bare-ref repair ride the same mechanism). wasm leg: native fallback (\`env.get\`/http are native-only — an LLM client needs the network host) |"

  echo
  echo "Every graphics/AI repo above now builds or verifies on v1."
} > "$OUT"

echo "wrote $OUT (resolved: ${res_clean}/${res_counted} fully render; classify: ${clean}/${counted} at wall=0)" >&2
