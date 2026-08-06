# ADR-0006: One-bit fallibility polymorphism for HOFs; the try_* family is dissolved

- **Status**: Accepted
- **Date**: 2026-08-05
- **決定範囲**: 高階関数(HOF)の callback が可謬なとき、可謬性をどう表面化するか —
  専用 `try_*` 関数(現状)か、可謬性多相(callback の `!` が HOF に流れる)か
- **関連**: [ADR-0002](./0002-fallibility-effect-orthogonal.md)(`-> T!`・E=String —
  本 ADR はその HOF への系)、[ADR-0004](./0004-error-branchability-doctrine.md)
  (String 終着 — D4 の意図的省略の根拠)、[ADR-0005](./0005-operators-desugar-to-stdlib.md)
  (綴り最小化)、#1103(Phase 1)、#1055(capability 軸 — 本 ADR はその部分集合を先行切り出し)
- **経緯**: 2026-08-05、「`try_` 接頭辞が嫌」という違和感の深掘りから。調査の結果、
  try_* family は「rethrows 相当の言語機能の不在を関数で埋めた仮設物」であることが
  判明し、他言語比較と使用量実測を経て解体を決定。

## Context

v0.53.6 で出荷した `list.try_*` family(`try_map` / `try_filter` / `try_flat_map` /
`try_filter_map` / `try_fold` / `try_find` / `try_each` の 7 関数)は、
「callback が Result、最初の err で打ち切り、E 不変」の単相専用関数群である。

問題は 3 つ:

1. **`try` は almide の語彙に存在しない借り物**で、貸し主間で意味が割れている
   (Rust `try_fold` = 打ち切り / Swift・Zig `try` = 伝搬 = almide の `!`)。
   Swift/Zig を浴びた LLM は `try_map` を誤読しうる
2. **表面が戦略 × HOF で積算的に膨らむ**: HOF を足すたび「try_ 姉妹が要るか」が
   問われる
3. almide のリストは eager なので、Rust 流の遅延合成
   (`map(f).collect::<Result<_,_>>()` — collect が first-err 打ち切り)は**原理的に
   使えない**。try_map はこの制約下の融合関数として生まれた

### 他言語比較(一次情報)

| 言語 | 可謬 map | 仕組み |
|---|---|---|
| Swift 6 (SE-0413) | `try xs.map(f)` — map は 1 つ | **可謬性多相**: `map<U, E: Error>(body: (Element) throws(E) -> U) throws(E) -> [U]`。総 callback は E=Never に推論され map も総に |
| Rust | `map(f).collect::<Result<_,_>>()` | 遅延イテレータ + FromIterator(「最初の Err を返す」— std docs 明記)。closure が `?` 多相になれないため `try_fold` 系も併存 |
| Zig | ループ + `try f(x)` | エラー集合が呼び出しを透過して推論 |
| Haskell / Scala | `traverse` | 戦略が型に載る(Either=打ち切り / Validation=全収集) |
| Gleam | `list.try_map` | **専用単相関数 — 型クラスも効果もない制約下の妥協**(almide 現状と同型) |
| OCaml 5 / Koka | `List.map` そのまま | 効果システムが透過 |

P1(名前に戦略)を選んだのは Gleam だけで、それは道具の不在による妥協。
almide は ADR-0002 で Swift の throws モデル(道具)を作ると決めている。

### 使用量実測(2026-08-05)

family 自身の定義・テスト・docs を除く本物の使用箇所は**リポジトリ全体で 1 行**:
`tools/almide-gates/src/main.almd:46`(effect fn 経由 = E=String)。
compiler の `.rs` に見える `try_fold` 群は Rust 自身の `Iterator::try_fold` で無関係。
E ジェネリックな(String 以外の)使用は**ゼロ**。

## Decision

**HOF の可謬性は callback の型から流れる(1 ビット可謬性多相)。`try_*` family は
即時凍結し、多相 HOF の着地と同時に deprecated、次 minor で削除する。**

### D1. 1 ビット可謬性多相

fn 型パラメータは `!` を持てる(`(A) -> B!`)。generic HOF は callback が可謬なとき
自身も可謬になる — 規則は 1 つ:「**callback が `!` なら HOF も `!`**」。
E は ADR-0002 D2 に従い String 固定(Swift の E ジェネリックより 1 段単純)。
打ち切りは伝搬の自然な帰結として得られる(専用実装不要):

```almide
nums  |> list.map((n) => n * 2)              // 総 callback → List[Int]
files |> list.map((f) => read_meta(f)!)!     // 可謬 callback → List[Meta]!(first-err 打ち切り)
```

### D2. try_* family は即時凍結

新しい兄弟を追加しない。既存の family gate
(`tests/list_try_family_gate_test.rs`)は「7 で固定」の番人に転用する。
CLAUDE.md の try_map 推奨イディオムは**多相 HOF 着地までは有効なまま**
(先に消すと var+for が復活するため — 削除順序が本質)。

### D3. 着地時に deprecated → 次 minor で削除

deprecation hint は機械的書き換えをそのまま出す:

```
list.try_map(xs, f)      →  list.map(xs, (x) => f(x)!)!
list.try_filter(xs, p)   →  list.filter(xs, (x) => p(x)!)!
list.try_fold(xs, z, f)  →  list.fold(xs, z, (a, x) => f(a, x)!)!
// find / each / flat_map / filter_map も同型
```

移行コストの実測値: 1 行。

### D4. E ジェネリックな traverse は意図的に非サポート

カスタム E での打ち切り走査は明示再帰(または境界での String 変換)で書く。
ADR-0004(String 終着・variant E は自作ドメイン内)と整合する意図的省略。
需要が実測されたら Falsifier 3 で再考。

## Rationale

- **ADR-0002 の系**: 可謬性を fn 属性の軸として立てた以上、fn **型**(パラメータ位置)
  にその軸が現れるのは軸の完結であり、新しい概念ではない
- **表面の増殖が止まる**: 戦略 × HOF の積算が消え、HOF は今後何を足しても 1 名
- **MSR**: 書き手は「どの try_ 姉妹があるか」を思い出す必要がなく、いつもの `map` を
  書いて `!` を置くだけ。可謬性の過不足は check が指摘し、hint で自己修復する
- **今が最安値**: 使用 1 行・出荷 1 リリース目。採用が広がるほど解体コストは増える

## Alternatives — 検討して却下した案

1. **改名だけ行う**(`traverse` / `map_try` 等): 将来 D1 で family ごと溶けるなら
   **二度改名**になる。最悪手として**却下**。
2. **try_* を恒久維持**(Gleam 型): 道具の不在による妥協を、道具を作ると決めた言語が
   抱え続ける理由がない。**却下**。
3. **full #1055(効果多相)を待って一括でやる**: 1 ビット可謬性多相は E 固定・
   bool 伝搬のみで分離可能に小さい。人質に取る理由がない。**却下**(先行切り出し)。
4. **遅延イテレータ導入で Rust 流合成**(P3): コレクションモデル全体の変更で
   影響半径が桁違い。**却下**。

## Consequences

- Phase 2 実装(#1108): fn 型の `!`、lambda の可謬性推論(`(x) => f(x)!` は
  `(A) -> B!`)、generic HOF の総/可謬 2 態モノモーフ化、native ⇄ wasm 等価
- 二重 `!` の綴り `((x) => f(x)!)!` が標準形になる — B トラック(auto-`?` 位置)の
  設計と噛むため、Phase 2 設計時に相互参照必須
- try_* の死は 3 段階(凍結 → deprecated → 削除)で、各段階がリリースノートに載る

## Falsifier

1. **1 ビット伝搬の推論が予測不能になる**(注釈の有無で挙動が変わる等、B トラック型の
   非正則が Phase 2 設計で解消できない)場合 — D1 を撤回し try_* を恒久化する。
2. **dojo A/B で `((x) => f(x)!)!` の綴りが try_map より MSR を有意に悪化させた**
   場合 — 解体を中止し、多相 HOF と try_* の共存(try_* = 糖衣、ADR-0005 型の
   定義関係)に切り替える。
3. **カスタム E の traverse 需要が実測された**(≥3 箇所の本物の使用)場合 —
   E ジェネリック多相(Swift の完全形)を新 ADR で検討する。

## References

- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
  (`map` の rethrows → `throws(E)` 一般化の before/after を引用取得済み)
- Rust — [std::result: Collecting into Result](https://doc.rust-lang.org/std/result/index.html)
  (「最初の Err を返す」を引用取得済み)、
  [Iterator::try_fold](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.try_fold)
- Zig — [Errors](https://ziglang.org/documentation/master/#Errors)
- Gleam — [gleam/list.try_map](https://hexdocs.pm/gleam_stdlib/gleam/list.html#try_map)
- 内部: ADR-0002 / 0004 / 0005、使用量実測(本文)、#1103(Phase 1)、
  `tests/list_try_family_gate_test.rs`(凍結の番人)
