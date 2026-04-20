<!-- description: Non-exhaustive match suggests missing arm code; unreachable arms become hard errors -->
<!-- done: 2026-04-20 -->
# Variant Exhaustiveness Refinement

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

## Motivation

パターンマッチの網羅性チェックは既に動作しているが、**エラーメッセージが「不足枝の名前」止まり** で、LLM が自動修復するには情報が足りない。また、到達不能な枝は現状警告レベルだが、**到達不能な枝は LLM の生成誤りの直接のサイン** であり、エラーで返すべき。

本項目は「書いた LLM がそのままコピペして通る」レベルまでエラーメッセージを磨き、修正生存率を直接押し上げることを狙う。

## Current State

```
Non-exhaustive match at line 14:3
  Missing: Node
```

LLM はこれを受け取っても、コンストラクタの arity やフィールド名を覚えていない可能性がある。結果として再度失敗する。

## Design

### 1. Paste-ready missing arms

不足枝をそのまま貼れる形で提示する：

```
Non-exhaustive match at line 14:3
  Missing arms:
    Node(left, right) => _
    Empty              => _
  Hint: add the arms above, or use `_ => todo()` to compile incrementally.
```

- 各コンストラクタを **フィールド名付き** で列挙
- 本体は `_` プレースホルダ（compile-time hole）
- 「とりあえず通したい」場合のために `_ => todo()` ヒントも併記

### 2. Unreachable arms become errors

```
match tree {
  Leaf(_) => ...,
  Leaf(0) => ...,  // ← 到達不能
  Node(_, _) => ...,
}
```

現状これは警告。これをエラーにする。理由：

- LLM は警告を読まない（stdout がノイズになる）
- 到達不能な枝は **高確率で生成誤り**（直前の枝の条件を誤認識している）
- 意図的な「将来用の枝」であれば削除すべき

### 3. Nested exhaustiveness

ネストした Variant に対する網羅性：

```
match tree {
  Node(Leaf(_), _) => ...,
  // Missing: Node(Node(_,_), _)
}
```

現状、ネストの外層だけ見て網羅判定しているなら、内層まで追跡する。

### 4. Guard-aware exhaustiveness

ガード付きの枝は **網羅性の計算から除外** する。`if x > 0` ガードがあれば、その枝は「確実にマッチする」とは言えない。ガードのある枝のあとには `_ =>` を必須化するエラーを出す：

```
Guard at line 17 is not total
  Hint: guarded arms never contribute to exhaustiveness. Add a `_ => ...` arm.
```

### 5. Auto-derived Codec for variants

（関連）既存の Codec 規約（`Type.decode` / `Type.encode`）を variant にも均質に適用する。手書きの serialize は LLM の事故源なので、**デフォルトで自動導出**、必要なら `#[codec(manual)]` で opt-out。

## Acceptance Criteria

- 非網羅エラーが不足枝のコード片をそのまま提示する
- 到達不能な枝がエラーになる
- ネストした Variant の網羅性が検出される
- ガード付き枝の後に `_ =>` が無ければエラー
- Variant の Codec がデフォルトで自動導出される
- 既存 snapshot テストが更新され、診断のゴールデンファイルが新形式に揃う

## Progress

### 2026-04-19 — §1 paste-ready missing arms

`check_exhaustiveness` now returns `Vec<MissingArm>` carrying both the
compact witness pattern (`Node(_, _, _)`) and a paste-ready arm
template (`Node(arg1, arg2, arg3) => _`). The E010 diagnostic hint
uses the latter; tuple variants get positional `argN` bindings,
record variants reuse declared field names, `some/ok/err` get `x` /
`e`. Tests in `tests/exhaustiveness_hint_test.rs` (4 cases: tuple
variant, Option missing-arm, unit variants, catch-all for Int).

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

### 2026-04-20 — §4 guard totality note

E010 now appends "guarded arms (`pat if cond =>`) do NOT count
toward exhaustiveness — the guard can fail at runtime" when a
guarded arm is present in the match. The message clarifies why the
LLM's `X if cond => ...` arm didn't cover pattern X.

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

### Still open

- **§5 Variant Codec auto-derive** — extend the existing
  `derive Codec` machinery to produce encode / decode for variants
  by default. Requires touching the auto-derive codegen pipeline;
  separate arc-sized effort.

## Dependencies

- なし（型チェッカと診断の局所的変更で完結）

## Risk

- 到達不能→エラー化は既存コードを壊す可能性。別フラグ（`--strict-unreachable`）を経て段階移行する選択肢を検討
