原文の説明 metadata：宣言された適合 + opt-in の `any P` 存在型 — Go の interface 値の使いやすさを、暗黙の充足や nil-interface の落とし穴なしに取り入れる。取り入れる価値のある Swift のアイデアはこの 1 つだけで、それ以外は取り入れない。

<a id="protocols-declared-conformance--opt-in-any-p"></a>

# Protocol：宣言された適合 + opt-in の `any P`

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](protocol-any-existentials.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](protocol-any-existentials.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](protocol-any-existentials.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/protocol-any-existentials.md)

> 原文注記：Active は設計記録済み・日程未定を意味し、存在型対応の実装済みを意味しない。protocol 文書監査は独立した前段階である。比較図や他言語比較は本提案の単純化・選好であり、独立に確認した業界全体や安全性の結論ではない。enum 優先表現、組込みとの関係、交差存在型、Self を返す処理は、本文の設計レビューでの未決境界を保つ。


> 2026 年の protocol 設計に唯一の勝者はいないが、定まった判断はある。この roadmap は機能を 1 つ（`any P` 存在型）だけ取り入れ、残りは意図的に退ける。再議論を避けるため理由を記録する。

状態：**Active** — 設計は記録済み、日程は未定。前段階（文書の乖離）は直ちに行う。

<a id="where-almide-stands-today"></a>

## 現在の Almide の立ち位置

適合は**宣言的かつ名目的**（`type GreetAction: Action`）であり、規約に従うメソッド（`fn GreetAction.execute(...)`）で充足し、宣言時に検査する。8 つの組込み protocol が `crates/almide-frontend/src/canonicalize/protocols.rs` に登録されている：

| protocol | signature |
|---|---|
| `Eq` | `fn eq(a: Self, b: Self) -> Bool` |
| `Repr` | `fn repr(v: Self) -> String` |
| `Ord` | `fn cmp(a: Self, b: Self) -> Int` |
| `Hash` | `fn hash(v: Self) -> Int` |
| `Codec` | `fn encode(v: Self) -> Value`, `fn decode(v: Value) -> Result[Self, String]` |
| `Encode` / `Decode` | Codec の一方向ずつの半分 |
| `Numeric` | generics の numeric-bound |

**前段階：文書の棚卸しを終える。** CHEATSHEET はすでに serialization 節で `Encode` と `Decode` を説明しており、まったく文書化されていないという主張は古い。主要 protocol 表と `Numeric` を登録表と照合して監査し、その後 drift gate を加える（llms.txt の先例）。この文書作業は `any P` 存在型の実装とは別である。

<a id="the-2026-landscape-in-one-map"></a>

## 2026 年の状況を 1 枚の図にする

```text
                    conformance declaration
             implicit (structural)   explicit (declared)
           ┌───────────────────────┬───────────────────────┐
 dynamic   │  Go                   │  Almide + `any P`     │
 (existen- │  (interface values    │  ← THIS ROADMAP       │
  tials)   │   are the default)    │                       │
           ├───────────────────────┼───────────────────────┤
 static    │  TypeScript           │  Rust / Swift /       │
 (mono-    │  (structural types)   │  Almide today         │
  morphic) │                       │                       │
           └───────────────────────┴───────────────────────┘
```

図の説明：横軸は適合宣言で、左が暗黙（構造的）、右が明示（宣言的）。上段は動的（存在型）で、左に interface 値が既定である Go、右にこの roadmap の Almide と any P がある。下段は静的（単相）で、左に構造型の TypeScript、右に Rust / Swift / 現在の Almide がある。

この提案が採用する設計上の選好（業界全体に普遍的な結論ではない）：

- **既定は static dispatch、存在型は明示する**（Swift の `some`/`any`、Rust の `impl`/`dyn`）。この提案は明示的区別を選ぶのであって、業界全体の収束を立証するものではない。
- **Coherence は譲れない**（Scala 2 の implicits が教訓となる）。Almide の型宣言箇所での適合は構成上 coherent である。型の適合は 1 箇所に置かれるので、「どの instance が勝つか」は生じない。
- **Deriving が決定的な用途になる**（Rust の serde は ecosystem を形作った）。Almide はすでにこの方向に賭けている（Eq/Repr/Ord/Hash/Codec の auto-derive）。

<a id="what-we-take-and-from-whom"></a>

## 何を、どこから取り入れるか

**Go から — interface 値の使いやすさを opt-in で。** 動機となる用途は存在型である。異種の handler list、plugin table、「書込み可能な何か」。LLM は Python/TS/Go からこの事前知識を持ち込み、繰り返しこう書く：

```text
let handlers: List[any Action] = [GreetAction { .. }, LogAction { .. }]
for h in handlers { h.execute(ctx)! }
```

今の Almide ではこれを表現できない（list は単相）。LLM が書き、失敗し、MSR が下がる。`any P` は 1 keyword でこの穴を埋める。

**Swift から — `any` という綴りと、その明確さだけ。** ほかは何も取り入れない。

**Rust から — 新しいものはない**（deriving への賭けと coherence はすでに持つ）。

<a id="what-we-explicitly-reject-and-why"></a>

## 明示的に退けるものと理由

| 機能 | 退ける理由 |
|---|---|
| **暗黙の構造的充足**（Go） | メソッド名がたまたま一致したために型が protocol を満たす偶然の適合は、LLM が書く code に直結する静かな驚きの種類である。適合は宣言された意図のままにする。 |
| **Go の nil-interface semantics** | 終わらない Go の #1 FAQ（「interface 内の nil は nil ではない」）。存在型を opt-in にし、Almide には null がないため、この落とし穴の表面は存在しない。 |
| **遡及的適合**（Swift） | 2 つの module が同じ外部型を別々に適合させられる。Swift 6 自身も今は警告する coherence の穴（`@retroactive`）である。Almide の型の場所で宣言する規則はこれを構造的に不可能にする。そのまま保つ。 |
| **関連型**（Swift/Rust） | エラーメッセージが人間にも読みにくく、LLM にはさらに難しい型レベルのプログラミング。具体的な stdlib の必要が生まれるまで延期する。現在の surface は generics + concrete type でカバーできる。 |
| **条件付き適合** | 必要なところでは組込みがすでに構造的に動く（container の Eq/Hash）。一般化で得るのは能力ではなく複雑さである。 |
| **名前付き instance / HKT** | Almide の使命の範囲外。複雑さは MSR を損なう。 |

<a id="design-sketch-any-p"></a>

## 設計の概略：`any P`

- **型**：`any Action` は型を置ける場所ならどこでも使える第一級の型（とくに `List[any Action]`、record field、param、return）。
- **導入**：宣言された適合の境界で暗黙に coercion する。`any Action` が期待されるところへ `GreetAction` を代入・渡すと包む。宣言された適合だけが対象（構造的な潜り込みはない）。
- **除去**：`any Action` 上で protocol メソッドを呼ぶと、適合型の規約メソッドに動的 dispatch する。**v1 に downcast はない**（`match v { as GreetAction g => .. }` はない）。実際のプログラムが必要としたときだけ追加する。downcast は存在型設計が難点を増やす場所である。
- **Witness の表現**：
  - *native（Rust emit）*：プログラム内の適合型を列挙する生成 enum（コンパイルごとに閉じた世界。Almide はプログラム全体を link するので、enum dispatch を使え、単相かつ `match` 網羅的）または `Box<dyn>` 形式の fn pointer record。**Enum 優先**。Eq/Repr を derive 可能に保ち、object-safety 規則を完全に避けられる。
  - *wasm*：`[type_tag: i32, value_ptr: i32]` の組 + 既存 closure function-table 機構を用いる protocol ごとの method table（先例：`call_indirect` dispatch、#385 の型ごとの `__repr_<T>` 関数）。
  - *interpreter*：tag 付き `Value::Any { type_name, inner }`。第 3 の judge も同じ dispatch を実装しなければならない（3-way fixture が必要）。
- **組込みとの関係**（設計レビューで決定し、contract-ledger 項目が必要）：
  - `${any_value}` 補間 → 内側の型の literal repr に `Repr` 方式で dispatch（C-008..C-010 と整合）。
  - `any P` 上の `==` → `P` が `Eq` を要求するときだけ。それ以外は hint 付き compile error。同じ gate を `Ord`/`Hash` に適用する（したがって set/map-key の `any P` は `P: Hash + Eq` でなければ拒否）。
- **エラー品質**：非適合型を `any P` へ代入すると、不足する適合と不足するメソッドの両方を挙げる（E017 診断基準）。

<a id="staging"></a>

## 段階分け

1. **前段階（今）**：Encode/Decode/Numeric の文書同期 + protocol 表の drift gate。
2. **Phase 1**：checker（`any P` 型、coercion 規則、method-call typing、==/hash の組込み protocol gate）+ native enum-witness emit。初日から fixture を用意する。
3. **Phase 2**：wasm emit（tag+ptr の組、method table）。`// @contract:` header を持つ cross-target fixture、dispatch と repr semantics の新しい contract 項目。
4. **Phase 3**：interpreter 対応（3-way）、fuzzer generator に `any P` program（異種 list の形）を出させる、CHEATSHEET + specs 更新。

各 phase は既存 gate の下で入れる（cross-target byte-gate、contract ledger、新 runtime routine 用 oracle registry、3-way harness）。

<a id="open-questions-design-review-before-phase-1"></a>

## 未決事項（Phase 1 前に設計レビュー）

- native の closed-world enum witness 対 open fn-pointer witness。enum の方が単純で derive 可能だが、適合型を追加するたび witness を再 compile する。whole-program compile には問題ない。separate compilation が実現したら再検討する。
- `any P1 + P2`（交差存在型）：延期するか（Go にはないが成り立ち、Swift には `any P & Q` がある）。syntax を予約して延期する方向。
- `Self` を返す protocol メソッド：`any` の裏では表現できない（古典的 object-safety 問題）。Rust の object safety に似て、より親切な hint を付け coercion site で拒否する。

<a id="related"></a>

## 関連

- [llm-first-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md) — この設計を判断する MSR の観点
- [cross-target-completeness.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/cross-target-completeness.md) — 各 phase が通過すべき gate
- `docs/specs/type-system.md` — 本提案が拡張する protocol system 仕様
