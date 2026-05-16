<!-- description: Ceangal UI framework — widget system, reactive state, GPU rendering pipeline -->
# Ceangal Widget System

> **Active scope: Phase 1-3** — View type + diff, cell + memo, each + key reconciliation.
> Scroll/virtual list foundation ✅ complete (24 tests, 1000 items 60fps).
> **Exit criteria**: Todo app running with cell-based reactivity at 60fps.

Ceangal is the UI framework for Almide. Mission: **"LLM が最も正確に UI を実現できるフレームワーク"**.

## Architecture

```
Application (.almd)
  ↓ import
Ceangal (dubhlux/ceangal) — View, cell, memo, scroll, virtual list
  ↓ import
almide-web (almide/almide-web) — DOM, fetch, timer, console
  ↓ import
almide-wasm-bindgen (almide/almide-wasm-bindgen) — ABI / type marshalling
  ↓ compile
almide (almide/almide) — .almd → .wasm
  ↓ render
snaidhm (dubhlux/snaidhm) — GPU tiled path renderer
```

## Core API (7 concepts)

| API | Role |
|-----|------|
| `cell(initial)` | Reactive state (global or local) |
| `.get()` | Read value (dependency tracking) |
| `.set(value)` | Write value (triggers update) |
| `.update(fn)` | Atomic update `(old) -> new` |
| `memo(fn)` | Derived data (cached if deps unchanged) |
| `each(items, key, render)` | Key-based list rendering |
| `on_mount(fn)` | Lifecycle (return value = cleanup) |

## Rendering Model

```
cell.set(value)
  → app() re-execution (0.14ms / 100 nodes)
  → memo/each cache hit for unchanged data
  → diff old vs new View tree (0.01ms / 100 nodes)
  → GPU patch (changed nodes only)
```

v1: Full tree rebuild + diff. v2 (future): Lazy children → O(1) build.

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Widget name | View | SwiftUI proven, LLM 100% accuracy |
| Styling | record `{}` + pipe `\|>` | 2 patterns only. Tailwind-inspired utility chain |
| State | cell + memo | Minimal API. No signals/hooks ceremony |
| State management | Thin core (React/Flutter model) | Patterns in examples, not enforced |
| Rendering | app() rebuild + diff | 0.14ms/100 nodes, within frame budget |
| List | each with key-based reconciliation | O(N) scan + O(changed) rebuild |
| Interaction | ViewState (hovered/pressed/focused) | Separate from cell tracking, GPU paint-only |

## Ecosystem: almide-web

Browser API bindings extracted from Ceangal into `almide/almide-web`:

| Module | APIs |
|--------|------|
| dom | Element creation, attributes, styles, string interning |
| fetch | HTTP GET/POST with async callback |
| timer | setTimeout, setInterval, requestAnimationFrame |
| console | log, warn, error |

almide-js archived — replaced by almide-web.

## Phases

### Phase 1: View + Diff ← current
- [ ] View type + constructors (text, col, row, box)
- [ ] Pipe utilities (bg, padding, font, gap, grow, etc.)
- [ ] Recursive diff (benchmarked: 0.09ms / 1000 nodes)
- [ ] Bench harness

### Phase 2: Cell + Dispatch
- [ ] cell type (.get / .set / .update)
- [ ] Dependency tracking (get records current context)
- [ ] memo (computed cache + reference equality propagation)
- [ ] Dispatch: cell change → app() rebuild → diff → patch

### Phase 3: each + Key Reconciliation
- [ ] Key-based item scan
- [ ] Per-item rebuild (COW reference skip)
- [ ] Subtree identity (key = local cell lifetime)

### Phase 4: Render Pipeline
- [ ] View tree → LayoutNode conversion
- [ ] layout.almd (flexbox, 56 tests)
- [ ] LayoutRect → GPU commands (snaidhm)
- [ ] Paint-only vs layout change detection

### Phase 5: Interaction
- [ ] ViewState (hovered, pressed, focused)
- [ ] (View) -> Value pipe re-evaluation
- [ ] Hit test integration (scroll.almd)
- [ ] on_mount / cleanup lifecycle

### Phase 6: Widgets + Theme
- [ ] button, checkbox, text input
- [ ] Design tokens (colors, spacing, radius)
- [ ] Virtual list integration
- [ ] Music player demo restoration

## Completed Foundation

| Module | Tests | Status |
|--------|-------|--------|
| scroll.almd | 24 | Multi-region, nested bubble, scrollbar, hit test |
| virtual_list.almd | — | GPU procedural rendering, 1000 items 60fps |
| layout.almd | 56 | Yoga-aligned Flexbox |
| dom.almd | — | DOM overlay for text selection |
| snaidhm | — | Tiled path renderer + SDF text + content-space tiling |

## Benchmark Results

| Metric | Value | Target |
|--------|-------|--------|
| diff: 1000 nodes unchanged | 0.087ms | < 2ms ✅ |
| diff: 1000 nodes 1 paint | 0.108ms | < 2ms ✅ |
| diff: 5000 nodes unchanged | 0.884ms | — ✅ |
| build: 100 nodes | 0.13ms | < 3ms ✅ |
| build: 1000 nodes | 4.9ms | < 16.7ms 🟡 |
| build: 5000 nodes | 146ms | ❌ (virtual list required) |

## Compiler Requirements

| Requirement | Status |
|-------------|--------|
| COW List index assign (WASM) | ✅ 0.17.3 |
| COW List index assign (Rust) | ✅ 0.17.5 |
| syntax_guide false positive fix | ✅ 0.17.6 |
| @export annotation string | ✅ 0.17.6 |
| COW List → Vec (Rust codegen) | ❌ pending |
| Record spread `{ ...r, field: val }` | ✅ works (tested, pipe utilities use it) |
| UFCS on generic types (Rust codegen) | ✅ 0.17.7 — mono discovery + cross-module type checker + lowering |
| almide-web created | ✅ (DOM, fetch, timer, console) |
| almide-js archived | ✅ |
