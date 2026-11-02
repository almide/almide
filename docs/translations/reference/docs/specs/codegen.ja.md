> 最終更新: 2026-10-31

<a id="codegen-specification"></a>

# コード生成仕様

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](codegen.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](codegen.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](codegen.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/codegen.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットで、自動同期しない。原文を正とし、コード・型・コメントは原表記を保持する。実装・性能・所有権の説明と他言語への言及は原文の主張であり、翻訳に際して独立検証・実行した保証ではない。原文冒頭の「3 層」と図の Layer 1/2 表示の差は統一せず保持する。

Almide のコード生成（`crates/almide-codegen`）は、型付き IR を **Rust** ターゲット（および WGSL）のソース出力へ変換する 3 層アーキテクチャである。すべての意味論上の判断は、テキストを出力する前に IR 上で行う。

**WASM** ターゲットはこの crate を経由しない。`src/wasm_route.rs` の共通経路は `crates/almide-wasm` の構造的エミッターを使い、Wasm バイト列を直接生成する。廃止された WAT レンダラーはフォールバックではない。[../ARCHITECTURE.md](../../../../ARCHITECTURE.md) と [../wasm/WASM-OUTPUT.md](../../../../wasm/WASM-OUTPUT.md) を参照。

**ソース**: `crates/almide-codegen/src/lib.rs`

```
IrProgram (typed IR)
    |
Layer 1: Nanopass Pipeline (target-specific semantic rewrites)
    |
Layer 2: Emit (target-specific output)
    Rust  -> Template Renderer (TOML-driven) -> source code
    WGSL  -> emit_wgsl                       -> shader source
```

<a id="1-entry-point"></a>

## 1. エントリーポイント

**ソース**: `crates/almide-codegen/src/lib.rs`

唯一のエントリーポイントは `codegen(program, target) -> CodegenOutput` である（`codegen_with` は `CodegenOptions` を受け取る）。

```rust
pub enum CodegenOutput {
    Source(String),   // Rust / WGSL targets
    Binary(Vec<u8>),  // legacy variant; no longer produced
}
```

処理の流れ:

1. `target::configure(target)` が nanopass パイプラインとテンプレートセットを含む `TargetConfig` を構築する
2. パイプラインが適用可能なすべてのパスを実行し、IR をその場で変換する
3. ターゲット固有の出力:
   - **Rust**: walker がテンプレートを通じて IR を描画し、ランタイムの前置部分を付ける
   - **WGSL**: `emit_wgsl::emit()` がシェーダーソースを描画する
   - **Wasm**: `unreachable!` — この crate に到達する前に構造的な `almide-wasm` エミッターへ振り分ける

```almd
// Input: hello.almd
fn greet(name: String) -> String =
  "Hello, " + name + "!"

fn main() =
  println(greet("world"))
```

```rust
// Output: Rust target (simplified, preamble omitted)
pub fn greet(name: String) -> String {
    format!("{}{}", format!("{}{}", "Hello, ".to_string(), name), "!".to_string())
}

pub fn main() -> () {
    println!("{}", greet("world".to_string()))
}
```

<a id="2-target-enum"></a>

## 2. Target 列挙型

**ソース**: `crates/almide-codegen/src/pass.rs`

```rust
pub enum Target {
    Rust,
    Wasm,   // tombstone: codegen arm is unreachable! (v0 emitter retired, #782)
    Wgsl,
}
```

この crate で有効なターゲットは **Rust** と **WGSL** である。TypeScript コード生成は削除済み（2026-03-28）。現在の Wasm 経路は `src/wasm_route.rs` → `crates/almide-wasm` であり、廃止された MIR-to-WAT レンダラーではない。

各ターゲットは `TargetConfig` を返す `target::configure()` で構成する。

**ソース**: `crates/almide-codegen/src/target.rs`

```rust
pub struct TargetConfig {
    pub target: Target,
    pub pipeline: Pipeline,
    pub templates: TemplateSet,
}
```

<a id="3-nanopass-pipeline"></a>

## 3. Nanopass パイプライン

**ソース**: `crates/almide-codegen/src/pass.rs`, `crates/almide-codegen/src/target.rs`

各パスは `NanoPass` トレイトを実装する。

```rust
pub trait NanoPass: std::fmt::Debug {
    fn name(&self) -> &str;
    fn targets(&self) -> Option<Vec<Target>>;  // None = all targets
    fn depends_on(&self) -> Vec<&'static str>;
    fn run(&self, program: IrProgram, target: Target) -> PassResult;
}
```

各パスを組み合わせて `Pipeline` を構成する。パイプラインランナーは以下を行う。
- 現在のターゲットに関係しないパスをスキップする
- 宣言された依存関係を検証する（依存パスが未実行なら panic）
- **すべてのプロファイルの各パスの後**に IR の整合性と確立済みのすべての `Postcondition` を検証する。違反はコンパイラのバグであり、リリースを含めビルドを失敗させる。パスごとの走査は以前、1 ファイル約 1.2 秒という古いコスト主張に基づきデバッグ専用だった（`ALMIDE_VERIFY_IR` でリリースビルドを対象にできた）。spec/lang の 218 ファイルで計測すると 1 ファイル約 8 ms だったため、現在出荷するプロファイルは `cargo test` と同じ走査を行う。`ALMIDE_IR_FAULT=<pass>`（ハーネス用スイッチ）は指定したパスの後に違反を注入する。リリースバイナリの陰性対照である（`tests/ir_verify_every_profile_test.rs`）。

<a id="rust-pipeline-source-of-truth"></a>

### Rust パイプラインの正本

[`build_pipeline(Target::Rust)`](../../../../../crates/almide-codegen/src/target.rs) が依存関係と条件付きゲートを含むパス順序の正本である。これは実装パイプラインであり、固定された言語レベルの順序ではない。

段階には型／呼び出しの正規化、リージョンと所有権の解析、借用／クローン、effect と Result の適応、ストリーム融合、Rust 固有の lowering、ランタイム呼び出しの正規化、最終的なリンク／格納判断が含まれる。`RegionWindowPass`、`StreamFusionPass`、`OwnedSourceHoistPass` は現在の実装の一部であり、古い `AutoParallelPass` の表は廃止済みである。並列スケジューリングは、削除されたそのパスではなく、明示的な `fan` 形式に従う。

<a id="4-template-system"></a>

## 4. テンプレートシステム

**ソース**: `crates/almide-codegen/src/template.rs`, `crates/almide-codegen/templates/rust.toml`

テンプレートは構文だけを定義する。意味論上の判断はすべて nanopass の各パスが行う。

<a id="structure"></a>

### 構造

```rust
pub struct TemplateRule {
    pub template: String,           // "{placeholder}" holes
    pub when_type: Option<String>,  // Guard: match on expression type
    pub when_attr: Option<String>,  // Guard: match on TargetAttrs flag
}

pub struct TemplateEntry {
    pub rules: Vec<TemplateRule>,   // First matching rule wins
}

pub struct TemplateSet {
    pub target_name: String,
    pub entries: HashMap<String, TemplateEntry>,
}
```

<a id="toml-format"></a>

### TOML 形式

構成要素ごとに 1 規則:

```toml
[if_expr]
template = "if {cond} {{ {then} }} else {{ {else} }}"
```

ガード付きの複数規則（配列構文）:

```toml
[[concat_expr]]
when_type = "String"
template = "format!(\"{{}}{{}}\", {left}, {right})"

[[concat_expr]]
when_type = "List"
template = "AlmideConcat::concat({left}, {right})"
```

属性でガードされたバリアント:

```toml
[[call_expr]]
when_attr = "needs_try"
template = "{callee}({args})?"

[[call_expr]]
template = "{callee}({args})"
```

<a id="guard-system"></a>

### ガードシステム

**`when_type`**: IR 式の型カテゴリ（`"Int"`、`"Float"`、`"String"`、`"List"`、`"Option"`）に対してマッチする。`+`（連結か加算）、`**`（pow か powf）、unwrap_or のような型による振り分け操作に使う。

**`when_attr`**: nanopass が設定した `TargetAttrs` フラグにマッチする。用途:

| フラグ | 設定元 | 効果 |
|------|--------|--------|
| `needs_try` | ResultPropagationPass | 呼び出しに `?` を付加 |
| `needs_clone` | CloneInsertionPass | `.clone()` を出力 |
| `needs_borrow` | BorrowInsertionPass | 仮引数型に `&` を追加 |
| `none_type_hint` | TypeConcretizationPass | 明示的な型付きで `None::<T>` を出力 |
| `repr_c` | CodegenOptions | struct/enum に `#[repr(C)]` を追加 |

<a id="rule-priority"></a>

### 規則の優先順位

ガード付き規則はガードなしの既定規則より先に検査する。最初にマッチした規則が選ばれる。TOML ローダーは構成要素ごとにガード付き規則を既定規則より前に並べる。

<a id="template-placeholders"></a>

### テンプレートのプレースホルダー

`{name}` は対応する束縛で置き換える。`{{` と `}}` はリテラルの波括弧のエスケープである。未知のプレースホルダーはそのまま残す。

<a id="key-template-constructs-rust"></a>

### 主要なテンプレート構成要素（Rust）

Rust テンプレートセット（`crates/almide-codegen/templates/rust.toml`）は以下の構成要素を定義する。

| 分類 | 例 |
|----------|----------|
| 式 | `if_expr`, `call_expr`, `binary_op`, `field_access`, `index_access`, `pipe_call` |
| Option/Result | `some_expr`, `none_expr`, `ok_expr`, `err_expr`, `unwrap_expr`, `unwrap_or_expr`, `try_expr`, `to_option_expr` |
| 等値 | `eq_expr` (`almide_eq!`), `ne_expr` (`almide_ne!`) |
| 連結 | `concat_expr` (型による振り分け: String と List) |
| べき乗 | `power_expr` (型による振り分け: `.pow()` と `.powf()`) |
| リテラル | `int_literal` (`{value}i64`), `float_literal` (`{value}f64`), `string_literal` (`"{value}".to_string()`), `list_literal` (`vec![{elements}]`) |
| 文 | `let_binding`, `var_binding`, `assignment` |
| 宣言 | `fn_decl`, `effect_fn_decl`, `test_block`, `struct_decl`, `enum_decl` |
| 型 | `type_int` (`i64`), `type_string` (`String`), `type_list` (`Vec<{inner}>`), `type_map` (`HashMap<{key}, {value}>`) |
| ループ | `for_loop`, `while_loop`, `break_stmt`, `continue_stmt` |
| ラムダ | `lambda` (`move \|{params}\| {{ {body} }}`), `lambda_single` |
| マッチ | `match_expr`, `match_arm`, `pattern_some`, `pattern_ok`, `pattern_variant`, など |
| トップレベル | `top_let_const`, `top_let_lazy` (`LazyLock`), `top_let_thread_lazy` (クロージャーを保持する値のスレッドごとのスロット) |
| モジュール呼び出し | `module_call` (`almide_rt_{module}_{func}({args})`) |

<a id="5-walker"></a>

## 5. Walker

**ソース**: `crates/almide-codegen/src/walker/mod.rs`

walker は型付き IR を走査し、テンプレートで描画する。完全にターゲット非依存であり、`if target == Rust` 検査はゼロである。ターゲット差はすべてパスとテンプレートで処理する。

<a id="rendercontext"></a>

### RenderContext

```rust
pub struct RenderContext<'a> {
    pub templates: &'a TemplateSet,
    pub var_table: &'a VarTable,
    pub indent: usize,
    pub target: Target,
    pub auto_unwrap: bool,          // true inside effect fn (not test)
    pub ann: CodegenAnnotations,
    pub type_aliases: HashMap<Sym, Ty>,
    pub minimal_generic_bounds: bool, // Clone only for bundled .almd modules
    pub repr_c: bool,               // Emit #[repr(C)] on structs/enums
}
```

<a id="rendering-functions"></a>

### 描画関数

以下のサブモジュールに分割されている。

| モジュール | 関数 | 描画対象 |
|--------|----------|---------|
| `walker/expressions.rs` | `render_expr()` | 式（部分式を再帰的に描画） |
| `walker/statements.rs` | `render_stmt()`, `render_pattern()` | 文、match パターン |
| `walker/types.rs` | `render_type()` | 型注釈（名前付きレコード、ジェネリクス、タプル） |
| `walker/declarations.rs` | `render_type_decl()` | struct、enum、別名宣言 |
| `walker/mod.rs` | `render_function()`, `render_program()` | 関数宣言、プログラム全体の組み立て |
| `walker/helpers.rs` | `terminate_stmt()`, `ty_contains_name()` | ユーティリティ |

<a id="program-rendering-order"></a>

### プログラムの描画順序

`render_program()` は以下の順に出力を組み立てる。

1. 匿名レコードの struct 定義
2. 型宣言（struct、enum、別名）
3. トップレベルの let（const と lazy）
4. テスト以外の関数
5. テスト関数（`mod tests { use super::*; ... }` で包む）
6. インポートしたモジュール関数（`almide_rt_{module}_{func}` を接頭辞にする）

<a id="function-name-sanitization"></a>

### 関数名のサニタイズ

関数名はターゲットとの互換性のためサニタイズされる。空白、ドット、ハイフンはアンダースコアになる。特殊文字（`+`、`/`、`*`、`(`、`)` など）は説明的な名前（`_plus_`、`_div_`、`_mul_` など）で置き換える。ターゲット固有キーワードは `keyword_escape` テンプレートでエスケープする（Rust: `r#name`）。

テスト関数名は `__test_almd_<NNNN>_<label>`（`almide_ir::test_fn_name`）となる。接頭辞で実関数と区別し、宣言順の通し番号により、サニタイズ後に同じ綴りになるラベルの 2 テスト（`"a b"` / `"a_b"`、#3488）を区別する。

<a id="6-rust-target-specifics"></a>

## 6. Rust ターゲットの詳細

**ソース**: `crates/almide-codegen/src/lib.rs`, `emit_source()`

<a id="preamble"></a>

### 前置部分

すべての Rust 出力は以下から始まる。

1. `#![allow(...)]` — 未使用警告を抑制
2. `use std::collections::{HashMap, HashSet};`
3. `AlmideConcat` トレイト — String と Vec の連結のために `+` をオーバーロード
4. `almide_eq!` / `almide_ne!` マクロ — 深い等値比較
5. ランタイムモジュール（ユーザーコードから参照されるものだけ）

<a id="runtime-inclusion"></a>

### ランタイムの取り込み

ランタイムモジュールは `include_str!`（`build.rs` が生成）によりコンパイラバイナリに埋め込まれる。出力時、エミッターはユーザーコードの `almide_rt_{module}_` パターンを走査し、参照されたモジュールだけを含める。競合を避けるため、ランタイムソースから `mod tests { ... }` ブロックを除去する。

ランタイムのモジュール間依存を解決する（例: `json` は `value` に依存）。

```almd
// Using list.map triggers inclusion of the list runtime module
import list

fn main() =
  let xs = [1, 2, 3]
  let doubled = xs.map((x) => x * 2)
  println(doubled)
```

```rust
// Output includes list runtime functions (almide_rt_list_*)
// but NOT string, map, json, etc.
```

<a id="parameter-passing-on-the-native-target"></a>

### ネイティブターゲットでの引数渡し

**ソース**: `pass_borrow_inference_ownership.rs` (`param_borrow`), `pass_borrow_lowering.rs`.

借用可能な型（`String`、`List[T]`、レコード）の非 `mut` 仮引数は、本体が値の所有を必要としない限り**参照渡し**（`&str` / `&[T]` / `&T`）になる。本体が所有を必要とするのは以下の場合である。

- 仮引数を返す、連結する、レコード／リスト／タプルリテラルに組み込む、match の対象にする、fold の初期値にする。
- クロージャーで捕捉する（クロージャーは借用を保持できない `Rc<dyn Fn>` を通して捕捉するため、捕捉は move か、仮引数を再度読む場合は clone になる）。
- 消費する **stdlib** の引数位置（`list.push` の要素）へ渡す。これらは値を受け取るランタイム呼び出しに lowering される。
- **要素**の所有を必要とする本体で反復する（下記）。`for` ループ、または融合パスがインライン化するリストコンビネーターが該当する。

**反復元の扱いは、コンビネーターの引数位置ではなく要素に従う**（#2287）。`list.map`、`fold`、`filter`、`any` などはリストの引数位置に `@consume(xs)` を宣言するが、これは融合しないランタイム側の対応物を表す。融合したチェーンと `for` ループは、本体が各要素に何を行うかから反復元のモードを決める（`element_reads_only`。借用判定、clone パス、所有権証明器が読む共通の規則）。

- `Copy` 要素（`List[Int]`）は反復元の所有を決して必要としない。`mapped(ns:
  List[Int]) = ns |> list.map((n) => n * 2)` は `ns.iter().cloned()` 上の `mapped(ns: &[i64])` となり、リストを 2 回読む呼び出し元も何もクローンしない。
- 受け取るすべてのラムダが読むだけのヒープ要素（借用する呼び出し引数位置、フィールドの読み取り、clone、`&str` 比較、補間の一部、または同じ反復内で束縛が再度読まれるため clone パスがいずれにせよクローンする消費用途）は、反復元を借用のままにし、`xs.iter()` から `&T` を束縛する。`lens(ws: &[String]) = ws.iter().map(|w| len(w.as_str()))` で要素はクローンされない（Rust が `&&T` を渡す `filter` 系の段階は、`let w = w.clone()` ではなく `let w = *w` で再束縛する）。
- 所有された形でチェーンを出る要素（map の値として返す、連結する、値に組み込む、所有する引数位置へ渡す、`filter` / `find` で収集する）は、反復元を消費のままにする。`ws.into_iter()` 上の `shouted(ws: Vec<String>)` となり、呼び出し元での最後の使用がリストを move し、借用元なら clone しかできない各要素も追加コストなしに move される。

これは参照カウント型コンパイラが適用する要素ごとの規則（リストの読み取りはリスト内部を借用し、出現箇所が要求する場合だけ要素を保持する）を、反復元のモードへ持ち上げたものである。ネイティブ側では借用した反復元からは clone しかできないためである。

ストリーム融合パスがインライン化するリストコンビネーターのコールバック（`list.map(xs, (x) =>
… t …)`、`filter`、`fold`、`any` など）は、この方針では**クロージャーではない**。このパスは借用パスより前に実行され、チェーンの各段階はチェーン内部で要素ごとに 1 回実行され、そこから逃れないスコープとなる。読むだけの仮引数は `&T` のまま（`(x) => x + list.len(t.names)` は `t: &Table` を保つ）であり、その段階は `move` も捕捉束縛もなしで描画し、チェーンの存続中だけ読んだものを借用する。段階内で消費する読み取り（要素ごとに `t` を返す）は、仮引数が所有されているかどうかに関係なくその場でクローンされるため、仮引数の所有には利益がない。呼び出しより長く生きるクロージャー（返す、保存する、別のクロージャーで捕捉する、呼び出し先が保持する引数位置へ渡す）だけが捕捉し、捕捉は所有する（Lean、Koka、Roc も同じ区別を行う。実在するクロージャーは保持するものを所有し、利得は所有権を決める前にクロージャーを消去することで得られる）。

**関数型の仮引数は、その呼び出し可能値が逃れる場合を除き借用される**（#2288）。ユーザー高階関数の `f: (A) -> B` 仮引数は、すべての出現箇所が呼び出すだけ、借用するだけ、または別の関数の借用関数引数位置へ渡すだけなら `&dyn Fn(A) -> B` となる（`&T` を決めるのと同じ不動点で、未確定または自己再帰の呼び出し先には楽観的）。呼び出し可能値が呼び出しより長く生きる出現箇所（返す、ローカルに束縛する、レコード／リストに組み込む、クロージャーに捕捉する、所有する引数位置へ渡す。未融合の `list.map` のようなランタイムの対応物や、保存されるチェーンコールバック）がある場合は `Rc<dyn Fn>` ハンドルとなる。借用引数位置にあるラムダリテラルは、そこでチェーン段階と同じスコープとなる。`apply(xs, &|x| x * k)` — `Rc::new`、`move`、捕捉束縛はなく、読む仮引数は借用のままである。その位置にクロージャーの**値**を渡す場合はハンドルを通じて貸す（`&*g`）。末尾再帰関数は、`&Vec<u8>` と同じように借用した呼び出し可能値をループ内で引き継ぐ。これはクロージャーを持たない参照実装が通常の引数として渡す借用捕捉レコードであり、lambda sets は使わない。関数型仮引数そのものが ABI であり、escape 判定が本体に必要な 2 形式のどちらかを決める。所有権証明器の C5 は最終 IR 上で判定を再読する（逃れる `&dyn Fn` 仮引数、借用引数位置の boxed クロージャー）。`ALMIDE_FN_ESCAPE_OFF=1` がその陰性対照である。

同じ寿命の `let` 束縛クロージャーもスコープである（#3455）。その束縛のすべての出現がこの規則で許されるもの（呼び出し、借用、`&dyn Fn` 引数位置への貸し出しであり、返す、保存する、より長く生きるクロージャーに捕捉する、`fan` アームで使うことはない）であり、捕捉するすべての変数が一度も書き込まれず、`var` もなく、ヒープ値については関数内のどこでも消費されない場合、束縛は非 `move` の `let f = &|i: i64| …` となる。読み取りは借用となり、読む仮引数は `&T` のままで、呼び出しのために実引数をクローンする呼び出し元はない。判定は各借用ラウンド内の最大不動点で、確定後は `Borrow { Lambda }` と表されるため、`CaptureClone` は `__cap_*` 束縛を追加しない。TCO 対象の関数は除外する。`ALMIDE_SCOPED_CLOSURE_OFF=1` は所有する捕捉に戻す。

仮引数をそのまま呼び出し先の所有する引数位置へ渡す（`stored` が `t` をレコード内に保持する `stored(t, 1)`）ことで仮引数が所有されるのは、その箇所が仮引数の**最後**の使用である場合だけである。その場合は所有によって move が可能になり、まだ値を必要とする呼び出し元は 1 回クローンする。その箇所より後でも仮引数が読まれる場合（`{ let h
= stored(t, 1); h.n + plain(t) }`）、仮引数が所有されているかどうかにかかわらずその箇所でクローンする。したがって仮引数は `&T` のまま、その 1 箇所が自身の読み取りを所有する（`stored(t.clone(), 1)`）。その箇所で 1 クローン、呼び出し元ではゼロである。割り当て台帳（`tests/native_borrow_oracle_test.rs`）は代替策のコストを固定して検証する。生存性に関係なくそのような全箇所でクローンする方式（#2278 の初稿）は、固定した 5 プログラムすべてで回数を増やした。`mut` 仮引数は本体全体で `&mut T` であり、すべての値渡し位置で同じ箇所単位のクローンを行う（#2266）。

**バリアント**仮引数（`s: Shape`）も同じ方針に従うが、`match` には 1 規則がある。アームが導入する各束縛が読むだけ（`Copy` スカラー、またはすべての出現が非消費位置であるヒープペイロード。借用実引数、メンバー読み取り、比較、補間の一部、さらに別の match の対象）なら、対象位置は仮引数を参照で読む。アームは `&T` ペイロードを束縛し（Rust の既定束縛モード。スカラー読み取りは逆参照し、ヒープペイロードの `&b` は束縛そのものになり、boxed ペイロードは再び `&**b` としてマッチする）、呼び出し元は値を渡すためのクローンを行わない。ペイロードが 1 つでも返される、連結される、値に組み込まれる、捕捉される、所有する引数位置へ渡されるなら仮引数は所有のままとなる。値による match は、借用 match ならクローンするペイロードを追加コストなしに move して取り出す。所有権証明器の C4 は同じ述語を読むため、match が読むだけなのに所有されるバリアント仮引数は名前の付いた欠陥として扱われる（`tests/variant_param_borrowed_test.rs`）。

所有される仮引数は、その値を引き続き必要とするすべての呼び出し元に 1 クローンのコストを課す。借用仮引数は呼び出し時にコストを課さない。関数が最終的に `&T` と `T` のどちらになるかは出力 Rust（`almide app.almd --target rust`）で確認できる。所有権証明器（`ALMIDE_CERTIFY_OWNERSHIP=report`）は、所有を必要とする出現箇所がないのに所有されている仮引数を名指しする。

<a id="effect-functions"></a>

### Effect 関数

成功型 `T` を宣言するユーザーの `effect fn` は、失敗可能な Result インターフェースを持つ（既定エラー型は `String`）。明示的に宣言した Result はそのエラー型を保持する。ソース上の伝播は明示的である。`!` を書くか、ADR-0008 に従って Result を処理／破棄する。フロントエンドはその意図を IR に lowering し、ネイティブの `ResultPropagationPass` がシグネチャ、末尾、呼び出しを Rust 表現へ適応させる。ソースコードの暗黙的伝播を復活させるものではない。以下の Rust は模式的なもので、ランタイムのラッパー型は省略している。

```almd
effect fn read_config(path: String) -> String =
  let content = fs.read_text(path)!
  content
```

```rust
pub fn read_config(path: String) -> Result<String, String> {
    let content: String = almide_rt_fs_read_text(&path)?;
    Ok(content)
}
```

<a id="generic-bounds"></a>

### ジェネリック境界

ジェネリック境界は 2 段階ある。

- **完全**（ユーザー関数）: `T: Clone + std::fmt::Debug + PartialEq + PartialOrd`
- **最小限**（同梱 .almd モジュール関数）: `T: Clone`

<a id="7-wasm-target-not-in-this-crate"></a>

## 7. WASM ターゲット（この crate の外）

**ソース**: `src/wasm_route.rs`, `crates/almide-wasm`

CLI とライブラリ利用側は 1 つの経路を共有する。リンク済みの型付き IR は構造的エミッターを経由して Wasm バイト列になり、MIR-to-WAT 描画や CLI の `wat` アセンブル段階を経由しない。対応範囲外のプログラムは名前の付いた wall（CLI では `E082`）となり、廃止されたレンダラーへのフォールバックはない。ブラウザーランナーが使う stock-WASI 形式は `almide_wasi::to_wasi` で導出する。ランタイムインポートと self-host 標準ライブラリ本体はリンク済みプログラムの一部であり、別のレンダラーを選ぶ理由にはならない。

詳細: [../wasm/WASM-OUTPUT.md](../../../../wasm/WASM-OUTPUT.md)、[../ARCHITECTURE.md](../../../../ARCHITECTURE.md)、[crates/CLAUDE.md](../../../../../crates/CLAUDE.md)。

<a id="8-codegenoptions"></a>

## 8. CodegenOptions

**ソース**: `crates/almide-codegen/src/lib.rs`

```rust
pub struct CodegenOptions {
    pub repr_c: bool,  // Emit #[repr(C)] on structs/enums for stable C ABI layout
}
```

`repr_c` が true の場合、`struct_decl` と `enum_decl` テンプレートは `when_attr = "repr_c"` バリアントを選び、derive ブロックの前に `#[repr(C)]` を付ける。

<a id="9-module-function-naming"></a>

## 9. モジュール関数の命名

**ソース**: `crates/almide-codegen/src/walker/mod.rs`, `render_program()`

インポートしたモジュール関数は、接頭辞付きの名前で出力する。

```
fn almide_rt_{module}_{function}(...)
```

モジュール識別子は `IrModule.versioned_name`（設定されている場合）または `IrModule.name` から取り、ドットをアンダースコアに置き換える。例えば、バージョン `1.0` のモジュール `string` に `versioned_name = "string_1_0"` があれば、`almide_rt_string_1_0_trim(...)` を生成する。

テンプレート `module_call` が呼び出し箇所を処理する。

```toml
[module_call]
template = "almide_rt_{module}_{func}({args})"
```

`StdlibLoweringPass` は build.rs が生成した `arg_transforms` テーブルに基づく IR レベルの引数装飾（BorrowStr、BorrowRef、ToVec、LambdaClone、Direct）を用いて、`CallTarget::Module { module: "list", func: "map" }` を `CallTarget::Named { name: "almide_rt_list_map" }` に書き換える。

<a id="10-codegenannotations"></a>

## 10. CodegenAnnotations

**ソース**: `crates/almide-codegen/src/annotations.rs`

注釈は nanopass の各パスが設定し、walker が読む。walker が型や文脈を直接検査することはない。

```rust
pub struct CodegenAnnotations {
    pub lazy_vars: HashSet<VarId>,                          // Top-level lazy vars (need *DEREF)
    pub ctor_to_enum: HashMap<String, String>,              // Constructor -> enum name (Red -> Color)
    pub anon_records: HashMap<Vec<String>, String>,          // Field names -> generated struct name
    pub named_records: HashMap<Vec<String>, String>,         // Field names -> declared type name
    pub recursive_enums: HashSet<String>,                    // Enums with recursive variants
    pub boxed_fields: HashSet<(String, String)>,             // (ctor, field) pairs needing Box::new()
    pub default_fields: HashMap<(String, String), IrExpr>,   // Default field values for constructors
}
```

<a id="11-targetattrs"></a>

## 11. TargetAttrs

**ソース**: `crates/almide-codegen/src/pass.rs`

パスが設定し、テンプレートレンダラーが使うノードごとの属性:

```rust
pub struct TargetAttrs {
    pub needs_try: bool,             // Append ? for auto-propagation
    pub needs_clone: bool,           // Emit .clone()
    pub needs_borrow: bool,          // Emit & reference
    pub needs_box: bool,             // Wrap in Box<T>
    pub none_type_hint: Option<String>, // None::<T> with explicit type
    pub match_as_str: bool,          // .as_str() on match subject
    pub lazy_init: bool,             // Top-level let -> LazyLock
}
```
