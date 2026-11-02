# Almide translation consumer audit

Snapshot: 9a95b28519b90ae535dc9918bfed04805dc4675c. Read-only repository inspection, 2026-11-02. 80 originally “Handwritten documentation — consumer audit pending” rows. No repository edits made by this audit.

## Method and limits

- Word count means whitespace-separated tokens; not a reliable Japanese linguistic word count. Character count included. Language is a character-assisted provisional label unless prose review is stated; English code tokens do not make a Japanese source English.
- Exact full-path references searched across tests/, src/, crates/, scripts/, build.rs (all non-Markdown file types). Generic Markdown enumeration was additionally inspected; known enumerators target stdlib/diagnostics/specs/als rather than design/project/wasm. This is not proof that no indirect or external consumer exists.
- No-reference results are not automatically safe. “Candidate” means a reader translation is plausible after the stated inspection, not certification of all build consumers. Unreviewed bodies remain Pending. No CI changes proposed or made.
- Current scripts/ci_prose.py:29–65 recognizes fixed root/top-level names and design/project directories; new top-level localized siblings, mcp, wasm are outside its narrow lane. That may run full CI. Do not change the classifier in this task.
- Parent has separate translations of SECURITY.md, docs/mcp.md and docs/design/EQUIVALENCE.md underway; this audit does not duplicate their work.

## Significant reclassifications

STABILITY is normative, org-trust-status is legacy generated evidence, fuzz-green README is a parsed evidence ledger. The grammar-lab and benchmark prompt files below are experiment inputs, not documentation-only prose. ARCHITECTURE/CHEATSHEET/WASM-OUTPUT/BENCHMARKS have exact-path checks that do not cover newly added siblings.

## Per-file routing

### `SECURITY.md`
- Language: English (provisional); size: 391 whitespace tokens / 2875 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - `scripts/ci_prose.py:30:             "SUPPORT.md", "SECURITY.md"}`
  - `scripts/test_ci_prose.py:103:        for path in ["README.md", "CLAUDE.md", "SUPPORT.md", "SECURITY.md", "docs/SPEC.md",`

### `crates/README.md`
- Language: English (provisional); size: 547 whitespace tokens / 4398 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `crates/almide-egg-lab/README.md`
- Language: English (provisional); size: 795 whitespace tokens / 5699 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `crates/almide-kernel/README.md`
- Language: English (provisional); size: 1874 whitespace tokens / 12898 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `crates/almide-perceus-belt/README.md`
- Language: English (provisional); size: 2 whitespace tokens / 21 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `crates/almide-wasm-vm/REQUIREMENTS.md`
- Language: English (provisional); size: 1373 whitespace tokens / 8657 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `demo/make-verify/README.md`
- Language: English (provisional); size: 846 whitespace tokens / 5481 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/ARCHITECTURE.md`
- Language: English (provisional); size: 5792 whitespace tokens / 42437 characters.
- Status: **Machine-read prose — controlled translation**.
- Reason: scripts/check-pass-roster.sh:29 parses exact original roster. Siblings are not automatically checked. Preserve original headings/roster and gate coverage; translation not a second canonical roster.
- Direct references:
  - `scripts/check-pass-roster.sh:4:# docs/ARCHITECTURE.md carries the per-target optimization pass roster — which`
  - `scripts/check-pass-roster.sh:29:DOC="$ROOT/docs/ARCHITECTURE.md"`
  - `scripts/check-pass-roster.sh:45:  echo "PASS ROSTER FAIL — section \"$HEADING\" not found in docs/ARCHITECTURE.md" >&2`
  - `scripts/check-pass-roster.sh:113:  echo "PASS ROSTER FAIL — a pass exists in the code that docs/ARCHITECTURE.md's roster does not name (#929). Add its row to the section \"$HEADING\"." >&2`

### `docs/CHEATSHEET.md`
- Language: English with Japanese fragments (provisional); size: 11119 whitespace tokens / 70604 characters.
- Status: **LLM surface — validation coverage needed**.
- Reason: scripts/check-llm-surface.sh:34 selects exact original; translated checked fences would not automatically be exercised. Source-pinned companion with explicit coverage required.
- Direct references:
  - `scripts/check-perf-ratio.sh:156:# the mission depends on: CLAUDE.md and docs/CHEATSHEET.md tell authors (and`
  - `scripts/check-perf-ratio.sh:409:          "CLAUDE.md and docs/CHEATSHEET.md tell authors to write \`list.range |> list.flat_map\` "`
  - `scripts/check-llm-surface.sh:6:# \`almide check\`. The LLM surface (llms.txt, docs/CHEATSHEET.md) is the`
  - `scripts/check-llm-surface.sh:34:FILES=("$ROOT/llms.txt" "$ROOT/docs/CHEATSHEET.md")`
  - `tests/selective_import_ctor_let_test.rs:8://! docs/CHEATSHEET.md documents \`import self.classifier.{classify, NUMBERS}\`.`
  - `crates/almide-codegen/src/pass_rust_lowering.rs:214:// idiom CLAUDE.md and docs/CHEATSHEET.md recommend, and it lowered to`
  - `crates/almide-frontend/src/check/calls.rs:1298:                    (format!("No function '{}' in module '{}'. See docs/CHEATSHEET.md for available functions", func, module), None, None)`
  - `tests/diagnostics/e002-module-fn-no-candidate/meta.toml:3:hint_substring = "See docs/CHEATSHEET.md"`

### `docs/STABILITY.md`
- Language: English (provisional); size: 654 whitespace tokens / 4423 characters.
- Status: **Normative — special authority handling**.
- Reason: Body explicitly says this page is normative. Preserve source authority; label translations non-normative/source-pinned, do not infer new guarantees.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/bugfix-research-2026-09-09.md`
- Language: English (provisional); size: 807 whitespace tokens / 6186 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/bugfix-research-2026-09-12.md`
- Language: English (provisional); size: 6789 whitespace tokens / 44643 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/cross-target-decisions.md`
- Language: English (provisional); size: 936 whitespace tokens / 6102 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/design/DESIGN.md`
- Language: English (provisional); size: 1810 whitespace tokens / 11754 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/design/EQUIVALENCE.md`
- Language: English (provisional); size: 576 whitespace tokens / 4591 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/design/HIDDEN_OPERATIONS.md`
- Language: Japanese or mixed (provisional); size: 473 whitespace tokens / 3677 characters.
- Status: **Adjacent translation candidate — limited audit**.
- Reason: Japanese prose reviewed; no exact consumer found in requested roots. Use .en.md / .zh-CN.md companions without relabeling Japanese original. Preserve code comments unchanged.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/design/MEMORY-SAFETY.md`
- Language: English (provisional); size: 2626 whitespace tokens / 18878 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/design/REJECTED_PATTERNS.md`
- Language: Japanese or mixed (provisional); size: 588 whitespace tokens / 4892 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/design/fan-cross-language.md`
- Language: English (provisional); size: 1927 whitespace tokens / 13068 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/design/fan-semantics-audit.md`
- Language: English (provisional); size: 3499 whitespace tokens / 23280 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - `tests/fan_acceptance_test.rs:1://! Fan acceptance tests (docs/design/fan-semantics-audit.md): pin what the`
  - `tests/fan_acceptance_test.rs:301:// ---- known divergences (docs/design/fan-semantics-audit.md §Discrepancies) ----`
  - `crates/almide-codegen/src/generated/rust_runtime.rs:10:    ("fan", "// Fan concurrency runtime functions\n\n// \`fan.map\` runs SEQUENTIALLY over an \`Rc<dyn Fn>\` thunk (the uniform closure\n// repr). This is what lets a closure VALUE bound to a var reach \`fan.map\`: that\n// va [... omitted end of long line]`
  - `crates/almide-mir/src/lower/desugar_fan.rs:8:/// least-(spend, index) fold (docs/design/fan-semantics-audit.md §5).`

### `docs/design/fan-usability-eval.md`
- Language: English (provisional); size: 911 whitespace tokens / 5453 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/fan-capture-ownership-review.md`
- Language: English (provisional); size: 332 whitespace tokens / 2460 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/fan-parallel-safety-review.md`
- Language: English (provisional); size: 496 whitespace tokens / 3756 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/grammar/README.md`
- Language: English (provisional); size: 1425 whitespace tokens / 9272 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - `scripts/check-lark-grammar.sh:30:# docs/grammar/README.md for the enumerated gap list.`

### `docs/mcp.md`
- Language: English (provisional); size: 1126 whitespace tokens / 7534 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/native-projection-ownership-review.md`
- Language: English (provisional); size: 474 whitespace tokens / 3450 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/native-read-borrow-review.md`
- Language: English (provisional); size: 472 whitespace tokens / 3282 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/project/BENCHMARKS.md`
- Language: English (provisional); size: 4640 whitespace tokens / 29342 characters.
- Status: **Measurement evidence — companion / coverage review**.
- Reason: scripts/check-readme-numbers.sh:75 scans exact original for fossils. Historical dated measurements and comparison qualifiers must stay intact; sibling would not receive this check.
- Direct references:
  - `scripts/check-readme-numbers.sh:75:for f in "${README_NUMBER_PAGES[@]}" docs/wasm/README.md docs/project/BENCHMARKS.md; do`
  - `scripts/gen-readme-stats.sh:277:    'Measured '"${vic_date}"' with '"${vic_binary}"' (\`'"${vic_version}"'\`) on x86 = '"${vic_x86}"' and arm = '"${vic_arm}"'; the reference: '"${vic_rustc}"'. Two ratios per cell are the two input sizes (the size is in e [... omitted end of long line]`

### `docs/project/BREAKING_CHANGE_POLICY.md`
- Language: Japanese or mixed (provisional); size: 232 whitespace tokens / 1780 characters.
- Status: **Adjacent translation candidate — limited audit**.
- Reason: Japanese source read in full. Future post-1.0/edition proposal explicitly separated from existing implementation. Keep .md Japanese identity; .en.md and .zh-CN.md siblings, or explicit mapping.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/project/GENERATED-RUST-REVIEW.md`
- Language: English (provisional); size: 382 whitespace tokens / 2813 characters.
- Status: **Adjacent translation candidate — limited audit**.
- Reason: Full prose reviewed. No direct machine reader found; code and trace-map conditions must remain exact. Qualification model explicitly not current certification.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/project/ISSUE-TAXONOMY.md`
- Language: English with Japanese fragments (provisional); size: 1285 whitespace tokens / 7704 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - `scripts/count-release-blockers.sh:4:# docs/project/ISSUE-TAXONOMY.md:`

### `docs/project/PARTITIONED-RUNTIME.md`
- Language: English (provisional); size: 499 whitespace tokens / 3753 characters.
- Status: **Adjacent translation candidate — limited audit**.
- Reason: Full prose reviewed; tests/static_memory_test.rs:19 is documentation comment, not input. Preserve PID-reference scope, per-deployment host assumptions.
- Direct references:
  - `tests/static_memory_test.rs:19://! docs/project/PARTITIONED-RUNTIME.md.`

### `docs/project/RUST-LINT-GATES.md`
- Language: English (provisional); size: 984 whitespace tokens / 6964 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - `scripts/check-clippy-warnings.sh:35:# and its adoption plan are recorded in docs/project/RUST-LINT-GATES.md.`

### `docs/project/WCET-STORY.md`
- Language: English (provisional); size: 419 whitespace tokens / 2993 characters.
- Status: **Adjacent translation candidate — limited audit**.
- Reason: Full prose reviewed; tests/wcet_kernel_test.rs:4 is documentation comment. Explicitly no hardware WCET result claimed; keep this boundary.
- Direct references:
  - `tests/wcet_kernel_test.rs:4://! fns' bodies. The story is docs/project/WCET-STORY.md; the kernel is`

### `docs/project/linguist-submission.md`
- Language: Japanese or mixed (provisional); size: 787 whitespace tokens / 6955 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/project/org-trust-status.md`
- Language: English (provisional); size: 2158 whitespace tokens / 11982 characters.
- Status: **Historical generated evidence — separate companion**.
- Reason: scripts/org-trust-status.sh:37 overwrites original. Header records legacy generator and prohibits unmeasured refreshed verdicts. Do not clone active generation markers or present as current status.
- Direct references:
  - `scripts/org-trust-status.sh:3:# trust-spine lowering wall and regenerate docs/project/org-trust-status.md.`
  - `scripts/org-trust-status.sh:20:#   scripts/org-trust-status.sh                 # sweep + rewrite docs/project/org-trust-status.md`
  - `scripts/org-trust-status.sh:37:OUT="$work_root/docs/project/org-trust-status.md"`

### `docs/record-option-cells-verification.md`
- Language: English (provisional); size: 258 whitespace tokens / 1801 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/stream-fusion-safety-review.md`
- Language: English (provisional); size: 501 whitespace tokens / 3620 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/wasm-decoder-reclaim-review.md`
- Language: English (provisional); size: 351 whitespace tokens / 2516 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `docs/wasm/JS-HOST-ASYNC-IMPORTS.md`
- Language: English with Japanese fragments (provisional); size: 1935 whitespace tokens / 12213 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - `src/cli/js_host_fan.rs:20://! Design: docs/wasm/JS-HOST-ASYNC-IMPORTS.md, "Fan overlap".`
  - `src/cli/js_host_async.js:5:// after another (docs/wasm/JS-HOST-ASYNC-IMPORTS.md).`
  - `src/cli/js_host_async.rs:14://! one sync that can suspend. Design: docs/wasm/JS-HOST-ASYNC-IMPORTS.md.`

### `docs/wasm/README.md`
- Language: English (provisional); size: 1154 whitespace tokens / 8153 characters.
- Status: **Machine-checked host guide — controlled translation**.
- Reason: scripts/check-readme-numbers.sh:75 scans exact original; sibling not checked. Keep component/core limits and host distinctions; not blocked structurally, but validation coverage needs explicit review.
- Direct references:
  - `scripts/check-readme-numbers.sh:75:for f in "${README_NUMBER_PAGES[@]}" docs/wasm/README.md docs/project/BENCHMARKS.md; do`

### `docs/wasm/WASM-OUTPUT.md`
- Language: English (provisional); size: 3796 whitespace tokens / 24399 characters.
- Status: **Machine-checked evidence — controlled translation**.
- Reason: src/cli/docs_gen.rs:318 exact-path count validation. Header has historic retired-backend measurements. Do not imply sibling has automatic count validation.
- Direct references:
  - `src/cli_args.rs:210:        /// rendered (see docs/wasm/WASM-OUTPUT.md). Default off — without this`
  - `src/cli/build_wasm.rs:292:/// pipeline (docs/wasm/WASM-OUTPUT.md). \`wasm-opt\` is a different kind of`
  - `src/cli/docs_gen.rs:318:        "docs/wasm/WASM-OUTPUT.md",`

### `docs/wasm/capability-system.md`
- Language: English (provisional); size: 1299 whitespace tokens / 9642 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Priority opening/role reviewed, but full body and indirect consumers still pending; retain source identity and evidence qualifiers.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `examples/almd-outline/README.md`
- Language: English (provisional); size: 446 whitespace tokens / 3195 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/exercises/BENCHMARK.md`
- Language: English (provisional); size: 904 whitespace tokens / 5521 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/fuzz-green/README.md`
- Language: English (provisional); size: 163 whitespace tokens / 1064 characters.
- Status: **Machine-read evidence ledger — separate companion**.
- Reason: scripts/lib/ledger-counts.sh:69 parses dated rows; scripts/fuzz-green-streak.sh refreshes ledger. Not ordinary reader prose.
- Direct references:
  - `scripts/lib/ledger-counts.sh:69:  streak=$(grep -E '^\| [0-9]{4}-' research/benchmark/fuzz-green/README.md | tail -1 | awk -F'|' '{gsub(/ /,"",$3); print $3}')`
  - `scripts/fuzz-green-streak.sh:43:# research/benchmark/fuzz-green/README.md is refreshed (BENCHMARKS.md`

### `research/benchmark/lang-bench/README.md`
- Language: English (provisional); size: 1107 whitespace tokens / 7746 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/lang-bench/tasks-example/xlang/README.md`
- Language: English (provisional); size: 70 whitespace tokens / 473 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/01-add-sub/prompt.md`
- Language: English (provisional); size: 56 whitespace tokens / 306 characters.
- Status: **Experiment input — separate explanatory translation**.
- Reason: research/grammar-lab/run-experiment.sh:31,103–104 and src/mod.almd:92–96 load prompt filenames; lang-bench/runner_edits.rb:94 loads prompt.md. Preserve experimental stimulus identity; do not replace or feed translations into original benchmark.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/02-rename-show-to-print/prompt.md`
- Language: English (provisional); size: 49 whitespace tokens / 300 characters.
- Status: **Experiment input — separate explanatory translation**.
- Reason: research/grammar-lab/run-experiment.sh:31,103–104 and src/mod.almd:92–96 load prompt filenames; lang-bench/runner_edits.rb:94 loads prompt.md. Preserve experimental stimulus identity; do not replace or feed translations into original benchmark.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/perf/README.md`
- Language: English (provisional); size: 2885 whitespace tokens / 18852 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/perf/hot-issues-2026-09-10/README.md`
- Language: English (provisional); size: 837 whitespace tokens / 6035 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/perf/hot-issues-2026-09-10/gramide-cached-operators/README.md`
- Language: English (provisional); size: 579 whitespace tokens / 4396 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/perf/spectralnorm/README.md`
- Language: English (provisional); size: 1766 whitespace tokens / 10897 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - `tests/counted_index_chain_test.rs:31://!   (research/benchmark/perf/spectralnorm/README.md).`

### `research/benchmark/perf/string-gap-1004.md`
- Language: English (provisional); size: 2028 whitespace tokens / 12837 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - `scripts/perf-ratio-baseline.txt:39:# research/benchmark/perf/string-gap-1004.md.`
  - `scripts/check-perf-ratio.sh:82:# turn out to be anchorable. See research/benchmark/perf/string-gap-1004.md.`

### `research/benchmark/stdlib/HANDOFF.md`
- Language: Japanese or mixed (provisional); size: 352 whitespace tokens / 3475 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/benchmark/three-way/README.md`
- Language: English (provisional); size: 343 whitespace tokens / 1655 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - `scripts/check-abstain-classes.sh:11:# research/benchmark/three-way/README.md.`

### `research/beyond-rust/README.md`
- Language: Japanese or mixed (provisional); size: 1402 whitespace tokens / 10096 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/fan-audit/probes/README.md`
- Language: English (provisional); size: 109 whitespace tokens / 887 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/LESSONS.md`
- Language: Japanese or mixed (provisional); size: 982 whitespace tokens / 7148 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/README.md`
- Language: English (provisional); size: 463 whitespace tokens / 3575 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/REPORT.md`
- Language: Japanese or mixed (provisional); size: 639 whitespace tokens / 3811 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/experiments/optional-handling/REPORT.md`
- Language: English (provisional); size: 596 whitespace tokens / 3840 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/prompts/layer1_rules.md`
- Language: English (provisional); size: 175 whitespace tokens / 888 characters.
- Status: **Experiment input — separate explanatory translation**.
- Reason: research/grammar-lab/run-experiment.sh:31,103–104 and src/mod.almd:92–96 load prompt filenames; lang-bench/runner_edits.rb:94 loads prompt.md. Preserve experimental stimulus identity; do not replace or feed translations into original benchmark.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/prompts/layer2_combinator.md`
- Language: English (provisional); size: 99 whitespace tokens / 783 characters.
- Status: **Experiment input — separate explanatory translation**.
- Reason: research/grammar-lab/run-experiment.sh:31,103–104 and src/mod.almd:92–96 load prompt filenames; lang-bench/runner_edits.rb:94 loads prompt.md. Preserve experimental stimulus identity; do not replace or feed translations into original benchmark.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/prompts/layer2_laws.md`
- Language: English (provisional); size: 76 whitespace tokens / 467 characters.
- Status: **Experiment input — separate explanatory translation**.
- Reason: research/grammar-lab/run-experiment.sh:31,103–104 and src/mod.almd:92–96 load prompt filenames; lang-bench/runner_edits.rb:94 loads prompt.md. Preserve experimental stimulus identity; do not replace or feed translations into original benchmark.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/prompts/layer2_match.md`
- Language: English (provisional); size: 108 whitespace tokens / 731 characters.
- Status: **Experiment input — separate explanatory translation**.
- Reason: research/grammar-lab/run-experiment.sh:31,103–104 and src/mod.almd:92–96 load prompt filenames; lang-bench/runner_edits.rb:94 loads prompt.md. Preserve experimental stimulus identity; do not replace or feed translations into original benchmark.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/prompts/layer2_variant_fn.md`
- Language: English (provisional); size: 72 whitespace tokens / 511 characters.
- Status: **Experiment input — separate explanatory translation**.
- Reason: research/grammar-lab/run-experiment.sh:31,103–104 and src/mod.almd:92–96 load prompt filenames; lang-bench/runner_edits.rb:94 loads prompt.md. Preserve experimental stimulus identity; do not replace or feed translations into original benchmark.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/grammar-lab/prompts/layer2_variant_paren.md`
- Language: English (provisional); size: 72 whitespace tokens / 507 characters.
- Status: **Experiment input — separate explanatory translation**.
- Reason: research/grammar-lab/run-experiment.sh:31,103–104 and src/mod.almd:92–96 load prompt filenames; lang-bench/runner_edits.rb:94 loads prompt.md. Preserve experimental stimulus identity; do not replace or feed translations into original benchmark.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/spike/charge-probe/BURNDOWN.md`
- Language: Japanese or mixed (provisional); size: 2299 whitespace tokens / 17009 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/spike/charge-probe/REPORT.md`
- Language: Japanese or mixed (provisional); size: 1770 whitespace tokens / 13448 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - `tests/charge_probe_test.rs:3://! Three layers, per research/spike/charge-probe/REPORT.md:`

### `research/spike/logical-time-race/GATE.md`
- Language: Japanese or mixed (provisional); size: 200 whitespace tokens / 1559 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/spike/substrate-observability/README.md`
- Language: English (provisional); size: 330 whitespace tokens / 2238 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `research/spike/v1-mir/GATE.md`
- Language: Japanese or mixed (provisional); size: 389 whitespace tokens / 3072 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `runtime/rs/README.md`
- Language: English (provisional); size: 1018 whitespace tokens / 7270 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - `scripts/check-runtime-test-placement.sh:12:#     package fails with 220 rustc errors — see runtime/rs/README.md.`
  - `scripts/check-runtime-test-placement.sh:36:  echo "it runs on every leg. See runtime/rs/README.md."`
  - `crates/almide-codegen/src/generated/rust_runtime.rs:28:    ("random", "// random extern — Rust native implementations (no external crate)\n// int/float/choice/shuffle use a per-thread xorshift seeded from OS entropy\n// (#4122; not for secrets); random.bytes reads the OS CSP [... omitted end of long line]`
  - `tests/runtime_random_float_construction_test.rs:5:// there runs nowhere (runtime/rs/README.md; scripts/check-runtime-test-placement.sh`

### `spec/churn/README.md`
- Language: English (provisional); size: 232 whitespace tokens / 1669 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `spec/gauntlet/README.md`
- Language: English (provisional); size: 1875 whitespace tokens / 12667 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - `crates/almide-wasm/tests/gauntlet.rs:5://! tracker (spec/gauntlet/README.md §1).`
  - `crates/almide-tools/tests/fmt_corpus_test.rs:58:        // (the manifest pins their front rejection — spec/gauntlet/README.md);`

### `stdlib/README.md`
- Language: English (provisional); size: 347 whitespace tokens / 3230 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - `scripts/test_ci_prose.py:112:                     "docs/GRAMMAR.md", "docs/stdlib/list.md", "docs/stdlib/README.md", "docs/roadmap/README.md",`

### `tools/claude-plugin/README.md`
- Language: English (provisional); size: 93 whitespace tokens / 725 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

### `tools/xtarget-fuzz/README.md`
- Language: English (provisional); size: 2196 whitespace tokens / 14923 characters.
- Status: **Pending — body/indirect consumers not fully reviewed**.
- Reason: Exact-reference search only; no safety approval.
- Direct references:
  - None found in searched roots (not proof of no consumers).

## Status counts

- Pending — body/indirect consumers not fully reviewed: 59
- Machine-read prose — controlled translation: 1
- LLM surface — validation coverage needed: 1
- Normative — special authority handling: 1
- Adjacent translation candidate — limited audit: 5
- Measurement evidence — companion / coverage review: 1
- Historical generated evidence — separate companion: 1
- Machine-checked host guide — controlled translation: 1
- Machine-checked evidence — controlled translation: 1
- Machine-read evidence ledger — separate companion: 1
- Experiment input — separate explanatory translation: 8
