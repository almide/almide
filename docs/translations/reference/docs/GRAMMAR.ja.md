<a id="almide-grammar-ebnf"></a>

# Almide 文法（EBNF）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](GRAMMAR.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](GRAMMAR.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](GRAMMAR.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/GRAMMAR.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットで、自動同期しない。原文とその指定する実装パーサーが正本であり、この翻訳は独立した構文検証や現行保証ではない。コード・EBNF・コメントは原文のまま保持する。

これは記述的なスナップショットである。受理される構文の正本は実装パーサー（[crates/almide-syntax](https://github.com/almide/almide/tree/develop/crates/almide-syntax)。このリポジトリのコーパスと AST 同等性ゲートで検証される）であり、機械可読文法は [almide/almide-grammar](https://github.com/almide/almide-grammar) にある。
v0.34.x のパーサーを基に書かれ、その後も構文変更に合わせて修正されている。
終端記号: `IDENT`（小文字始まり）、`TYPENAME`（大文字始まり）、`INT`、`FLOAT`、`STRING`。
キーワードはバッククォートでエスケープすると識別子として使える: `` `type` ``。

<a id="declarations"></a>

## 宣言

```ebnf
program     = module_decl? import* decl*
module_decl = "module" dotted_path                        (* legacy, optional *)
import      = "import" dotted_path
              ( "." "{" name ("," name)* ","? "}"         (* selective: import mod.{ A, b } *)
              | "as" IDENT )?                             (* alias: import self as app *)
dotted_path = IDENT ("." IDENT)*                          (* import pkg.submodule *)

decl        = type_decl | fn_decl | protocol_decl | top_let | test_decl

type_decl   = ("local" | "mod")? "type" TYPENAME generic_params?
              (":" TYPENAME ("," TYPENAME)*)?             (* conventions: type Name: Eq, Repr *)
              "=" type_body

fn_decl     = attr* "pub"? ("local" | "mod")? "effect"? "fn" fn_name
              generic_params? "(" params ")" "->" return_type ("=" fn_body)?
fn_name     = IDENT | TYPENAME "." IDENT                  (* convention method: fn Point.norm(self, ...) *)
fn_body     = expr | braceless_block                      (* body may start with let/var/guard directly *)
params      = ("self" ","?)? (param ("," param)* ","?)?
param       = attr* "mut"? IDENT ":" type ("=" expr)?     (* default args: once one has =, the rest must *)
attr        = "@" IDENT ("(" attr_arg ("," attr_arg)* ")")?   (* @extern(...), @export(...), @inline_rust *)

protocol_decl   = "protocol" TYPENAME generic_params? "{" protocol_method* "}"
protocol_method = "effect"? "fn" fn_name generic_params? "(" params ")" "->" return_type

top_let     = "pub"? ("local" | "mod")? ("let" | "var") name (":" type)? "=" expr
test_decl   = "test" STRING where_clause* block           (* where: table cases / binds / call stubs *)
generic_params = "[" gparam ("," gparam)* "]"
gparam      = TYPENAME (":" (TYPENAME ("+" TYPENAME)* | "{" fields "}"))?   (* bounds; structural bound *)
```

- 可視性: 既定は公開。`pub` は明示的な、動作を変えない同義表記である。`mod` = 同じプロジェクト、`local` = 同じファイル。`pub type` は受理されない（型に使えるのは `local`/`mod` のみ）。
- `effect fn` はエフェクト呼び出しを示し、失敗可能な結果チャネルを持つ。エラー型は必ずしも String ではない（`T!E` は `Result[T, E]` を表す）。後置 `!` は明示的であり、その伝播／unwrap の動作は、テストブロックの規則を含め、囲んでいる関数またはラムダの文脈で決まる。すべての `!` を Rust の `?` とみなすのではなく、[文脈の表](../../../specs/result-option-effect.md) を参照する。`!` のない呼び出しは Result/Option 値を保持する。E041 と E042 は、診断リファレンスで説明する無注釈の束縛と破棄の場合を扱う。

<a id="types"></a>

## 型

```ebnf
type_body   = record_type | variant_type | type            (* otherwise: alias *)
record_type = "{" field ("," field)* ","? "}"
field       = attr* name ("as" STRING)? ":" type ("=" expr)?  (* serialize alias; field default *)
variant_type= "|"? variant ("|" variant)+                  (* leading | optional *)
variant     = TYPENAME
            | TYPENAME "(" type ("," type)* ")"            (* tuple payload *)
            | TYPENAME "{" field ("," field)* "}"          (* record payload *)

type        = named_type "?"?                          (* Int, Float, String, Bool, Unit, List[T], ... *)
            | "{" field ("," field)* (",")? ".."? "}"      (* record / open record { a: Int, .. } *)
            | "(" ")"                                      (* Unit *)
            | "(" type ("," type)* ")" "?"?               (* grouped / tuple type; optional suffix *)
            | "effect"? fn_type                           (* function slot; optional effect prefix *)
            | INT                                          (* const literal: Array[Float, 128] *)
named_type  = (IDENT ".")* TYPENAME type_args?
fn_type     = ("fn" | "Fn")? "(" (type ("," type)*)? ")" "->" return_type
return_type = type ("!" named_type?)?                      (* T! = Result[T,String]; T!E = Result[T,E] *)
type_args   = "[" type ("," type)* "]"                     (* generics use [] not <> *)
```

`?` は名前付きまたは括弧付きの型アトムに付く。`(A) -> B?` は Option[B] を返す一方、`((A) -> B)?` はオプショナルな関数である。入れ子の Option は `T??` ではなく `(T?)?` を使う。戻り値マーカー `!E` と省略可能な名前付きエラー型は、同じ行に置かなければならない。これはあらゆる型に付けられる一般的な後置マーカーではない。

<a id="expressions"></a>

## 式

```ebnf
expr        = if_expr | if_let | match_expr | for_in | while_expr | fan_expr
            | lambda | binary | range | pipe | postfix | primary
block       = "{" (stmt ";"?)* expr? "}"                   (* trailing expr is the block value *)
stmt        = let_stmt | var_stmt | guard_stmt | assign | expr
let_stmt    = "let" ("_" | IDENT) (":" type)? "=" expr
            | "let" "(" tuple_pat ")" "=" expr             (* tuple destructuring, nestable *)
            | "let" "{" IDENT ("," IDENT)* ","? "}" "=" expr   (* record destructuring *)
var_stmt    = "var" IDENT (":" type)? "=" expr
            | "var" "(" tuple_pat ")" "=" expr             (* the let patterns; every bound name is a var *)
            | "var" "{" IDENT ("," IDENT)* ","? "}" "=" expr
tuple_pat   = (IDENT | "_" | "(" tuple_pat ("," tuple_pat)* ")") ("," tuple_pat)*
assign      = IDENT ("." name)* ("[" expr "]")? "=" expr (":" type)?
            (* targets: `x = v`, `s.f = v`, `o.inner.xs = v`, `xs[i] = v`, `o.m[k] = v`.
               An index can only be the LAST step: `ps[0].n = v` is refused at the
               target with the read-modify-store rewrite. The value's trailing
               ascription (`s.xs = []: List[Int]`) is optional — the target's own
               type already fixes an empty literal. *)
guard_stmt  = "guard" expr "else" expr
            | "guard" "let" IDENT "=" expr "else" expr     (* bind Option, else on none *)

if_expr     = "if" expr "then" if_branch ("else" if_branch)?  (* no else => Unit *)
if_branch   = assign | expr                                (* any `assign` target, unbraced:
                                                              `if c then o.ui.xs[i] = v`; an
                                                              assignment branch is Unit *)
if_let      = "if" "let" IDENT "=" expr block "else" block (* else required, branches braced *)
match_expr  = "match" expr "{" (arm ","?)* "}"
arm         = pattern ("|" pattern)* ("if" expr)? "=>" expr  (* or-alternatives + optional guard *)
for_in      = "for" (binder | "(" binder ("," binder)* ")") "in" expr block
binder      = IDENT | "_"
while_expr  = "while" expr block
fan_expr    = "fan" "{" (expr ","?)+ "}"                   (* concurrent arms → tuple; `,`/newline between, never `;` *)
            | "fan" "{" (IDENT ":" expr ","?)+ "}"         (* record form: concurrent fields → record (#1388) *)
lambda      = "(" (lparam ("," lparam)*)? ")" "=>" expr    (* the ONLY lambda form *)
lparam      = (IDENT | "_") (":" type)?
            | "(" (IDENT | "_") ("," (IDENT | "_"))* ")"   (* tuple-destructuring param *)

pipe        = expr "|>" expr                               (* also: expr |> match { arms } *)
range       = expr "..<" expr | expr "..." expr            (* exclusive / inclusive *)
postfix     = primary ( "(" args ")"                       (* call *)
                      | type_args "(" args ")"     (* explicit type args: f[Int](x) *)
                      | "[" expr "]"                       (* index *)
                      | "." IDENT | "." INT                (* field access; tuple index t.0 *)
                      | "!"                                (* context-dependent propagation/unwrap; see context table *)
                      | "?"                                (* Result -> Option *)
                      | "?." IDENT                         (* optional chaining *)
                      | "??" unary )*                      (* fallback; RHS binds at unary level *)
args        = ((expr | IDENT ":" expr) ",")* ...           (* named args; partial application is rejected as E046 *)
primary     = literal | IDENT | TYPENAME | "(" expr ")" | "(" expr ("," expr)+ ")"
            | "(" ")" | block | record_lit | "todo" "(" STRING ")"
            | "break" | "continue" | "_"                   (* _ = typed hole in expr position *)
            | "none" | "some" "(" expr ")" | "ok" "(" expr ")" | "err" "(" expr ")"
record_lit  = "{" (fieldinit ("," fieldinit)* ","?)? "}"   (* anonymous *)
            | (IDENT ".")? TYPENAME "{" fieldinit* "}"     (* named; module-qualified *)
            | (IDENT ".")? TYPENAME? "{" "..." expr ("," fieldinit)* "}"  (* spread: { ...base, x: 1 } *)
fieldinit   = name (":" expr)?                             (* shorthand: { x } = { x: x } *)
```

<a id="patterns"></a>

## パターン

```ebnf
pattern     = IDENT "@" pattern                           (* bind the whole matched value *)
            | "_" | IDENT | INT | FLOAT | STRING | "-" (INT | FLOAT)
            | "true" | "false"
            | "none" | "some" "(" pattern ")" | "ok" "(" pattern ")" | "err" "(" pattern ")"
            | (IDENT ".")? TYPENAME                        (* unit constructor; module-qualified *)
            | (IDENT ".")? TYPENAME "(" pattern ("," pattern)* ")"
            | (IDENT ".")? TYPENAME "{" field_pat ("," field_pat)* (".." ","?)? "}"
            | "(" pattern ("," pattern)* ")"               (* tuple *)
            | "[" (pattern ("," pattern)* ","?)? "]"      (* fixed-length list *)
            | "[" (pattern ",")* ".." IDENT? "]"           (* tail rest, last position only *)
field_pat   = name (":" pattern)?                          (* shorthand binds the field name *)
```

match アームは or 選択肢（`a | b => ...`）、as パターン（`whole @ pattern`）、リスト末尾の残余パターン（`[h, ..tail]` または `[h, ..]`）をサポートする。
範囲パターン、3 点の残余表記（`[h, ...tail]`）、`head :: tail` は受理されない。残余パターンの後に別のパターンや末尾カンマを置くことはできない。

<a id="literals"></a>

## リテラル

- Int: `_` 区切り付きの 10 進数（`1_000`）、16 進数 `0xFF`、2 進数 `0b1010`、8 進数 `0o17`。基数の接頭辞は大文字と小文字を区別せず、数字には `_` 区切りを使える。
- Float: `3.14`、科学表記 `1e9` / `2.5E-3`、`_` 区切り。
- 文字列: `"..."`（エスケープ + `${expr}` 補間）、`'...'`（エスケープのみ、補間なし）、`"""..."""` ヒアドキュメント（補間 + 共通インデントの除去）、`r"..."` / `r"""..."""` raw（エスケープなし、補間なし）。エスケープ: `\n \t \r \\ \" \$ \xNN \u{...}`。
- コレクション: リスト `[1, 2]` / `[]`、マップ `["a": 1]`、空のマップ `[:]`、タプル `(a, b)`、unit `()`。
- コメント: `//` 行コメント、`/* ... */` ブロックコメント（入れ子可）。

<a id="operator-precedence"></a>

## 演算子の優先順位

結合が弱いものから強いものへの順。比較演算子は非結合であり、`a < b < c` は構文解析エラーになる（`and` を使う）。

| レベル | 演算子 | 結合性 |
|---|---|---|
| 1 | `or` | 左 |
| 2 | `and` | 左 |
| 3 | `==` `!=` `<` `>` `<=` `>=` | 非結合 |
| 4 | `\|>` | 左（非対称 — 下記参照） |
| 5 | `..<` `...` | — |
| 6 | `+` `-` | 左 |
| 7 | `*` `/` `%` | 左 |
| 8 | `^`（別名 `**`）べき乗 | 右 |
| 9 | `>>` 合成 | 左（最も強い二項演算子） |
| 10 | 単項 `-`、`not` | 前置 |
| 11 | 後置: `()` `[]` `.` `!` `?` `?.` `??` | — |

**`|>` は非対称**であり、右辺は単一の後置／合成チェーンである。右辺の後にあるそれ以外の二項演算子は、パイプ結果全体に適用される。

- `xs |> list.map(f) + ys` = `(xs |> list.map(f)) + ys`
- `xs |> f >> g` = `xs |> (f >> g)`（右辺の内側に入るのは `>>` のみ）
- `xs |> list.len > 5` = `(xs |> list.len) > 5`
- `a + b |> f` = `(a + b) |> f`

`x ?? fallback` のフォールバックは単項式だけを取る: `x ?? 0 + 1` = `(x ?? 0) + 1`。

<a id="stdlib-imports"></a>

## 標準ライブラリのインポート

現在の自動インポート集合は `crates/almide-frontend/src/import_table.rs` の初期リストと、`crates/almide-types/src/stdlib_info.rs` の `AUTO_IMPORT_BUNDLED` の和集合である。自動インポートによって使えるようになるのはモジュール名であり、そのすべての関数を非修飾名で使えるようにするものではない。`prim` は標準ライブラリのソースに限定され、一般ユーザーが公開インポートとして使うものではない。旧 `error` モジュールは削除されている。

この構文スナップショット内で別のモジュール一覧を維持するのではなく、現在の API とインポート例は [標準ライブラリリファレンス](../../../stdlib/README.md) と [CHEATSHEET](../../../CHEATSHEET.md) を使う。

<a id="removed--rejected-forms"></a>

## 削除済み／拒否される形式

以下は対象に応じた診断を出すためだけに字句解析または構文解析されるものであり、言語の一部ではない。

- `++` — 削除済み。文字列とリストの連結には `+` を使う
- `&&` / `||` / 前置 `!` — `and` / `or` / `not` を使う
- `fn (x) => e` ラムダ — `(x) => e` を使う
- `impl` ブロック — 規約メソッド `fn Type.method(self, ...)` を使う
- `deriving` — 規約節 `type Name: Eq, Repr = ...` を使う
- `do`、`while ... do ... done`、`let ... in`、`let mut`、`let rec`
- `return`、`class`、`null`、`unsafe`、`newtype`

<a id="notes"></a>

## 注記

- 大文字／小文字の別名があるキーワードは厳密に 4 つ: `Ok`/`ok`、`Err`/`err`、`Some`/`some`、`None`/`none`。`true`/`false`/`todo` は小文字のみ。
- `ok/err/some/none/todo` はソフトキーワードであり、フィールド／メンバー名には使えるが、束縛名には使えない。
- すべてのエラーは `Result[T, E]`、すべてのオプショナル値は `Option[T]` を経由する。
- `else` のない `if` は Unit を返す。ブロックは末尾の式を評価した値になる。
- 演算子は継続行の先頭に置ける。ただし行頭の `-` がオペランドと接している場合（`-1`）は、代わりに新しい文が始まる。
