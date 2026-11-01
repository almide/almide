<!-- description: SolidJS-like reactive UI framework built as a pure Almide library -->
# Almide UI — Reactive Web Framework as Almide Library

## Thesis

Proposal: build a reactive UI library in Almide while keeping framework-specific
knowledge out of the compiler where possible. The original design relied on TS
output, which is retired. Choosing a supported browser host/DOM bridge is now
a prerequisite; no Svelte/SolidJS-class performance or no-compiler-change
feasibility has been demonstrated. Code below preserves the original design
sketches, not a runnable current Almide framework.

```
What the user writes: Almide builder syntax + signal library
Original backend assumption (retired): optimize Almide code → TS output
Desired result: no virtual DOM, fine-grained updates; current host mapping unresolved
```

**Design goal: keep framework knowledge within the library.** Host interop and
optimization coverage must be established before claiming this needs no compiler work.

## Why This Is The Right Approach

### Comparison with Other Approaches

| Approach | Example | Problem |
|---|---|---|
| Framework built into compiler | Svelte | Compiler becomes complex. Framework evolution = compiler rework |
| JS runtime library | React, Vue | Runtime cost of virtual DOM diffing. Bundle size |
| JS library + fine-grained | SolidJS | Fastest class. But no JS type info limits optimization ceiling |
| **Almide library** | **This proposal** | SolidJS model + additional optimization via Almide's type info |

### Benefits of Being an Almide Library

1. **Minimize compiler changes** — evaluate existing language/host support before deciding whether a library-only implementation is sufficient
2. **Framework is written in Almide** — Same language as user code, same optimization target
3. **Type info penetrates framework internals** — `.get()` on `Signal[Int]` is confirmed to return Int. Type specialization reaches all the way down
4. **Framework evolution is independent of the compiler** — Library version upgrades don't require compiler releases
5. **Multi-target benefits** — Framework tests can run fast via `--target rust`

## Architecture

### Layer Structure

```
almide-ui/                    ← Almide package (library)
├── reactive.almd             Signal, Effect, Derived
├── dom.almd                  DOM primitives (bridged via @extern)
├── component.almd            Component lifecycle
├── renderer.almd             Signal→DOM update connection
└── html.almd                 builder Html + element functions (div, p, ...)
```

### Reactive Core

```almide
// reactive.almd

type Signal[T] = {
  var value: T,
  var subs: List[fn() -> Unit],
}

fn signal[T](initial: T) -> Signal[T] =
  { value: initial, subs: [] }

fn get[T](s: Signal[T]) -> T = s.value

fn set[T](s: Signal[T], new_val: T) -> Unit = {
  s.value = new_val
  for sub in s.subs { sub() }
}

fn subscribe[T](s: Signal[T], f: fn() -> Unit) -> Unit =
  s.subs = s.subs ++ [f]

fn derived[T](compute: fn() -> T) -> Signal[T] = {
  // Automatically tracks dependent signals and returns computed value
  // Equivalent to SolidJS's createMemo
  ...
}
```

This is a proposed representation. A one-line body does not by itself guarantee
inlining: current inliners have scalar, scope and ownership admission rules.
Validate the representation and generated code before claiming zero overhead.

### DOM Bridge

```almide
// dom.almd — minimal @extern bridge

@extern(ts, "document.createElement(tag)")
fn create_element(tag: String) -> DomNode

@extern(ts, "node.textContent = text")
fn set_text(node: DomNode, text: String) -> Unit

@extern(ts, "parent.appendChild(child)")
fn append(parent: DomNode, child: DomNode) -> Unit

@extern(ts, "node.addEventListener(event, handler)")
fn on(node: DomNode, event: String, handler: fn() -> Unit) -> Unit

@extern(ts, "node.setAttribute(name, value)")
fn set_attr(node: DomNode, name: String, value: String) -> Unit
```

The DOM API can be fully covered with 10-15 `@extern` declarations. The framework's job is to combine these to provide a declarative API.

### Renderer — Connecting Signals to the DOM

```almide
// renderer.almd

fn reactive_text(parent: DomNode, s: Signal[String]) -> Unit = {
  let node = create_element("span")
  set_text(node, s.get())
  append(parent, node)
  // When signal changes, only update textContent
  s.subscribe(fn() => set_text(node, s.get()))
}

fn reactive_attr(node: DomNode, name: String, s: Signal[String]) -> Unit = {
  set_attr(node, name, s.get())
  s.subscribe(fn() => set_attr(node, name, s.get()))
}

fn reactive_list[T](
  parent: DomNode,
  items: Signal[List[T]],
  render: fn(T) -> DomNode,
) -> Unit = {
  // List diff update (keyed reconciliation)
  ...
}
```

### User Code

```almide
import almide_ui exposing (signal, get, set, derived)
import almide_ui/html exposing (div, h1, p, button, ul, li)

var count = signal(0)
let doubled = derived(fn() => count.get() * 2)

template app() -> HtmlDoc = Html {
  div(class: "app") {
    h1 { "Counter" }
    p { "Count: ${count.get()}" }
    p { "Doubled: ${doubled.get()}" }
    button(onclick: fn() => count.set(count.get() + 1)) {
      "Increment"
    }
  }
}
```

Users only need to know Signals and builder syntax. DOM operations are invisible.

## Compiler Optimization — What Happens Automatically

The following are desired optimization examples, not guaranteed transformations
of this unimplemented Signal API:

### 1. Inlining

```almide
// Definition of signal.get()
fn get[T](s: Signal[T]) -> T = s.value

// User code
p { "Count: ${count.get()}" }

// After inlining (compiler internal)
p { "Count: ${count.value}" }

// Original illustrative TS output (retired target)
p.textContent = `Count: ${count.value}`;
```

The proposed `.get()` / `.set()` optimization needs generated-code and runtime
measurements; function size alone does not establish zero Signal overhead.

### 2. Type Specialization (Monomorphization)

```almide
// Generic definition
fn get[T](s: Signal[T]) -> T = s.value

// Used with Signal[Int] → Int-specialized version is generated
// Used with Signal[String] → String-specialized version is generated
```

V8 processes monomorphic call sites at maximum speed. Type specialization makes all Signal operations monomorphic.

### 3. Pipeline Fusion

```almide
let visible_names = items.get()
  |> list.filter((x) => x.active)
  |> list.map((x) => x.name)
```

Fused into a single loop with no intermediate arrays. Reactive list updates become faster.

### 4. Dead code elimination

Unused Signal methods and DOM helpers are removed from output. Bundle size is minimized.

## Future Option: Compiler-Aware Signal Tracking

An optional research direction is a pass recognizing Signals. Neither
library-only performance parity nor the need for that pass is established:

```almide
// Compiler tracks Signal.get() calls inside builder blocks
Html {
  p { "Count: ${count.get()}" }     // ← depends on count
  p { "Name: ${name.get()}" }      // ← depends on name
  p { "Static text" }               // ← no dependencies
}
```

The compiler sees dependencies and auto-generates update functions:

```typescript
// create
const p0 = document.createElement('p');
p0.textContent = `Count: ${count.value}`;
const p1 = document.createElement('p');
p1.textContent = `Name: ${name.value}`;
const p2 = document.createElement('p');
p2.textContent = 'Static text';

// update — when count changes, only p0 updates
count.subscribe(() => { p0.textContent = `Count: ${count.value}`; });
// update — when name changes, only p1 updates
name.subscribe(() => { p1.textContent = `Name: ${name.value}`; });
// p2 has no subscription (static, no update needed)
```

A runtime dependency-tracking implementation is another candidate. Its
correctness, host integration and cost need measurement before choosing between
it and compiler-assisted alternatives.

## Competitive Position

```
                    Compile-time analysis  Runtime cost       Ecosystem
Svelte              ◎ (compiler)           ◎ (minimal)        ○ (JS)
SolidJS             △ (JSX transform)      ◎ (signal)         ○ (JS)
React               × (runtime diff)       △ (vDOM)           ◎ (largest)
Leptos (Rust→WASM)  ○ (signal)             ○ (WASM overhead)  △ (WASM FFI)
Almide UI proposal  unmeasured           unmeasured         host mapping unresolved
```

Almide UI differentiators:
- **Client output** — unresolved after retirement of the TypeScript backend; the original plain-JS performance/compatibility hypothesis is not a shipping capability or measured result
- **Shared SSR/client code** — a goal, not current TS output; a supported client host and boundary remain to be designed
- **Type-directed optimization** — a proposed opportunity, not an established advantage over SolidJS
- **Library boundary** — a design goal; any required compiler integration must still be identified

## Relationship to Other Roadmap Items

- **Original TS emitter roadmap**: historical input only; it is not an available output target
- **Result Builder (template.md)**: `builder Html` is exactly the UI description foundation for this framework. Once the builder machinery is complete, Almide UI's DOM construction works naturally
- **Cross-target semantics**: any SSR/client adapter needs explicit compatibility tests, not an assumption inherited from the old TS plan
- **Wasm/browser host**: a possible replacement direction, not a selected or implemented UI backend

## Prerequisites

1. **Builder machinery (template.md Phase 1)** — Without `builder Html` and trailing blocks, UI descriptions can't be written
2. **Supported client backend** — select and validate a browser host/DOM boundary; the old TS emitter prerequisite is retired
3. **Generics stability** — `Signal[T]` needs correct type checking and codegen
4. **Closure/ownership semantics** — validate subscription lifetimes and callbacks on the selected host

## Why ON HOLD

The client backend, DOM boundary, builder syntax and subscription ownership
remain design dependencies. The original library-first rationale is retained,
but it does not close those questions:

- **Design work remains** — a Signal sketch, an extern declaration and a builder proposal do not establish a working DOM integration
- **Compiler scope undecided** — measure what current generic optimizations support before selecting framework-specific work
- **Can be built incrementally** — reactive core → DOM bridge → renderer → builder integration, each step is independent
- **Prior-art motivation** — library-first fine-grained reactivity is worth evaluating; its existence elsewhere is not a benchmark of this proposal

Confidence is high. Once the prerequisite language features are in place, the framework itself is just Almide code.
