<a id="variant-exhaustiveness-refinement"></a>

# Variant Exhaustiveness Refinement

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](variant-exhaustiveness-refinement.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](variant-exhaustiveness-refinement.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](variant-exhaustiveness-refinement.zh-CN.md)

> Non-normative reader snapshot of the [mixed Japanese/English canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/variant-exhaustiveness-refinement.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. The original remains authoritative. This snapshot is not automatically synchronized and is not generator/checker input. Dates, plans, completion status, measurements and comparative claims below are retained as historical source statements, not independently reproduced or validated for a current release. Code blocks and comments remain verbatim. Historical collection metadata is displayed as an inactive reader note.

> Historical description: Non-exhaustive match suggests missing arm code; unreachable arms become hard errors. Done metadata: 2026-04-20.

> Source caveat: the completion section distinguishes four compiler features from documentation-only closure of §5, whose compiler auto-default was deferred. The older design, acceptance list and Still open section are retained, not a claim that automatic Codec derivation shipped. Guard diagnostics, temporary holes and examples retain their historical scope, not current runnable guidance or independent MSR results.

<a id="completion-status-2026-04-20"></a>

## Completion status (2026-04-20)

Arc landed across 4 sub-sections (§1–§4 as compiler features, §5 at
documentation level):

- **§1 paste-ready missing arms** (2026-04-19) — `MissingArm`
  carries both the compact witness pattern (`Node(_, _)`) and a
  paste-ready arm template; E010 hint renders the template block.
- **§2 unreachable → error** (2026-04-20) — Maranget §3 usefulness
  check via `find_unreachable_arms`; each dead arm fires E011.
  Opaque/Infinite column types route through default-matrix so
  generics don't false-positive.
- **§3 nested exhaustiveness** (2026-04-20) — detection was already
  correct (Maranget is recursive); `fmt_arm_head` became recursive
  so nested witnesses render as `Node(Node(arg1, arg2), Leaf) => _`.
- **§4 guard totality note** (2026-04-20) — when any arm has a
  guard, E010 appends "guarded arms do NOT count toward
  exhaustiveness — the guard can fail at runtime" so the user sees
  the actual reason their `X if cond => ...` didn't cover X.
- **§5 Variant Codec auto-derive** (2026-04-20) — scope closed at
  documentation level: the convention is codified in
  `docs/CHEATSHEET.md` ("variant serialization — recommended
  pattern"). A compiler-level auto-default was deferred because the
  cost (2 extra fns per variant type across stdlib + user code) and
  blast radius (existing Codec decode paths still carry
  `"payload decode not yet implemented"` stubs for non-trivial
  payloads) outweigh the MSR benefit for LLMs that already learn
  `deriving Codec` from one documented example. Re-scope as a
  distinct arc if dojo MSR data shows LLM drift on the
  explicit-derive form.

<a id="motivation"></a>

## Motivation

Pattern-match exhaustiveness checking already works, but **error messages stop at the names of missing arms**, which is insufficient for an LLM to repair them automatically. Unreachable arms currently receive warnings, but **an unreachable arm is a direct sign of an LLM generation error** and should be reported as an error.

This item aims to refine error messages until “the LLM that wrote the code can copy and paste them and pass,” directly increasing modification survival rate.

<a id="current-state"></a>

## Current State

```
Non-exhaustive match at line 14:3
  Missing: Node
```

Even after receiving this, the LLM may not remember the constructor arity or field names, and fail again.

<a id="design"></a>

## Design

<a id="1-paste-ready-missing-arms"></a>

### 1. Paste-ready missing arms

Present missing arms in a directly pasteable form:

```
Non-exhaustive match at line 14:3
  Missing arms:
    Node(left, right) => _
    Empty              => _
  Hint: add the arms above, or use `_ => todo()` to compile incrementally.
```

- List each constructor **with field names**
- The body is an `_` placeholder (compile-time hole)
- Also include an `_ => todo()` hint for “make it pass for now”

<a id="2-unreachable-arms-become-errors"></a>

### 2. Unreachable arms become errors

```
match tree {
  Leaf(_) => ...,
  Leaf(0) => ...,  // ← 到達不能
  Node(_, _) => ...,
}
```

This is currently a warning. Make it an error. Reasons:

- LLMs do not read warnings (stdout becomes noise)
- An unreachable arm is **highly likely to be a generation error** (misreading the preceding arm condition)
- An intentional “arm for the future” should be removed

<a id="3-nested-exhaustiveness"></a>

### 3. Nested exhaustiveness

Exhaustiveness for nested Variants:

```
match tree {
  Node(Leaf(_), _) => ...,
  // Missing: Node(Node(_,_), _)
}
```

If exhaustiveness is currently judged only at the outer layer of nesting, follow it into the inner layers.

<a id="4-guard-aware-exhaustiveness"></a>

### 4. Guard-aware exhaustiveness

Guarded arms are **excluded from exhaustiveness calculations**. With an `if x > 0` guard, that arm cannot be said to “definitely match.” Emit an error requiring `_ =>` after a guarded arm:

```
Guard at line 17 is not total
  Hint: guarded arms never contribute to exhaustiveness. Add a `_ => ...` arm.
```

<a id="5-auto-derived-codec-for-variants"></a>

### 5. Auto-derived Codec for variants

(Related) Apply the existing Codec convention (`Type.decode` / `Type.encode`) uniformly to variants too. Handwritten serialization is a source of LLM mistakes, so **auto-derive by default**, with `#[codec(manual)]` to opt out when needed.

<a id="acceptance-criteria"></a>

## Acceptance Criteria

- Non-exhaustive errors present ready-to-use code snippets for missing arms
- Unreachable arms become errors
- Exhaustiveness of nested Variants is detected
- An error if no `_ =>` follows a guarded arm
- Variant Codec is auto-derived by default
- Existing snapshot tests are updated and diagnostic golden files use the new format

<a id="progress"></a>

## Progress

<a id="2026-04-19--1-paste-ready-missing-arms"></a>

### 2026-04-19 — §1 paste-ready missing arms

`check_exhaustiveness` now returns `Vec<MissingArm>` carrying both the
compact witness pattern (`Node(_, _, _)`) and a paste-ready arm
template (`Node(arg1, arg2, arg3) => _`). The E010 diagnostic hint
uses the latter; tuple variants get positional `argN` bindings,
record variants reuse declared field names, `some/ok/err` get `x` /
`e`. Tests in `tests/exhaustiveness_hint_test.rs` (4 cases: tuple
variant, Option missing-arm, unit variants, catch-all for Int).

<a id="2026-04-20--2-unreachable--error"></a>

### 2026-04-20 — §2 unreachable → error

`find_unreachable_arms` (Maranget §3 usefulness) detects arms whose
pattern is fully covered by earlier arms. Each dead arm fires
`E011 unreachable match arm` on its body's span with a tightening
hint. Guarded arms are skipped for both shadowing directions — they
don't cover later arms and don't get shadowed themselves.

Opaque / Infinite column types (TypeVars, Int, Float, String) route
through the default-matrix branch rather than the constructor-
enumeration branch so generic fns like `map_option[A, B]` keep
compiling; the ctor-enum branch would falsely report `some(v)` dead
against an empty `[A]` ctor set.

<a id="2026-04-20--4-guard-totality-note"></a>

### 2026-04-20 — §4 guard totality note

E010 now appends "guarded arms (`pat if cond =>`) do NOT count
toward exhaustiveness — the guard can fail at runtime" when a
guarded arm is present in the match. The message clarifies why the
LLM's `X if cond => ...` arm didn't cover pattern X.

<a id="2026-04-20--3-nested-exhaustiveness-arm-template"></a>

### 2026-04-20 — §3 nested exhaustiveness (arm template)

Detection of nested uncovered patterns already worked — Maranget's
usefulness algorithm is recursive on sub-patterns. The gap was in
the `fmt_arm_template` formatter: it used `field_names` which always
produced positional `argN` names regardless of the inner ctor
structure. Now `fmt_arm_head` recurses through each arg:

- `Pat::Wild` → binding name (`argN` from a file-scope counter, or
  the record field name / `x` for Option/Result single fields).
- `Pat::Ctor(c, args)` → nested `c(...)` with a recursive call.

Result on `type Tree = | Leaf | Node(Tree, Tree)` with only `Leaf`
and `Node(Leaf, _) => ...` handled:

```
hint: add arms for Node(Node(_, _), Leaf), Node(Node(Leaf, Leaf), Node(_, _)), Node(Node(Node(_, _), Leaf), Node(_, _)):
  Node(Node(arg1, arg2), Leaf) => _
  Node(Node(Leaf, Leaf), Node(arg1, arg2)) => _
  Node(Node(Node(arg1, arg2), Leaf), Node(arg3, arg4)) => _
```

The `argN` counter resets per arm template so bindings don't
accidentally match across the paste; this is deliberate since each
arm is independent.

<a id="still-open"></a>

### Still open

- **§5 Variant Codec auto-derive** — extend the existing
  `derive Codec` machinery to produce encode / decode for variants
  by default. Requires touching the auto-derive codegen pipeline;
  separate arc-sized effort.

<a id="dependencies"></a>

## Dependencies

- None (localized type-checker and diagnostic changes suffice)

<a id="risk"></a>

## Risk

- Making unreachable arms errors may break existing code. Consider a phased migration through a separate flag (`--strict-unreachable`).
