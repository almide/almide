# The common issue header + area label — spec for rewriting almide/almide issues

Every open issue gets (A) ONE block PREPENDED to its body, above everything that
is already there, and (B) an area label saying which crate it lives in. Nothing
in the original body is deleted, reworded or reordered. The block exists so that
a reader who opens this issue alone, with no other context, knows what to do and
how they will know they are done.

Worked example already published: **#2397** — read it first
(`gh issue view 2397`) and match its shape.

## (A) The block, verbatim in shape

```markdown
> **状態** — <one of: BLOCKER / 実装 / 計測器・ゲート / 設計判断 / 追跡 / 調査>・<one of: 着手可 / 前提待ち: #NNNN / 判断待ち>
> **一行で** — <what is wrong or missing, in one sentence, no background>
>
> **やること**
> 1. <concrete first action a fresh reader can start today>
> 2. <…>
>
> **完了条件** — <a verifiable condition: a gate that goes green, a measured number, a fixture that exists, a diagnostic that fires. Never "it feels done">
> **根拠** — <the measurement, file:line, commit or PR that makes this real; or `未測定` if the issue rests on a claim nobody has checked>
> **触る場所** — <paths, 2-4 of them, the ones a fix would actually edit>

---
```

状態 meanings (pick the single closest):

| 状態 | admits |
|---|---|
| `BLOCKER` | carries `I-unsound` / `I-miscompile` / `I-divergence` / `regression` — a final release tag is refused while it is open |
| `実装` | a change to compiler/stdlib/runtime code with a known target shape |
| `計測器・ゲート` | the work is a gate, a ratchet, a manifest, a measurement harness, or fixing one that lies |
| `設計判断` | needs a ruling before any code (usually labeled `mob`) |
| `追跡` | an umbrella whose items are other issues, or a bot-maintained ledger |
| `調査` | the subject is not yet pinned down; step 1 is a measurement, not an edit |

## Rules for filling it

1. **Derive, do not invent.** Every line comes from the issue's own body and
   comments, or from the tree. If the issue does not say what done looks like,
   say what it would take to decide — that is the honest 完了条件.
2. **Verify the state when verifying is cheap** (running the issue's own
   3-line repro, `gh pr view` on a PR it names, `ls` on a path it claims is
   missing, `grep` for a symbol it says does not exist). Do NOT start builds,
   benchmarks or test suites for this task. If the state has changed since
   filing, say so in 一行で and leave the original text to show what it was.
3. **`根拠` names something checkable** — `crates/…/foo.rs:120`, a PR number, a
   measured pair of numbers quoted from the issue, a gate script. If nothing is
   checkable, write `未測定` rather than a plausible sentence. An unverified
   claim in this field is worse than an empty one. If the issue's own numbers
   are the evidence, say whose they are: `本文の実測(2026-09-13)`.
4. **No new claims.** Do not add analysis, do not propose designs the issue does
   not already contain, do not estimate effort or duration. This is a wrapper,
   not a rewrite.
5. **Japanese for the labels, the content in whatever language the issue uses.**
   The six field names are fixed and in Japanese so a reader can scan them.
6. **`前提待ち: #NNNN`** only when the issue cannot start before that one lands.
   Being *related* is not a prerequisite.
7. **No neighbour project names** anywhere — never write the names of the
   comparison languages/toolchains in an issue. "a neighbouring toolchain" /
   「近隣実装」 is the spelling. (Standing instruction from the user.)

## (B) The area label

Exactly ONE `A-*` label per issue (two only when the work genuinely spans both
crates), chosen from the labels that already exist — do not create new ones:

| label | crates |
|---|---|
| `A-frontend` | `crates/almide-frontend`, `almide-syntax` — parse, check, diagnostics, import table |
| `A-ir` | `crates/almide-ir`, `almide-mir`, `almide-optimize`, `almide-spine` |
| `A-codegen` | `crates/almide-codegen` — nanopasses, Rust lowering, native borrow/RC inference |
| `A-wasm` | `crates/almide-wasm`, `almide-wasm-run`, `almide-wasm-vm` |
| `A-runtime` | `runtime/rs`, `crates/almide-rt-core` — native intrinsics |
| `A-interp` | `crates/almide-interp` — the third oracle |
| `A-stdlib` | `stdlib/*.almd`, `crates/almide-types` stdlib registry |
| `A-driver` | `crates/almide-driver`, `src/` — CLI: run/build/test/check/fmt/bench |
| `A-perf` | `research/benchmark/perf`, `scripts/check-perf-ratio.sh` |
| `A-ci` | `.github/workflows`, `scripts/*.sh` gates, ledgers and ratchets |
| `A-modules` | module and package system — resolution, submodules, MVS, dialect epochs |

- The label must agree with 触る場所. If you cannot pick one, the issue's paths
  are unclear — say so in your report and add no `A-*` label.
- Add `bug` or `enhancement` if the issue has neither and the class is obvious
  from its body (a wrong behaviour = `bug`, new surface = `enhancement`).
- **Never add, remove or change** `I-*`, `regression`, `P-*` or `mob`. Those are
  severity/scheduling/mob decisions and not ours to make in a formatting pass
  (`docs/project/ISSUE-TAXONOMY.md`).
- Apply with `gh issue edit <n> --add-label A-xxx` (repeat `--add-label`).

## Mechanics (this part is not optional)

- Fetch first: `gh issue view <n> --json body --jq .body > $SCRATCH/i<n>.md`
- Build the new body by PREPENDING the block to that file (`cat hdr i<n>.md >
  i<n>.new.md`); never pass `--body` built from anything but the fetched file.
- Write: `gh issue edit <n> --body-file $SCRATCH/i<n>.new.md`
- Re-read and check the round trip in a way that cannot lie: fetch the published
  body again and assert the ORIGINAL's first 200 characters still occur in it
  (compare in python, not by eye). If they do not, restore from the fetched copy
  (`gh issue edit <n> --body-file $SCRATCH/i<n>.md`) and report the issue as
  skipped.
- Idempotence: if the fetched body ALREADY starts with `> **状態** —`, the issue
  is done — do not prepend a second block. Check the label and move on.
- Skip and report, do not guess, when the body is empty or the fetch fails.
  A tracking issue whose body is a checkbox list of other issues DOES get the
  block, with every checkbox left untouched.

## Report back

One line per issue: `#NNNN <状態> <A-label> ok` — plus a short note for any
issue you skipped, any where verification contradicted the body, and any where
you could not choose an area label.
