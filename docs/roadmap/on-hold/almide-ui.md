<!-- description: SolidJS-like reactive UI framework built as a pure Almide library -->
# Almide UI — Reactive Web Framework as Almide Library

## Thesis

Build a SolidJS-like reactive UI framework written in Almide **as an Almide library**. No framework-specific optimization passes are added to the compiler. The compiler's general-purpose optimizations (type specialization, inlining, pipeline fusion) apply equally to framework code, resulting in Svelte/SolidJS-class output.

```
What the user writes: Almide builder syntax + signal library
What the compiler does: Optimize as regular Almide code → TS output
What comes out: No virtual DOM, fine-grained updates in plain JS
```

**Framework knowledge stays within the library. The compiler only needs to know Almide.**

## Why This Is The Right Approach

### Comparison with Other Approaches

| Approach | Example | Problem |
|---|---|---|
| Framework built into compiler | Svelte | Compiler becomes complex. Framework evolution = compiler rework |
| JS runtime library | React, Vue | Runtime cost of virtual DOM diffing. Bundle size |
| JS library + fine-grained | SolidJS | Fastest class. But no JS type info limits optimization ceiling |
| **Almide library** | **This proposal** | SolidJS model + additional optimization via Almide's type info |

### Benefits of Being an Almide Library

1. **No compiler changes required** — Works with just builder machinery and general-purpose optimizations
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

**From the compiler's perspective, this is just regular Almide code.** `get` is a one-line function, so it gets inlined. `Signal[Int]` gets monomorphized.

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

Both framework and user code are the same Almide to the compiler. The following optimizations apply automatically:

### 1. Inlining

```almide
// Definition of signal.get()
fn get[T](s: Signal[T]) -> T = s.value

// User code
p { "Count: ${count.get()}" }

// After inlining (compiler internal)
p { "Count: ${count.value}" }

// TS output
p.textContent = `Count: ${count.value}`;
```

Signal's `.get()` / `.set()` are one-line functions. Once the compiler inlines them, Signal overhead drops to zero.

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

The library alone can achieve SolidJS-class performance. To aim for Svelte-class, the compiler can **optionally** add a pass that recognizes Signals:

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

**The library can achieve the same thing at runtime** (which is what SolidJS does), so compiler support is purely an optional optimization. It can be added later.

## Competitive Position

```
                    Compile-time analysis  Runtime cost       Ecosystem
Svelte              ◎ (compiler)           ◎ (minimal)        ○ (JS)
SolidJS             △ (JSX transform)      ◎ (signal)         ○ (JS)
React               × (runtime diff)       △ (vDOM)           ◎ (largest)
Leptos (Rust→WASM)  ○ (signal)             ○ (WASM overhead)  △ (WASM FFI)
Almide UI           ○→◎ (can add later)    ◎ (inline signal)  ◎ (native JS output)
```

Almide UI differentiators:
- **Emits plain JS, not WASM** — Fastest at the edge, fully ecosystem-compatible
- **Same code becomes native via `--target rust`** — SSR as Rust binary, client as TS, all in one language
- **Type info penetrates framework internals** — Optimization headroom that SolidJS lacks
- **Framework is independent of the compiler** — Unlike Svelte, can evolve as a library

## Relationship to Other Roadmap Items

- **ts-edge-native.md**: Output target for Almide UI. Emitter optimization (Phase 1) directly impacts Almide UI performance
- **Result Builder (template.md)**: `builder Html` is exactly the UI description foundation for this framework. Once the builder machinery is complete, Almide UI's DOM construction works naturally
- **cross-target-semantics.md**: The SSR (Rust) + Client (TS) single-language story requires cross-target correctness as a prerequisite
- **emit-wasm-direct.md**: Independent. Almide UI uses TS output, so it's unrelated to the WASM path

## Prerequisites

1. **Builder machinery (template.md Phase 1)** — Without `builder Html` and trailing blocks, UI descriptions can't be written
2. **Emitter optimization (ts-edge-native.md Phase 1)** — Without type specialization and inlining, runtime overhead is too large
3. **Generics stability** — `Signal[T]` needs correct type checking and codegen
4. **Closure codegen** — `subscribe(fn() => ...)` needs correct TS output

## Why ON HOLD

Builder machinery and emitter optimization come first. However:

- **No design blockers** — Signal is just an Almide type, DOM bridge is `@extern`, builder is already designed
- **No compiler changes required** — If general-purpose optimizations work, framework-specific passes are optional
- **Can be built incrementally** — reactive core → DOM bridge → renderer → builder integration, each step is independent
- **SolidJS has proven this works** — Library approach + fine-grained signals achieves Svelte-class performance

Confidence is high. Once the prerequisite language features are in place, the framework itself is just Almide code.
