# ADR-0005: Value-level operators are desugarings of named stdlib functions

- **Status**: Accepted
- **Date**: 2026-08-05
- **決定範囲**: 値レベル演算子(`??` / `?` / `?.`)と stdlib コンビネータの関係 —
  重複の解消方針、糖衣定義、余剰綴りの削除、no-op の扱い
- **関連**: [ADR-0004](./0004-error-branchability-doctrine.md)(「同じことを書く 2 つ目の
  綴りを増やさない」原理 — result.context 却下)、REJECTED_PATTERNS.md(`??` 記載の矛盾)、
  BUG 台帳(パイプ × `??` miscompile)
- **経緯**: 2026-08-05、エラー表面 matrix で発覚した 4 件 — `??` と
  `unwrap_or`/`unwrap_or_else` の三重綴り、`??` の「却下と記載されたまま実装済み」矛盾、
  `?.` の実装済み・文書ゼロ状態、Option への `?` の黙った恒等 — を一問一具体物の○×で裁定。

## Context

fallback を書く綴りが 3 つ、しかも定義関係がないまま並立していた:

```almide
port_opt ?? 8080                              // 演算子(lazy)
option.unwrap_or_else(port_opt, () => 8080)   // 関数(lazy)— 意味は ?? と同一
option.unwrap_or(port_opt, 8080)              // 関数(eager)— default が常に評価される
```

さらに: REJECTED_PATTERNS は `??` を「却下」と記載したまま(実装・使用済み)、
`?.` は動作し専用診断まで持つのに文書ゼロ(診断コードも未採番)、`o?`(Option への
`?`)は黙って恒等写像。

先例の対立軸:

- **Rust / Haskell**: 演算子は関数の糖衣・関数が実体(`x?` → `Try::branch`、
  `a + b` → `Add::add`、Haskell では `(+)` は渡せる値)
- **C# `??` / Kotlin `?:`**: 関数実体を持たない孤立演算子

## Decision

**値レベル演算子は、名前付き stdlib 関数への脱糖として定義される。関数が実体、
演算子は表面であり、両者は「2 つの綴り」ではなく定義関係にある。定義関係のない
余剰綴りは削除する。制御フロー演算子(`!`・auto-`?`)は早期 return であり関数実体を
持てないため、本ドクトリンの明示的な境界外。**

### D1. 糖衣定義の表(contract テストで等価を固定)

```almide
x ?? d      ≡  option.unwrap_or_else(x, () => d)          // Option operand
r ?? d      ≡  result.unwrap_or_else(r, (_) => d)          // Result operand(E 破棄は (_))
a ?? b ?? c ≡  右結合でネスト
r?          ≡  result.to_option(r)                          // Result → Option
o?.x        ≡  option.map(o, (v) => v.x)                    // Option のフィールド安全アクセス
```

各行に等価性の contract fixture(native ⇄ wasm、演算子形と関数形の出力一致)を置く。

### D2. `?.` の公式化

実装済み・実測済み(`some(P{x:5})?.x ?? 0` → 5、`none` → 0)の `?.` を仕様に載せる。
Option 専用 — Result への誤用は既存の専用診断(現在コードレス)に **E-code を採番**する。
Result からは `(r?)?.x` と合成できる(D1 の 2 定義の合成そのもの)。

### D3. Option への no-op `?` は警告

`o?`(operand が既に Option)は定義域外の恒等写像。黙認をやめ警告にする:

```
warning[W0xx]: この ? は何もしません(operand は既に Option です)
  = help: ? は Result → Option の変換です。取り除いてください
```

リファクタ(戻り型 Result → Option)時にコンパイルは生き残り(MSR)、
死んだ演算子は掃除対象として可視化される。

### D4. eager `unwrap_or` は削除(option / result 両方)

定義関係を持たない 3 つ目の綴り。deprecated → 削除(E040 窓の前例に従い警告期間を
経る)。削除後の E002 には自己修復 hint を必須で付ける:

```
error[E002]: undefined function 'option.unwrap_or'
  hint: fallback は ?? を使ってください: port_opt ?? 8080
        関数形が必要なら option.unwrap_or_else
```

### D5. REJECTED_PATTERNS の `??` 記載を訂正

「却下」→「`unwrap_or_else` の糖衣として採用(ADR-0005)」。
flagged 矛盾はこれで閉じる。

### D6. パイプ × `??` miscompile の修正方針

BUG 台帳のパイプ内 `??` silent miscompile(native≠wasm・誤値・exit 0)は、
D1 の脱糖をパイプ変換より**前段**で行う実装に寄せることで構造的に消す。
修正 PR はこの方針に従う。

## Rationale

- **「実体なし演算子」への違和感が裁定の起点**: 関数を消して演算子だけ残す案
  (C# 型)は、演算子を higher-order / パイプ位置で使えない孤児にする。
  Rust/Haskell 型(定義関係)なら ADR-0004 の「綴りは一通り」原理と両立する —
  糖衣は 2 つ目の綴りではなく、同じ 1 つの定義の表面である。
- **`??` の lazy 性が `unwrap_or_else` と正確に一致**しており、定義は後付けの
  こじつけではない。E 破棄の `(_)` は ADR-0004 D3-(b) で決めた意図表明の綴りが
  そのまま定義に現れる。
- **`?.` は family 中で事前分布最強**(JS/TS/Swift/Kotlin/C# 全部が持つ)。
  実装済みのものを文書ゼロで放置する理由がない。
- **eager 版の存在理由は「短い」だけ**: default 側に作用・コストがある場合、
  eager は劣位(常に評価)。hint 付き E002 で書き損じは自己修復する。

## Alternatives — 検討して却下した案

1. **関数を消して演算子に一本化**(C# / Kotlin 型): `??` が孤立演算子になり、
   実体関数を持たないことへの違和感(裁定時の指摘)、higher-order 形の喪失。**却下**。
2. **3 綴り共存を明記して固定**: 定義関係のない並立は ADR-0004 の原理に反する。**却下**。
3. **`?.` を廃止**: 事前分布最強 + 実装済みで、削除は LLM の反射に毎回エラーを
   返すことになる。**却下**。
4. **`o?` の黙認を仕様化**: no-op の堆積を許す。警告なら MSR(生存)と掃除可視化を
   両立できる。**却下**。
5. **現状維持**(矛盾記載・文書ゼロ・三重綴りのまま): 論外として**却下**。

## Consequences

- fallback の綴りは「`??`(表面)+ `unwrap_or_else`(実体)」の定義ペアに収束。
  `unwrap_or` は警告期間を経て消える(破壊的変更 — リリースノートと hint で移行)
- 仕様に糖衣定義表(D1)が載り、等価性が contract で固定される —
  演算子と関数の意味が乖離する余地が機械的に消える
- `?.` の文書化・E-code 採番、`o?` 警告、REJECTED_PATTERNS 訂正が SPEC / 診断
  バッチに乗る
- パイプ × `??` バグ修正の設計方針が確定する(D6)

## Falsifier

1. **脱糖先行の実装が現行 `??` の観測挙動と一致しないケースが見つかった場合**
   (優先順位・評価順の相互作用)— D1 を「実装脱糖」から「文書上の等価 + contract」に
   後退させ、差分を仕様に明記する。
2. **`unwrap_or` 削除後、hint があっても dojo の MSR が有意に劣化した場合** —
   D4 を撤回し deprecated エイリアスとして復活させる。
3. **`?.` のネスト・メソッド呼び出し形が D1 の option.map 定義と食い違うと実測された
   場合** — 定義を修正するか適用範囲(フィールドアクセスのみ)を仕様に明記する。

## References

- Rust — [The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)、
  [`Add`](https://doc.rust-lang.org/std/ops/trait.Add.html)(演算子 = トレイト関数の糖衣)
- C# — [?? operator](https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/operators/null-coalescing-operator)、
  Kotlin — [Elvis operator](https://kotlinlang.org/docs/null-safety.html#elvis-operator)(実体なし演算子の対比)
- TypeScript — [Optional chaining](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-3-7.html#optional-chaining)(`?.` の事前分布)
- 内部: ADR-0004(綴り一本化の原理・`(_)` の意図表明)、2026-08-05 matrix
  (`coalesce/*`・`qop/optional-chaining/*` セル、パイプ × `??` バグ)、
  REJECTED_PATTERNS.md(D5 の訂正対象)
