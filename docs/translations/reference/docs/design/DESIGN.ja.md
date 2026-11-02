<a id="design-philosophy"></a>

# 設計思想

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](DESIGN.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](DESIGN.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](DESIGN.zh-CN.md)



> 原文：[docs/design/DESIGN.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/DESIGN.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。読者向けの非規範的なスナップショット訳であり、自動同期されない。原文の時点・適用範囲・主張を維持したもので、翻訳時に独立した実測や実装検証は行っていない。 原文の MSR 規則・初回成功を唯一の競争軸とする記述は、同じ固定版の CLAUDE.md と demo/make-verify/README.md にある MSR の天井効果・指標置換の説明と整合しない歴史的記述として残す。現行の実測結果や適用済みの採用規則と解釈しない。他言語比較や「排除」「常に」などの効果も原文の設計主張であり、本訳が検証した保証ではない。

Almide は**思考トークンの最小化**を目指して最適化する。LLM が構文、意味論、修復戦略、欠けた抽象化について分岐を考える必要が少ないほど、コード生成は速く、安く、信頼性が高くなる。これは生成、補完、修復中の分岐を減らしつつ、AI が不足した抽象化をその場しのぎで補わずに済む適切な道具を提供することを意味する。

<a id="surface-ambiguity-removed"></a>

## 表層の曖昧さの除去

| 曖昧さの源 | 他の言語 | Almide | トークン分岐への影響 |
|---|---|---|---|
| Null 処理 | `null`、`nil`、`None`、`undefined` | `Option[T]` のみ | null 検査のハルシネーションを排除 |
| エラー処理 | `throw`、`try/catch`、`panic`、エラーコード | `Result[T, E]` のみ | エラー経路が常に型に見える |
| ジェネリクス | `<T>`（`<` `>` と曖昧） | `[T]` | 比較とのパーサの曖昧さがない |
| ループ | `while`、`for`、`loop`、`forEach`、再帰 | コレクション反復は `for x in xs { }`、条件ベースのループは `while cond { ... }` | 各形式の目的が一つ |
| 早期脱出 | `return`、`break`、`continue`、`throw` | 関数の結果は最後の式。構造化脱出には `guard ... else`、ループ内では `break`/`continue` | early-return の混乱がない |
| ラムダ | `=>`、`->`、`lambda`、`fn`、`\x ->`、ブロック | `(x) => expr` のみ | 一つの構文、代替なし |
| 文の終端 | `;`、省略可能な `;`、ASI 規則 | 改行区切り | 挿入の曖昧さがない |
| 条件分岐 | `else` を省略できる `if`、三項 `?:` | `if/then/else`。省略された `else` は Unit を返す | dangling-else がない |
| 副作用 | どこでも暗黙的 | `effect fn` が effect 呼び出しを示す。output/abort 組み込みは通常の `fn` でも許容 | 各地点の呼び出し可能集合を制限 |
| 演算子の意味 | 独自演算子構文、暗黙変換 | 固定の演算子語彙。宣言された protocol 適合により requirement method へ演算子を dispatch できる。暗黙変換なし | 解決はオペランド型と protocol 要件に従う |
| 型変換 | 暗黙の拡張、型強制 | 明示のみ | 隠れた型変更がない |

<a id="semantic-ambiguity-removed"></a>

## 意味上の曖昧さの除去

| 曖昧さの源 | Almide の対応 | LLM にとっての意味 |
|---|---|---|
| 名前解決 | core モジュール（`int`、`string`、`list`、`map`、`math`、`datetime` など）は自動 import。effect を持つ platform モジュール（`fs`、`env`、`io`、`json`、`random` など）は明示的 `import` が必要 | LLM は使える名前を推測しない。core 操作は常に使える |
| 型推論 | ローカルのみ。関数シグネチャに注釈が必要 | 離れた定義をまたぐ推論なし |
| オーバーロード | なし。名前は ad-hoc なオーバーロード解決に参加しない | dispatch の曖昧さなし |
| 暗黙変換 | なし。`int.to_string(n)` と書き、自動型強制はしない | すべての変換がソースに見える |
| protocol 探索 | 型が protocol 適合を宣言し、convention method（`fn Type.method`）が実装する。グローバルな暗黙 instance 解決なし | グローバル instance 探索なし |
| メソッド解決 | 正準の呼び出しはモジュール修飾付き。レシーバを考慮する UFCS は checker が解決 | 明示的なレシーバ/スコープ規則に従う |
| 宣言順序 | 関数は互いを自由に参照できる | 前方宣言の混乱なし |
| import の形式 | `import module` または `import self as alias`。目的ごとに一形式。alias は self-import とサブモジュールのみ。core モジュールは自動 import | 二つの import 形式、`from` も `*` もなし |

<a id="the-effect-system-as-generation-space-reducer"></a>

## 生成空間を減らす `effect` システム

`effect fn` の第一の目的は安全機能ではなく、**コード生成の探索空間を減らすこと**である。

- 通常の `fn` は直接の effect 呼び出しを拒否する。output/abort 組み込みと透過的なコールバック引数には文書化された例外がある
- `effect fn` は I/O 境界を明示する → LLM は副作用が許される場所を正確に把握できる
- effect の不一致はコンパイル時に検出する → 誤った呼び出しは実行前に拒否される
- 関数本体を読まなくても、シグネチャだけで各地点で何を呼べるか LLM に伝わる

つまり LLM は現在の関数のシグネチャと import だけを見てコードを生成でき、グローバル解析は不要になる。

<a id="concurrency-fan--boring-on-purpose"></a>

## 並行処理：`fan` — 意図的に退屈にする

Almide は `async`/`await` を露出せず、明示的な fork/join を使う。通常の `fan` はすべての sibling を join し、実時間で最初に起きた失敗ではなく、ソース/リスト順で最初のエラーを報告する。

よく使う二つの形式：

- **`fan { a(), b() }`** — 式を一つのスコープ付き単位として実行し、すべてを待ち、宣言順のタプルで結果を返す。実行基盤はコンパイラと host が選ぶ
- **`fan.map(xs, f)`** — すべての要素を実行し、リスト順で結果を集める。並列実行されるかはコールバックとターゲットに依存する

規則は最小限である：

- `fan { }` は `effect fn` 内でのみ有効。pure 関数は fork できない
- arm が `Err` を返した場合、すべての sibling を join した後で arm 順の最初の `Err` をブロックが返す
- `fan` 内で `var` をキャプチャしない。外側スコープの `let` 束縛だけを読める（データ競合を防ぐ）
- 非構造化 `spawn` はない。すべての並行処理にスコープがある

通常の `fan` は sibling をキャンセルしない。budget/deadline 形式には別の協調的打ち切り規則がある。[SPEC §13](../../../../SPEC.md#13-concurrency-fan) を参照。

<a id="ufcs-why-two-forms-is-acceptable"></a>

## UFCS：二つの形式が許容できる理由

UFCS 解決が `f` を選んだとき、`x.f(y)` は `x` を第一引数として渡す。これは構文解析時の無条件な書き換えではない。

- **正準の解決はモジュール修飾の関数形式**：`module.fn(args)`。モジュール接頭辞が解決を明確にする
- **自動 import はモジュールを利用可能にする**のであり、すべての関数名をグローバルに利用可能にはしない。`string.len(s)` または `s.len()` のようなレシーバ構文を使う
- **メソッド形式はチェーン用だけの糖衣構文**：`x.f(y).g(z)` は左から右へ読める
- checker がレシーバ型とスコープを使い、convention method、ローカル/import 済み関数、組み込みモジュールのメソッドを解決する（`calls_ufcs.rs`）
- 将来の formatter が正準形式へ正規化し、スタイルのずれを排除する

<a id="iteration-forin--while"></a>

## 反復：`for...in` + `while`

二つのループ構文にはそれぞれ明確な目的がある：

- **`for x in xs { ... }`** — コレクションを反復する。リストや map のキーに自然な選択。effect に対応する（ループ本体内で I/O を行ってよい）。
- **`while cond { ... }`** — 条件ベースのループ。条件が真の間実行する。単純なループ条件がある場合の素直な選択。

<a id="compiler-diagnostics-single-likely-fix"></a>

## コンパイラ診断：有力な修正を一つにする

Almide の診断は、**各エラーがちょうど一つの修復を指す**ように設計されている。これは LLM の修正ループに重要である。

- 拒否する構文（前置 `!x`、`return`、`class`、`null`）には、正確な Almide の相当物を示すヒントが付く
- 各解析位置で期待するトークンを小さな列挙可能集合に保つ
- parser の回復は推測しない。正確な位置と期待を示して早く失敗する
- `_` の穴と `todo()` は未完成コードを示す。未完成のプログラムがコンパイルまたは実行に成功する保証ではない

```
Prefix '!x' is not boolean negation in Almide
  Hint: Use 'not x' for boolean negation, not '!x'.

'return' is not valid in Almide at line 12:5
  Hint: Use the last expression as the return value, or 'guard ... else' for early exit.
```

<a id="stdlib-naming-conventions"></a>

## 標準ライブラリの命名規則

標準ライブラリは LLM の推測を減らすため、厳密な命名規則に従う。

| 規約 | 規則 | 例 |
|---|---|---|
| モジュール接頭辞 | 正準形式は `module.function()`。UFCS 解決後にレシーバ構文で修飾付き呼び出しを置き換えてよい | `string.len(s)`、`list.get(xs, i)`、`map.get(m, k)`（core モジュールは自動 import） |
| 述語接頭辞 | 真偽値を返す関数は `is_` | `string.is_empty(s)`、`list.is_empty(xs)`、`map.contains(m, key)` |
| 戻り型の整合性 | 失敗し得る探索は `Option`、失敗し得る I/O は `Result`、失敗しない pure 変換は通常の値を返す | `list.get() -> Option[T]`、`fs.read_text() -> Result[String, String]`（effect fn） |
| 同義名なし | 一操作に一名、alias なし | `length`/`size`/`count` ではなく `len` |
| 対称な対 | 逆操作に対応する名前 | `read_text`/`write_text`、`split`/`join`、`to_string`/`to_int` |
| 名前の意味のずれなし | モジュール間で意味が一致する場合だけ同じ操作名を再利用 | `string.len` と `list.len` は両方「要素を数える」であり、同じ名前に異なる操作を隠さない |

<a id="what-almide-sacrifices"></a>

## Almide が犠牲にするもの

LLM 生成の信頼性を高めるため、意図的に手放したトレードオフである。

| 犠牲にするもの | 理由 |
|---|---|
| 素の表現力 | 各概念の慣用的な書き方は一つ。Almide は適切な抽象化（例：`map`、`for...in`）を提供するが、同じことを達成する複数の方法は提供しない。 |
| 独自演算子 | 独自の演算子構文なし。既存演算子は宣言された protocol（`Eq`、`Numeric`、順序要件）を介して dispatch できる。暗黙変換なし。 |
| メタプログラミング | macro、reflection、コード生成なし。言語の表層は固定。 |
| ad-hoc 多相性 | `protocol` が必要な convention method を定義する。型は適合を明示宣言する（`type Dog: Eq, Serializable`）。フラットな convention method（`fn Dog.serialize(...)`）で実装し、`impl` ブロックなし。組み込み convention（Eq、Repr、Ord、Hash、Codec）は protocol。ジェネリック境界は `fn f[T: Protocol](x: T)`。単相化し、動的 dispatch なし。暗黙 instance 解決も orphan rule もない。 |
| 名前付き/デフォルト引数 | デフォルト引数をサポート（`fn f(x: Int, y: Int = 0)`）。明確化のための名前付き引数（`f(x: 1, y: 2)`）。可変長引数なし。 |
| 複数の return スタイル | `return` キーワードなし。最後の式が常に値。例外なし。 |
| 多様な糖衣構文 | 小さな正準表層に、`T?`、`T!E`、UFCS、レコードフィールド省略形など、文書化した糖衣を持つ。具体的な文法を設計スローガンより優先する。 |
| DSL 能力 | 演算子定義も独自構文もない。Almide コードは常に Almide らしく見える。 |

これは未実装機能ではなく、**生成空間を絞る意図的な制約**である。目的は最小主義そのものではなく、各抽象化に一つの明確な道筋を確保することにある。

<a id="the-msr-rule-how-features-are-added--rejected"></a>

## MSR 規則（機能を追加・拒否する方法）

> **設計規則**：人間には美しくても LLM の罠になる構文は採用しない。

すべての言語変更を **MSR delta** で評価する。[almide-dojo](https://github.com/almide/almide-dojo) で `llama-3.3-70b`、`llama-3.1-8b`、利用可能なら `Claude Sonnet 4.6` に対し、`T=0`、3 回の実行で計測する再試行成功率の変化である。

次のいずれかに該当する変更は**拒否**する：
- いずれかのモデルで MSR 中央値を 3 パーセントポイント以上下げる、または
- すでに正準形式があるものに第二の正準形式を導入する（LLM の学習データを分断する）、または
- 生成の予測可能性を犠牲にして人間向けの美観を改善する。

次のいずれかに該当する変更は**受け入れる**：
- MSR を計測可能な形で上げる、**または**
- 診断付き再試行では修復できないエラー群を排除する（天井効果）。

`v0.14.5` 以降のリリースノートには **MSR delta 表**を含め、各リリースが自らの影響に責任を持つ。`docs/roadmap/active/` の新機能提案は、対処する失敗パターンと期待する MSR 変化を引用しなければならない。

<a id="beauty-axis-vs-msr-axis"></a>

## 美しさの軸と MSR の軸

Almide が競わないもの：
- 純粋 FP の美観（Elm、Haskell はここでより強い。その美しさのために LLM の書きやすさを犠牲にする）。
- 依存型 / 線形型 / 第一級の型としての effect（研究言語が勝るが、書きやすさが急落する代償を伴う）。
- DSL の表現力、macro、メタプログラミング。

Almide が競うのはただ一つ、**LLM が最初の試行で正しいコードを書くこと**である。上記のすべてのトレードオフと今後のすべての判断は、その軸を基準に選ぶ。
