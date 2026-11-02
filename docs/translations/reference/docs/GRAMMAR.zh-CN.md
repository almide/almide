<a id="almide-grammar-ebnf"></a>

# Almide 语法（EBNF）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](GRAMMAR.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](GRAMMAR.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](GRAMMAR.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/GRAMMAR.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本，固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不会自动同步。以原文及其指定的实现解析器为准，本译文不是独立的语法验证或现行保证。代码、EBNF 和注释保持原样。

这是一份描述性快照：接受哪些语法，以实现中的解析器为准（[crates/almide-syntax](https://github.com/almide/almide/tree/develop/crates/almide-syntax)，由本仓库的语料和 AST 一致性门禁进行测试）；机器可读语法位于 [almide/almide-grammar](https://github.com/almide/almide-grammar)。
本文基于 v0.34.x 解析器编写，之后随语法变化持续修订。
终结符：`IDENT`（小写字母开头）、`TYPENAME`（大写字母开头）、`INT`、`FLOAT`、`STRING`。
关键字可通过反引号转义用作标识符：`` `type` ``。

<a id="declarations"></a>

## 声明

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

- 可见性：默认为公开；`pub` 是显式、不改变行为的同义写法。`mod` = 同一项目，`local` = 同一文件。不接受 `pub type`（类型仅接受 `local`/`mod`）。
- `effect fn` 标记效应调用，并具有可失败的结果通道；其错误类型不一定是 String（`T!E` 表示 `Result[T, E]`）。后缀 `!` 是显式的，其传播／解包行为取决于外围函数或 lambda 的上下文，包括测试块规则。请查阅 [上下文表](../../../specs/result-option-effect.md)，而不要把每个 `!` 都当作 Rust 的 `?`。不带 `!` 的调用保留 Result/Option 值；E041 和 E042 覆盖诊断参考中描述的无注解绑定及丢弃情形。

<a id="types"></a>

## 类型

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

`?` 附着在具名或带括号的类型原子上；`(A) -> B?` 返回 Option[B]，而 `((A) -> B)?` 是可选函数。嵌套 Option 使用 `(T?)?`，而不是 `T??`。返回标记 `!E` 及其可选的具名错误类型必须位于同一行；它不是适用于所有类型的通用后缀标记。

<a id="expressions"></a>

## 表达式

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

## 模式

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

match 分支支持 or 备选项（`a | b => ...`）、as 模式（`whole @ pattern`），以及列表末尾的剩余模式（`[h, ..tail]` 或 `[h, ..]`）。
不接受范围模式、三点剩余写法（`[h, ...tail]`）或 `head :: tail`。剩余模式后面不能再跟其他模式或尾随逗号。

<a id="literals"></a>

## 字面量

- Int：使用 `_` 分隔符的十进制（`1_000`）、十六进制 `0xFF`、二进制 `0b1010`、八进制 `0o17`；进制前缀不区分大小写，数字可使用 `_` 分隔符。
- Float：`3.14`、科学记数法 `1e9` / `2.5E-3`、`_` 分隔符。
- 字符串：`"..."`（转义 + `${expr}` 插值）、`'...'`（仅转义，无插值）、`"""..."""` heredoc（插值 + 去除共同缩进）、`r"..."` / `r"""..."""` 原始字符串（无转义，无插值）。转义：`\n \t \r \\ \" \$ \xNN \u{...}`。
- 集合：列表 `[1, 2]` / `[]`；映射 `["a": 1]`，空映射 `[:]`；元组 `(a, b)`；unit `()`。
- 注释：`//` 行注释、`/* ... */` 块注释（可嵌套）。

<a id="operator-precedence"></a>

## 运算符优先级

从最松到最紧。比较运算符不具结合性——`a < b < c` 是解析错误（应使用 `and`）。

| 层级 | 运算符 | 结合性 |
|---|---|---|
| 1 | `or` | 左 |
| 2 | `and` | 左 |
| 3 | `==` `!=` `<` `>` `<=` `>=` | 不结合 |
| 4 | `\|>` | 左（不对称——见下文） |
| 5 | `..<` `...` | — |
| 6 | `+` `-` | 左 |
| 7 | `*` `/` `%` | 左 |
| 8 | `^`（别名 `**`）乘方 | 右 |
| 9 | `>>` 组合 | 左（最紧的二元运算符） |
| 10 | 一元 `-`、`not` | 前缀 |
| 11 | 后缀：`()` `[]` `.` `!` `?` `?.` `??` | — |

**`|>` 不对称**：其右侧是单个后缀／组合链。右侧之后的其他二元运算符作用于整个管道结果：

- `xs |> list.map(f) + ys` = `(xs |> list.map(f)) + ys`
- `xs |> f >> g` = `xs |> (f >> g)`（只有 `>>` 嵌套在右侧内部）
- `xs |> list.len > 5` = `(xs |> list.len) > 5`
- `a + b |> f` = `(a + b) |> f`

`x ?? fallback` 只接受一元表达式作为回退：`x ?? 0 + 1` = `(x ?? 0) + 1`。

<a id="stdlib-imports"></a>

## 标准库导入

当前自动导入集合是 `crates/almide-frontend/src/import_table.rs` 中种子列表与 `crates/almide-types/src/stdlib_info.rs` 中 `AUTO_IMPORT_BUNDLED` 的并集。自动导入使模块名可用，而不是使模块的所有函数都可通过非限定名称使用。`prim` 仅限标准库源码使用，不是面向用户的公开导入。旧 `error` 模块已移除。

当前 API 和导入示例请使用 [标准库参考](../../../stdlib/README.md) 和 [CHEATSHEET](../../../CHEATSHEET.md)，而不是在此语法快照中另行维护模块列表。

<a id="removed--rejected-forms"></a>

## 已移除／被拒绝的形式

以下形式仅为了产生针对性的诊断而进行词法或语法解析，并非语言的一部分：

- `++`——已移除；`+` 连接字符串和列表
- `&&` / `||` / 前缀 `!`——使用 `and` / `or` / `not`
- `fn (x) => e` lambda——使用 `(x) => e`
- `impl` 块——使用约定方法：`fn Type.method(self, ...)`
- `deriving`——使用约定子句：`type Name: Eq, Repr = ...`
- `do`、`while ... do ... done`、`let ... in`、`let mut`、`let rec`
- `return`、`class`、`null`、`unsafe`、`newtype`

<a id="notes"></a>

## 注记

- 恰好四个关键字具有大小写别名：`Ok`/`ok`、`Err`/`err`、`Some`/`some`、`None`/`none`。`true`/`false`/`todo` 仅允许小写。
- `ok/err/some/none/todo` 是软关键字：可用作字段／成员名，但不能用作绑定名。
- 所有错误均通过 `Result[T, E]`，所有可选值均通过 `Option[T]`。
- 没有 `else` 的 `if` 返回 Unit；块求值为其末尾表达式的值。
- 运算符可以出现在续行开头；但行首的 `-` 若紧贴其操作数（`-1`），则会开始一条新语句。
