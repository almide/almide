> Source description: Playground AI-powered error repair and type checker integration
> Source done metadata: 2026-03-11
<a id="playground-repair-turn"></a>

# Playground Repair Turn

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](playground-repair.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](playground-repair.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](playground-repair.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/playground-repair.md)
> The done metadata coexists with unchecked tasks and a v0.4.7 status snapshot. Preserve that historical distinction: the proposed 1.2-turn metric is an example, and one-turn repair is a demo success criterion, not a measured current guarantee.


<a id="vision"></a>

## Vision

User writes code in Playground, runs it, gets an error, clicks "Fix with AI", the LLM reads the error and fixes it, and the user watches the repair process.

A demo that showcases Almide's "error messages are clear enough for LLMs to fix in one turn."

<a id="current-state-v047"></a>

## Current State (v0.4.7)

<a id="done"></a>

### Done
- [x] Added type checker to Playground (parse -> **check** -> emit)
- [x] Repair turns in AI generation flow (generate -> error -> auto-repair x 3)
- [x] "Fix with AI" button for manual Run (Run -> error -> button -> repair loop)
- [x] Anthropic / OpenAI / Gemini streaming support
- [x] repair-log UI (step display for error/fix/ok/fail)

<a id="not-yet"></a>

### Not Yet
- [ ] Update Playground almide dependency to v0.4.7 (after PR #15 merge)
- [ ] Include list.swap and immutable patterns in CLAUDE.md (system prompt)
- [ ] Diff display of repair results
- [ ] Code comparison before/after repair (side-by-side or inline diff)
- [ ] "Accept fix" / "Reject fix" buttons

<a id="architecture"></a>

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

<a id="repair-prompt-strategy"></a>

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

<a id="roadmap"></a>

## Roadmap

<a id="tier-1--completeness-short-term"></a>

### Tier 1 — Completeness (short-term)

<a id="11-update-playground-almide-dependency"></a>

#### 1.1 Update Playground almide dependency
After PR #15 merge, update the almide git ref in `Cargo.toml`.
Confirm that the checker works in WASM.

<a id="12-system-prompt-optimization"></a>

#### 1.2 System prompt optimization
Include Almide-specific patterns in the Playground LLM repair prompt:
- `var` vs `let` usage differences
- `list.swap` for in-place algorithms
- `list.get` returns nullable → use `list.get_or`
- tuple return for functions that modify + return

<a id="13-diff-display"></a>

#### 1.3 Diff display
Show before/after code diff. Users can understand "what changed" at a glance.
A simple inline diff (green for added lines, red for removed) is sufficient.

<a id="tier-2--ux-improvements-medium-term"></a>

### Tier 2 — UX Improvements (medium-term)

<a id="21-accept--reject"></a>

#### 2.1 Accept / Reject
After "Fix with AI", show the repaired code tentatively. User chooses Accept or Reject.
Reject restores the original code.

<a id="22-partial-repair"></a>

#### 2.2 Partial repair
Highlight only the error location and repair just that function.
More accurate than full rewrite for LLMs.

<a id="23-repair-history"></a>

#### 2.3 Repair history
Display multiple repair turns in chronological order.
Shows "what was tried and what was fixed."

<a id="tier-3--benchmark-integration-long-term"></a>

### Tier 3 — Benchmark Integration (long-term)

<a id="31-modification-survival-rate-visualization"></a>

#### 3.1 Modification survival rate visualization
Record the number of repair turns and display metrics like "Almide repairs in 1.2 turns on average."
Embed comparison data with other languages in the Playground.

<a id="32-auto-repair-mode"></a>

#### 3.2 Auto-repair mode
Add an "Auto-repair" toggle to the Run button. When ON, repair starts automatically on error.
Users watch the repair process in real time.

<a id="success-metric"></a>

## Success Metric

quicksort (immutable patterns) in the Playground:
1. User writes with mutable pattern -> `cannot reassign immutable binding` error
2. "Fix with AI" -> LLM fixes with `var` + tuple return + `list.swap`
3. Repair completes in 1 turn, sorted result is displayed

If this works, it completes the demo for "Almide errors are clear enough for LLMs to fix."
