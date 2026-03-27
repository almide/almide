<!-- description: Playground AI-powered error repair and type checker integration -->
<!-- done: 2026-03-11 -->
# Playground Repair Turn

## Vision

User writes code in Playground, runs it, gets an error, clicks "Fix with AI", the LLM reads the error and fixes it, and the user watches the repair process.

A demo that showcases Almide's "error messages are clear enough for LLMs to fix in one turn."

## Current State (v0.4.7)

### Done
- [x] Added type checker to Playground (parse -> **check** -> emit)
- [x] Repair turns in AI generation flow (generate -> error -> auto-repair x 3)
- [x] "Fix with AI" button for manual Run (Run -> error -> button -> repair loop)
- [x] Anthropic / OpenAI / Gemini streaming support
- [x] repair-log UI (step display for error/fix/ok/fail)

### Not Yet
- [ ] Update Playground almide dependency to v0.4.7 (after PR #15 merge)
- [ ] Include list.swap and immutable patterns in CLAUDE.md (system prompt)
- [ ] Diff display of repair results
- [ ] Code comparison before/after repair (side-by-side or inline diff)
- [ ] "Accept fix" / "Reject fix" buttons

## Architecture

```
User writes code
       │
       ▼
   ┌──────────┐
   │   Run     │
   └────┬─────┘
        │
   ┌────▼─────┐     ok     ┌──────────┐
   │ compile   │───────────▶│  Output   │
   │ + run     │            └──────────┘
   └────┬─────┘
        │ error
   ┌────▼──────────┐
   │ Show error +   │
   │ "Fix with AI"  │
   └────┬──────────┘
        │ click
   ┌────▼──────────┐
   │ LLM repair    │◀──┐
   │ (stream)      │   │ error (max 3)
   └────┬──────────┘   │
        │              │
   ┌────▼─────┐   ┌───┴────┐
   │ compile   │──▶│ retry  │
   │ + run     │   └────────┘
   └────┬─────┘
        │ ok
   ┌────▼─────┐
   │ Output + │
   │ "Fixed!" │
   └──────────┘
```

## Repair prompt strategy

Current:
```
{phase} error:
{error message}

Fix the code and output ONLY the corrected .almd source. No explanations.
```

Improved (TODO):
- Include Almide grammar overview + common error patterns in system prompt
- `cannot reassign immutable binding` -> decide between `var` or tuple return pattern
- `list.get returns Option` -> suggest `list.get_or` or `list.swap`

## Roadmap

### Tier 1 — Completeness (short-term)

#### 1.1 Update Playground almide dependency
After PR #15 merge, update the almide git ref in `Cargo.toml`.
Confirm that the checker works in WASM.

#### 1.2 System prompt optimization
Include Almide-specific patterns in the Playground LLM repair prompt:
- `var` vs `let` の使い分け
- `list.swap` for in-place algorithms
- `list.get` returns nullable → use `list.get_or`
- tuple return for functions that modify + return

#### 1.3 Diff display
Show before/after code diff. Users can understand "what changed" at a glance.
A simple inline diff (green for added lines, red for removed) is sufficient.

### Tier 2 — UX Improvements (medium-term)

#### 2.1 Accept / Reject
After "Fix with AI", show the repaired code tentatively. User chooses Accept or Reject.
Reject restores the original code.

#### 2.2 Partial repair
Highlight only the error location and repair just that function.
More accurate than full rewrite for LLMs.

#### 2.3 Repair history
Display multiple repair turns in chronological order.
Shows "what was tried and what was fixed."

### Tier 3 — Benchmark Integration (long-term)

#### 3.1 Modification survival rate visualization
Record the number of repair turns and display metrics like "Almide repairs in 1.2 turns on average."
Embed comparison data with other languages in the Playground.

#### 3.2 Auto-repair mode
Add an "Auto-repair" toggle to the Run button. When ON, repair starts automatically on error.
Users watch the repair process in real time.

## Success Metric

quicksort (immutable patterns) を Playground で:
1. User writes with mutable pattern -> `cannot reassign immutable binding` error
2. "Fix with AI" -> LLM fixes with `var` + tuple return + `list.swap`
3. Repair completes in 1 turn, sorted result is displayed

If this works, it completes the demo for "Almide errors are clear enough for LLMs to fix."
